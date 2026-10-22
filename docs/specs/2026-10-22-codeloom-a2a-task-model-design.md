# El estado de un subagente ES el estado de tarea de A2A

Estado: propuesta del 2026-10-22, por petición del usuario: «acoplarnos al estándar A2A
para tener menos fricciones al implementar o modificar algo; en la capa visual sí
podemos mostrar los modelos algo diferente». Plan:
`docs/plans/2026-10-22-codeloom-a2a-task-model.md`.
Sustituye el enfoque de «proyección en el borde» de `2026-10-21-codeloom-a2a-task-state.md`
y absorbe las tareas A1 (renombrado de `Interrupted`) y A2 (productores de estados) de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`. Los productores se justifican en
`2026-10-22-codeloom-a2a-states-producers-design.md` (citas literales del proto, que
siguen vigentes). Fuente normativa: `https://a2a-protocol.org/latest/definitions/`
(proto `lf.a2a.v1`) y las secciones citadas de la especificación.

## Intención

Hoy un subagente tiene un enum propio (`SubagentStatus`) y una función que lo proyecta a
`TaskState`. Cada diferencia entre ambos (`Killed` frente a `canceled`, `Unfinished`,
`input-required` como marca suelta de la interfaz) es una decisión nuestra que el
estándar no pidió y que habría que deshacer al abrir A2A al exterior. Se adopta el
estándar como modelo y se deja la proyección para donde sí corresponde: la presentación.

## Dos capas

- **Modelo (protocolo): A2A.** Estados de tarea, `TaskStatus`, ids, reglas de terminal,
  cancelar y reanudar. No se inventa nada; lo que A2A no fija se declara «no consta».
- **Presentación (nuestra, libre):** glifos, colores, etiquetas del rail, la línea de
  causa del despertar, el contador de ficheros. Puede mostrar el modelo de forma
  distinta (p. ej. agrupar `submitted` y `working` bajo el mismo glifo de actividad),
  pero **nunca** reescribe ni aproxima el estado del modelo.

### Regla de trazabilidad (obligatoria)

La presentación puede agrupar varios estados de A2A bajo un mismo aspecto, pero **todo
texto visible que los agrupe o los oculte nombra entre paréntesis los estados originales
de A2A**, con sus nombres exactos (`submitted`, `working`, `input-required`,
`auth-required`, `completed`, `failed`, `canceled`, `rejected`). Ejemplo: una cabecera
«en curso (submitted, working, input-required, auth-required)» o, cuando se conoce el
estado concreto, «en curso (working)». Debe poder reconstruirse siempre el estado A2A
exacto de una tarea a partir de lo que se ve; un glifo o color por sí solos no bastan
si dos o más estados lo comparten. Se verifica con pruebas: cada punto de agrupación
tiene un test que comprueba que el texto lista los estados originales.

## Objetivos

1. El estado persistido y en memoria de un subagente es un `TaskState` de A2A, sin enum
   paralelo ni proyección en el núcleo.
2. El estado lleva su `TaskStatus` completo: `state`, `message` y `timestamp`
   (proto: «A container for the status of a task»).
3. Las transiciones respetan A2A: una tarea terminal no se reinicia («Once a task reaches
   a terminal state ... it cannot restart»), y `input-required`/`auth-required` los fija
   el agente.
4. `input-required` y `auth-required` son estados reales de la tarea, no marcas de la
   interfaz, y el runtime produce los ocho.
5. Las sesiones y bases de datos existentes se siguen leyendo.

## No objetivos

- `Artifact`, `Message`/`Part` del protocolo, `contextId` y `reference_task_ids`: son las
  tareas A5 y A11. Hasta entonces `result` sigue siendo texto y el aviso al padre sigue
  siendo el que hay (es presentación del cliente hacia su propio modelo, no la entrega
  de la salida de una tarea entre agentes). **Se declara como divergencia temporal.**
- Transporte, tarjeta del agente, nombres JSON del cable (A4, A6, A7, A8).
- `JobStatus` de los trabajos de `bash` y similares: no son tareas A2A y quedan como están.

## Anti-objetivos

- Que la presentación vuelva a mover el modelo: un glifo o color nuevo no cambia un
  estado.
- Estados inventados: el enum es el de A2A sin añadidos ni supresiones (salvo
  `UNSPECIFIED`, que no es un estado real y se omite, como ya se hacía).

## Decisiones

### Respaldadas por el protocolo

| Decisión | Respaldo |
|---|---|
| El estado es exactamente `TaskState` (8 valores) | proto `TaskState` |
| `Killed` desaparece: cancelar un hijo lo lleva a `canceled` | «a task was canceled before completion» |
| `Unfinished` desaparece: un hijo en marcha al cerrar la aplicación pasa a `failed`, con un mensaje de estado que lo explica | `failed` = «finished with an error»; el mensaje cabe en `TaskStatus.message` |
| `TaskStatus { state, message, timestamp }` | proto `TaskStatus` |
| Terminal es inmutable | «it cannot restart» (Life of a Task) |
| `submitted` al reconocer, `working` al ejecutar; `rejected` como tarea rechazada; `auth-required` ante aprobación humana | spec de productores |
| Persistir el estado con los nombres del proto v1.0 (`TASK_STATE_WORKING`, ...) | `A2A-Version: 1.0`; evita traducir al leer |

