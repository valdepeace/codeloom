# Tareas enlazadas: un seguimiento es una tarea nueva (A11)

Estado: implementada el 2026-10-23 (véase «Estado» al final; tarea A11 de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`, Fase C de `2026-10-21-codeloom-agent-autonomy-design.md`).

## Propósito

Cuando un subagente termina y el padre quiere afinar o continuar su trabajo, hoy solo puede
lanzar otro hijo sin contexto y copiar a mano lo que hizo el anterior. A2A dice qué hacer, y el
diseño de autonomía ya lo anticipó con un `follow_up_of`. Esta tarea lo cierra.

## Lo que consta en A2A (literal, ya recogido en `2026-10-15-loom-agent-profile-a2a.md`)

- «Once a task reaches a terminal state (completed, canceled, rejected, or failed), it cannot
  restart.» Todo refinamiento «must initiate a new task within the same `contextId`».
- «Clients SHOULD use the `referenceTaskIds` field in Message to explicitly reference related
  tasks» (en el proto, `reference_task_ids`).
- «Task IDs are server-generated»; el `contextId` «logically groups multiple related Task and
  Message objects».

## Lo que NO consta (y por tanto decidimos)

- Qué debe hacer el servidor con las tareas referidas (el texto solo dice que el cliente SHOULD
  referenciarlas): que su resultado pase al prompt de la nueva es **decisión nuestra**.
- Si se puede referir a una tarea que no es terminal.

## Estado antes de esta tarea

`Message.reference_task_ids` existe en `a2a.rs`, pero nada lo escribe. `contextId` ya se deriva
(`SubagentDescriptor::context_id`) de la sesión del padre, así que todos los hijos de un padre lo
comparten. No hay forma de enlazar dos tareas.

## Decisiones (nuestras salvo indicación)

- **D1. `subagent` gana `follow_up_of: child_id`** (entero ≥ 1, opcional). El nombre es del diseño
  de autonomía; se conserva en la herramienta, que es la cara que ve el modelo, y se traduce al
  campo del protocolo en el almacén.
- **D2. Se guarda como `reference_task_ids`**, el nombre del proto, en el descriptor del hijo
  nuevo: `Vec<SessionId>` (el id de tarea es la sesión del hijo). Persistido en una columna JSON
  de `subagents`, con migración como la de `agent_origin`. Hoy la herramienta solo admite una
  referencia; el almacén admite varias porque el protocolo es una lista.
- **D3. Solo se puede referir a una tarea terminal** del mismo padre. Si no existe o es de otro
  padre: `TaskNotFound`. Si sigue viva: error de herramienta pidiendo esperar a que termine o usar
  `send_message` / `answer`. Una tarea viva se *continúa*; una terminal se *sigue*.
- **D4. El resultado de la tarea referida pasa al prompt del hijo nuevo**, antes de la petición,
  con el formato fijo:

  ```
  [Follow-up of subagent {id} "{label}" ({estado en TASK_STATE_*})]
  Previous result:
  {resultado}

  {prompt}
  ```

  El resultado va acotado a `MAX_NOTICE_BYTES` (8 000), igual que el aviso de finalización, por la
  regla 8 del decálogo (lo que cruza de un agente a otro se acota). Si la tarea no dejó resultado
  (falló o se canceló sin artefactos) se escribe `(no result)` y, si lo hay, el texto del mensaje
  de su estado.
- **D5. `contextId` no cambia**: el hijo nuevo es hijo del mismo padre y lo hereda por derivación.
  Un test lo fija.
- **D6. Los ids siguen siendo del servidor**: el modelo nunca elige el id de la tarea nueva.

## Fuera de alcance, declarado

- Mostrar el enlace en la interfaz (el nodo del árbol de subagentes no lo pinta todavía).
- Referir a varias tareas desde la herramienta, o a tareas de otra sesión.
- Restringir `send_message` a responder preguntas (segunda viñeta de la Fase C); sigue sin
  verificarse cómo relee el hijo su historial.
- El cable (`referenceTaskIds` en JSON): A4.

## Preguntas abiertas

- Si el resultado referido debe ir como mensaje aparte en el historial del hijo en lugar de
  prefijar el prompt. Se prefija porque es lo más simple y no depende de cómo el bucle relee el
  historial; es decisión nuestra y reversible.

## Estado

Implementada el 2026-10-23, **sin verla en una terminal real**: probada con proveedores y trabajos
simulados. Commits: `95bf005` (esta spec y su plan), `881b374` (el descriptor guarda
`reference_task_ids`), `282f252` (`SubagentLauncher::start_follow_up`) y `5b001af` (parámetro
`follow_up_of` de la herramienta `subagent`).

### Lo hecho

- `SubagentDescriptor.reference_task_ids: Vec<SessionId>`, persistido como JSON en la columna
  `subagents.reference_task_ids` (migración idempotente). Una fila anterior, o un valor ilegible,
  se lee como lista vacía.
- `SubagentLauncher::start_follow_up(parent, label, prompt, agent, follow_up_of, cancel)`: falla
  con `TaskNotFound` si la tarea no existe o es de otro padre, y con error de herramienta si sigue
  viva; antepone al prompt el resultado de la anterior (D4, acotado a `MAX_NOTICE_BYTES`) y guarda
  el enlace. El arranque común quedó en `arrancar`; `start` no cambia de comportamiento.
- La herramienta `subagent` acepta `follow_up_of` (entero ≥ 1); cualquier otro valor es un error de
  herramienta y no lanza nada.

### Cómo se comprobó

Cinco pruebas del lanzador (prompt con el resultado, `contextId` compartido, tarea viva rechazada,
tarea ajena o inexistente, tope del resultado), una sexta para el hijo sin resultado, tres de la
herramienta y dos de almacenamiento. A mano se quitó el tope del resultado y la comprobación de
estado terminal (cada prueba afectada falló) y se aflojó el mínimo de `follow_up_of` a 0.

### Lo que no consta o queda pendiente

- **Que el resultado vaya prefijado al prompt es decisión nuestra**: no consta qué debe hacer el
  servidor con las tareas referidas. No se ha comprobado cómo lo aprovecha un modelo real.
- **El enlace no se pinta en la interfaz**: el nodo del árbol de subagentes no muestra de quién es
  seguimiento un hijo.
- **Una sola referencia desde la herramienta**; el almacén admite varias.
- **Un resultado de más de 8 000 bytes entra recortado** (cabeza y cola); el seguimiento no ve el
  resto. No hay herramienta para leer artefactos enteros (Fase B de la autonomía).
- **`send_message` sigue sin restringirse** a responder preguntas (segunda viñeta de la Fase C).
- **El cable** (`referenceTaskIds` en JSON) queda para A4.
