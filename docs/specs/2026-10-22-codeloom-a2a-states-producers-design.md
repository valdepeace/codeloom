# Productores de los estados A2A que faltan (`submitted`, `rejected`, `auth-required`)

Estado: propuesta del 2026-10-22 (tarea A2 de `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`).
Relacionado: `2026-10-15-loom-agent-profile-a2a.md` (secciones «Fuente normativa de los
estados» y «Lectura de la especificación», de donde salen todas las citas),
`2026-10-21-codeloom-subagent-input-required-design.md`,
`2026-10-22-codeloom-a2a-states-ui.md` (plan que ya pinta los ocho estados).

## Intención

Premisa: el protocolo A2A, sin inventar. El rail y el showcase ya pintan los ocho
estados de tarea, pero el runtime solo produce cinco (`working`, `input-required`,
`completed`, `failed`, `canceled`). Se pretende que el runtime emita los tres que
faltan **solo donde la especificación los define y nuestro código tiene un hecho que
los respalde**, y rotular como decisión nuestra todo lo que la especificación no fija.

## Qué dice A2A (literal) y qué no

- `submitted`: «Indicates that a task has been successfully submitted and acknowledged.»
  **No consta** cuándo lo emite el agente ni que sea obligatorio.
- `rejected`: «Indicates that the agent has decided to not perform the task. This may be
  done during initial task creation or later once an agent has determined it can't or
  won't proceed. This is a terminal state.» Los errores de validación de entrada tienen
  otra vía: error `-32602 Invalid params` (§3.3.2), sin tarea.
- `auth-required`: el agente «MUST use a Task», «MUST transition the TaskState to
  `TASK_STATE_AUTH_REQUIRED`» y «MUST include a TaskStatus message explaining the
  required authorization»; entre los ejemplos, «An agent requiring human approval
  before a destructive action is taken». «Agents MUST NOT treat the
  `TASK_STATE_AUTH_REQUIRED` state transition, by itself, as authorization.» Es un
  estado «interrupted», no terminal.
- **No consta** una tabla de transiciones ni quién provoca cada una (salvo que
  `input-required` y `auth-required` los fija el agente).

## Objetivos

1. Un hijo recién creado y reconocido aparece como `submitted` y pasa a `working` al
   empezar a ejecutarse.
2. Un hijo que no se admite por falta de capacidad aparece como `rejected`, terminal.
3. Un hijo que espera la aprobación de un permiso aparece como `auth-required` y
   vuelve a `working` al resolverse.

## No objetivos

- Autenticación de transporte, tarjetas del agente o cualquier cosa remota (A6, A8).
- Que `auth-required` autorice nada por sí mismo: la decisión sigue en
  `PermissionService`.
- Persistir `auth-required`: es transitorio, como `input-required` (se deriva de
  eventos y no se guarda).
- Cambiar el esquema SQLite (la columna `status` es `TEXT NOT NULL`).

## Anti-objetivos

- Un `rejected` por validación de entrada: eso es un error del parámetro, no una
  tarea rechazada (la especificación lo trata con `Invalid params`).
- Parpadeos o trabajo extra en el camino del composer.
- Inventar semántica para `submitted` más allá de «recibida y reconocida».

## Decisiones de diseño (mías, marcadas para revisión)

- **`submitted`.** Se persiste al guardar el descriptor del hijo, que es el instante en
  que el lanzador ha reconocido la tarea (sesión creada, trabajo lanzado). Pasa a
  `Running` cuando arranca la ejecución del agente del hijo
  (`update_subagent_status` ya existe). Puede durar milisegundos: es honesto y no se
  retiene artificialmente. *Alternativa descartada:* emitirlo solo en memoria; se
  perdería al reabrir una sesión con hijos aún no arrancados (caso raro).
- **`rejected`.** Solo la **capacidad agotada** («maximum of 8 active subagents
  reached»): es un «can't proceed» del agente, que encaja con la definición. Se crea el
  descriptor del hijo con estado `Rejected` (para que la tarea exista y sea visible,
  como exige que `rejected` sea un estado de una `Task`) y se sigue devolviendo el
  error al padre para que el modelo lo vea. *Pregunta abierta para el usuario:* si se
  prefiere no crear sesión ni descriptor y dejar este caso solo como error.
- **`auth-required`.** Mientras `ChildResponder::decide` espera al servicio de
  permisos del padre, el hijo se marca `auth-required`; al resolver, vuelve a
  `working`. Dos eventos nuevos en el bus (petición y resolución), con una
  descripción corta de lo solicitado (herramienta y resumen), para cumplir «MUST
  include a TaskStatus message explaining the required authorization». La TUI lo
  guarda en memoria junto a `awaiting_input`.
- **Despertar del padre.** `auth-required` **no** despierta al padre: el diálogo de
  permiso ya se muestra en la propia interfaz del padre, que lo está esperando. Sin
  cambios en `wake_pending`.
- **Variantes nuevas** en `SubagentStatus`: `Submitted` y `Rejected`, con cadenas
  `"submitted"` y `"rejected"` en SQLite y proyección a `TaskState::Submitted` y
  `TaskState::Rejected`. `Rejected` es terminal (`is_terminal` ya lo contempla) y no
  despierta al padre (el modelo ya recibe el error).
- **Rail y showcase.** Sin trabajo de pintado nuevo: `AgentState` ya tiene los ocho. La
  columna «productor» del showcase pasa a decir «runtime» para los ocho.

## Qué se deja al implementador

Nombres de los eventos nuevos, forma exacta del mensaje de estado de
`auth-required` y dónde se hace el paso `Submitted → Running` dentro del cierre del
trabajo.

## Verificación

Pruebas puras (sin red) de: estado inicial `Submitted` y paso a `Running`; rechazo por
capacidad con descriptor `Rejected` y error devuelto; lectura de las cadenas nuevas en
SQLite; `decide` que marca y desmarca `auth-required` con su mensaje; que `rejected` y
`auth-required` no despiertan al padre; proyección a `TaskState`; el rail con cada uno.
Validación visual en terminal real: pendiente.

## Limitaciones previstas

- `submitted` puede ser imperceptible en la práctica.
- El mensaje de `auth-required` es un resumen de la petición, no el diálogo completo.
- La cancelación de un hijo en `auth-required` o `input-required` no está definida por
  A2A (no consta); hoy el usuario puede matarlo y se proyecta a `canceled`.