### Lecturas o simplificaciones nuestras (rotuladas)

- **`TaskStatus.message` como texto.** En el proto es un `Message` con `Part`s; se guarda
  como texto hasta A5 (que introduce `Message`/`Part`).
- **Lo no terminado al reabrir pasa a `failed`.** A2A no dice qué estado corresponde a
  una tarea cuyo proceso desapareció; `failed` es la lectura más cercana a su
  definición.
- **Capacidad agotada = `rejected`**, como se razonó en la spec de productores.
- **Cadenas antiguas** al leer: `running` → `working`, `completed` → `completed`,
  `failed` → `failed`, `killed` → `canceled`, `interrupted` y `unfinished` → `failed`.
- **Nuevas columnas** `status_message` y `status_timestamp_ms` en `subagents`, añadidas
  de forma idempotente (se comprueba `PRAGMA table_info`).

### Solo de presentación (libres)

Glifos y colores (`AgentState`), etiquetas, agrupar estados bajo un mismo aspecto,
`wake_causes`, `Nf`. `AgentState` se deriva de `TaskState` en la capa de la interfaz.

## Qué se deja al implementador

Cómo y dónde se aplican y publican las transiciones en el lanzador (siempre vía un único
punto que valide A2A y emita `SubagentChanged`), y cómo obtiene `ChildResponder` acceso
para fijar `auth-required`.

## Verificación

Pruebas puras (sin red) de: ida y vuelta de cada estado por SQLite con sus nombres;
lectura de las cadenas antiguas; migración idempotente de columnas sobre una base
anterior; rechazo de transiciones desde un estado terminal; reapertura que marca lo no
terminado como `failed` con mensaje; productores (`submitted`→`working`, `rejected` por
capacidad sin que la validación cree tarea, `input-required`, `auth-required`,
`canceled` al matar); la interfaz derivando el aspecto de `TaskState`. Validación visual
en terminal real: pendiente.

## Limitaciones previstas

- Divergencias temporales declaradas arriba (`result` como texto, mensaje de estado como
  texto, ids propios sin `contextId`) hasta A5 y A11.
- La spec no define la cancelación en `input-required` ni `auth-required`; hoy el
  usuario puede matar al hijo y la tarea pasa a `canceled`.
- `submitted` puede durar milisegundos.

## Estado (2026-10-22)

Cabe señalar que la tarea 1 (modelo) y la tarea 2 (productores) del plan
`docs/plans/2026-10-22-codeloom-a2a-task-model.md` están implementadas, con los commits
`45807b2` y `6365c58`. La tarea 3 (presentación y documentación) cierra el plan.

### Lo implementado

- El estado de un subagente es `TaskState` con su `TaskStatus` (`state`, `message`,
  `timestamp_ms`). `SubagentStatus`, `Killed` y `Unfinished` han desaparecido.
- SQLite guarda los nombres del proto v1.0 y lee las cadenas antiguas. Las columnas
  `status_message` y `status_timestamp_ms` se añaden de forma idempotente.
- Al reabrir la aplicación, todo estado no terminal pasa a `failed` con el mensaje
  `interrupted by application shutdown`.
- Un único punto de transición valida las reglas de A2A, persiste y publica
  `SubagentChanged`. Una tarea terminal no admite transición posterior.
- El runtime produce los ocho estados. La interfaz deriva su aspecto de `TaskState` con
  `match` exhaustivo, sin comodín.

### Decisiones tomadas que A2A no fija

- Texto de `input-required`: las preguntas pendientes se unen con salto de línea.
- `auth-required`: el texto es «herramienta acción: detalle», con 160 caracteres como
  máximo. Solo se produce cuando el padre va a preguntar de verdad a una persona, es
  decir, cuando `PermissionService::would_prompt` lo confirma. `--yolo` o un permiso ya
  concedido no lo activan.
- Esc del padre sobre el hijo lleva a este a `canceled`.
- Capacidad agotada: `rejected` con el mensaje `maximum of 8 active subagents reached`.
  Los errores de validación de entrada no crean tarea.
- Lo no terminado al reabrir pasa a `failed` con el mensaje indicado arriba.

### Limitaciones reales

- La cabecera plana puede recortar el resumen en terminales estrechas. Ahí se incumple
  la regla de trazabilidad, porque el texto con los estados originales de A2A puede
  quedar fuera de la vista.
- La cabecera del workspace pasa de 2 a 3 líneas cuando hay subagentes.
- La validación visual en una terminal real está pendiente. Las pruebas son puras y no
  sustituyen una comprobación en pantalla.
