# Tareas enlazadas: un seguimiento es una tarea nueva (A11)

Estado: propuesta del 2026-10-23 (tarea A11 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`,
Fase C de `2026-10-21-codeloom-agent-autonomy-design.md`).

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

## Estado actual

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
