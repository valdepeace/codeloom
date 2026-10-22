# Productores de los estados A2A que faltan (`submitted`, `rejected`, `auth-required`)

Estado: propuesta del 2026-10-22, revisada el mismo día bajo la regla «no inventar nada
que no esté contemplado en A2A» (tarea A2 de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`). **Se implementa sobre el modelo de
`2026-10-22-codeloom-a2a-task-model-design.md`** (el estado del subagente ES el
`TaskState` de A2A), cuyo plan es `docs/plans/2026-10-22-codeloom-a2a-task-model.md`
(tarea 2); aquí se conservan las citas y la justificación de cada productor. Donde este
documento habla de «variantes nuevas de `SubagentStatus`», léase «estados de
`TaskState`»: esa enumeración desaparece.
Relacionado: `2026-10-15-loom-agent-profile-a2a.md` (secciones «Fuente normativa de los
estados» y «Lectura de la especificación», de donde salen las citas),
`2026-10-21-codeloom-subagent-input-required-design.md`.

## Intención

El rail y el showcase ya pintan los ocho estados de tarea, pero el runtime solo produce
cinco (`working`, `input-required`, `completed`, `failed`, `canceled`). Se pretende que
emita `submitted`, `rejected` y `auth-required` **únicamente donde el protocolo los
define o los ejemplifica y nuestro código tiene un hecho que los respalde**. Todo lo
que A2A no fija se rotula como nuestro y se mantiene al mínimo.

## Qué contempla A2A (literal, fuente de cada punto entre paréntesis)

- `submitted`: «Indicates that a task has been successfully submitted and acknowledged.»
  (proto). **Único uso en la especificación:** la respuesta a `SendMessage` de una
  tarea larga, con `"state": "TASK_STATE_SUBMITTED"` y el trabajo continuando después
  (§6.6). Los ejemplos de streaming, en cambio, abren con `TASK_STATE_WORKING` (§6.2):
  `submitted` no es obligatorio.
- `working`: «actively being processed by the agent.» (proto).
- `rejected`: «Indicates that the agent has decided to not perform the task. This may be
  done during initial task creation or later once an agent has determined it can't or
  won't proceed. This is a terminal state.» (proto). **La especificación no incluye
  ningún ejemplo** de emisión de `rejected`.
- Errores de validación de entrada: categoría aparte, «Servers MUST validate all input
  parameters before processing», con el código `-32602 Invalid params` (§3.3.2). No
  producen tarea rechazada.
- `auth-required` (§7.6): el agente «MUST use a Task», «MUST transition the TaskState to
  `TASK_STATE_AUTH_REQUIRED`», «MUST include a TaskStatus message explaining the required
  authorization»; entre los ejemplos, «An agent requiring human approval before a
  destructive action is taken». El cliente puede «send a response message to the Task to
  negotiate, correct, or reject the authorization request» o cumplirla «out-of-band».
  Tras recibir la credencial, «The agent MAY immediately continue Task processing».
  «Agents MUST NOT treat the `TASK_STATE_AUTH_REQUIRED` state transition, by itself, as
  authorization for any particular operation.»
- Un `SendMessage` devuelve una `Task` o un `Message`; si la tarea existe, su estado es
  uno de los de la enumeración. Por tanto, **un rechazo es una `Task` en
  `TASK_STATE_REJECTED`**, no la ausencia de tarea.

## Qué NO contempla A2A

- Cuándo exactamente emite el agente `submitted`; una tabla de transiciones; quién
  provoca cada una salvo las que fijan `input-required` y `auth-required` (el agente).
- Qué motivos concretos justifican un `rejected` (nada de capacidad ni de límites).
- Qué ocurre si el cliente rechaza una petición de `auth-required` (a qué estado pasa
  la tarea).

## Objetivos

1. Un hijo reconocido por el lanzador aparece como `submitted` y pasa a `working` cuando
   su agente empieza a ejecutarse.
2. Un hijo que el lanzador no admite por agotarse la capacidad aparece como una tarea
   `rejected`.
3. Un hijo que espera la aprobación de un permiso aparece como `auth-required`, con un
   mensaje que lo explica, y vuelve a `working` al resolverse.

## No objetivos

- Autenticación de transporte, tarjetas del agente o nada remoto (A6, A8).
- Que `auth-required` autorice: la decisión sigue en `PermissionService`.
- Persistir `auth-required` (es transitorio, como `input-required`).
- Cambiar el esquema SQLite (la columna `status` es `TEXT NOT NULL`).

## Anti-objetivos

- Convertir en `rejected` los errores de validación (prompt vacío o demasiado largo,
  etiqueta demasiado larga): son errores de parámetros, y siguen siéndolo.
- Retener artificialmente `submitted` para que se vea.

## Decisiones, clasificadas

### Respaldadas por el protocolo

| Decisión | Respaldo |
|---|---|
| `submitted` es el estado de la tarea cuando el lanzador la ha reconocido y el trabajo sigue en segundo plano; pasa después a `working` | definición de `submitted` y ejemplo §6.6; `working` en §6.2 |
| `auth-required` se produce cuando el hijo espera la aprobación humana de una acción | ejemplo de §7.6 |
| El hijo usa su `Task` (su sesión) y su cambio de estado incluye un mensaje explicativo | §7.6.1 |
| Al resolverse la aprobación, el hijo vuelve a `working` | «The agent MAY immediately continue Task processing» |
| Un rechazo es una `Task` en `rejected` | `SendMessage` devuelve `Task` o `Message`; `rejected` es un estado de `Task` |
| Los errores de validación no producen `rejected` | §3.3.2 |
| `rejected` y `auth-required` no los inventa el cliente: los fija el agente | `TaskStatusUpdateEvent` «sent by the agent» |

### Lectura nuestra de la definición (no explícita en A2A)

- **Capacidad agotada = `rejected`.** La definición dice «can't or won't proceed»; que
  el límite de ocho hijos activos sea un «can't» es una lectura nuestra, la única razón
  de rechazo que hoy tiene el lanzador además de la validación. Si no se acepta la
  lectura, `rejected` queda sin productor y no se inventa otro.
- **El mensaje de estado de `auth-required`** es un resumen de la petición (herramienta
  y argumentos resumidos), no el diálogo completo.
- **Denegar el permiso** hace volver al hijo a `working` con la herramienta fallida;
  A2A no dice a qué estado pasa la tarea tras un rechazo del cliente.

### Nuestras, no del protocolo (interfaz y plomería)

- Que `rejected` conserve el error que ya recibe el modelo del padre (`is_error`).
- Que `rejected` y `auth-required` **no despierten** al padre (el padre ya ve el error y
  el diálogo de permiso respectivamente).
- Los eventos internos del bus que comunican el cambio, y su forma.
- Los glifos y colores (ya implementados en `AgentState`).

## Qué se deja al implementador

Nombres de los eventos nuevos, forma exacta del mensaje de `auth-required` y dónde se
hace el paso `Submitted → Running` dentro del cierre del trabajo.

## Verificación

Pruebas puras (sin red) de: estado inicial `Submitted` y paso a `Running`; rechazo por
capacidad con tarea `Rejected` visible y error devuelto; que los errores de validación
**no** crean tarea; lectura de las cadenas nuevas en SQLite; `auth-required` marcado y
desmarcado con su mensaje; que no despiertan al padre; proyección a `TaskState`; el rail
con cada uno. Validación visual en terminal real: pendiente.

## Limitaciones previstas

- `submitted` puede durar milisegundos y ser imperceptible.
- A2A no define la cancelación en `auth-required` o `input-required`; hoy el usuario
  puede matar al hijo y se proyecta a `canceled`.
