# Flujo de eventos de tarea de A2A sobre el bus existente (A9)

Estado: propuesta del 2026-10-23, pendiente de aprobación (tarea A9 de
`docs/plans/2026-10-22-codeloom-a2a-roadmap.md`). No implementar todavía. Depende de A7.

## Propósito

Quien sigue una tarea (la interfaz, un cliente remoto en A8) necesita enterarse de los cambios de
estado y de la aparición de artefactos sin sondear. A2A lo modela con `SubscribeToTask` y con
eventos de estado y de artefacto dentro de un `StreamResponse`. El núcleo ya publica en su bus
`SubagentChanged`, `SubagentSettled` y `ChildEvent`. Esta tarea deriva de ellos los eventos de A2A,
**sin abrir un canal paralelo**, y añade `subscribe_task` al contrato de A7.

## Lo que consta en A2A v1.0

**Literal del proto** (`package lf.a2a.v1`, servido en `a2a-protocol.org/latest/spec/a2a.proto`,
copiado como fixture en `crates/codeloom-a2a/tests/fixtures/a2a.proto`):

- `TaskStatusUpdateEvent { task_id = 1, context_id = 2, status = 3, metadata = 4 }`. **No tiene
  indicador `final`** (la v0.3 sí lo tenía).
- `TaskArtifactUpdateEvent { task_id = 1, context_id = 2, artifact = 3, append = 4, last_chunk = 5,
  metadata = 6 }`; `append`: «the content of this artifact should be appended to a previously sent
  artifact with the same ID»; `last_chunk`: «this is the final chunk of the artifact».
- `StreamResponse` con `oneof payload { task = 1; message = 2; status_update = 3;
  artifact_update = 4 }`.
- `SubscribeToTask`: «Subscribes to task updates for tasks not in a terminal state. Returns
  `UnsupportedOperationError` if the task is already in a terminal state (completed, failed,
  canceled, rejected).»

- **Literal:** `StreamResponse` lleva «exactly one of» `task`, `message`, `statusUpdate`,
  `artifactUpdate`.
- **Resumen** (spec leída en la tarea A4): el orden de los eventos está garantizado y rige para
  cualquier binding; puede haber pérdida al reconectar; se difunde a varios flujos.

## Lo que NO consta (y por tanto decidimos)

- **Qué emite primero un flujo recién abierto**: el proto no lo dice (véase abajo).
- **Qué emite primero un flujo recién abierto.** Decisión nuestra: una instantánea `task` con el
  estado actual, para que el suscriptor no se pierda lo ya ocurrido (`StreamResponse` admite `task`
  como primer elemento).
- **Cuándo se cierra el flujo.** Duda abierta ya registrada (§3.1.2 frente a §11.7). Decisión
  nuestra: se cierra tras el estado terminal; en `input-required` y `auth-required` sigue abierto.
- **Cómo se reanuda tras una pérdida.** No consta. Decisión nuestra: si el suscriptor se atrasa
  (`Lagged` del bus), se le envía una instantánea nueva en lugar de un hueco silencioso.

## Decisiones (nuestras salvo indicación)

- **D1. Tipos en el núcleo** (`a2a.rs`), con los campos del proto: `TaskStatusUpdateEvent
  { task_id, context_id, status, metadata }`, `TaskArtifactUpdateEvent { task_id, context_id,
  artifact, append, last_chunk, metadata }` y `StreamResponse { Task, Message, StatusUpdate,
  ArtifactUpdate }`. Sin nombres del cable: los pone el adaptador (A4). Un artefacto nuestro llega
  entero: `append = false`, `last_chunk = true`.
- **D2. La fuente es el bus.** Una función pura `eventos_de_cambio(anterior, nuevo)` convierte un
  par de descriptores en cero o más eventos: uno de estado si cambió `status.state`, uno de
  artefacto por cada artefacto nuevo. No se inventan transiciones.
- **D3. Un flujo por tarea** (`subscribe_task(parent, task) -> BoxStream<Result<StreamResponse>>`),
  filtrando la suscripción al bus por `(parent, child)`. Varios suscriptores a la vez son
  independientes.
- **D4. Orden:** el de publicación en el bus, que es por emisor. Se prueba que dos cambios
  sucesivos llegan en orden.
- **D5. Suscribirse a una tarea ya terminal es un error `UnsupportedOperation`**, como exige el
  proto (corrige el primer borrador, que proponía instantánea y cierre). Se añade
  `CoreError::UnsupportedOperation(String)`, cuyo código de cable (−32004) lo asigna A8. Una tarea
  inexistente o ajena sigue siendo `TaskNotFound`.
- **D6. Cota:** el búfer del bus ya está acotado (`EventBus::new(n)`); un suscriptor lento pierde
  eventos y recibe una instantánea, nunca se acumulan sin límite.

## Fuera de alcance, declarado

- SSE, JSON-RPC y los nombres `statusUpdate` / `artifactUpdate` del cable: A8 y A4 (los tipos del
  cable se añaden en la tarea 4).
- Notificaciones push.
- Reproducción histórica de eventos pasados: el bus no guarda historial.
- Que la interfaz consuma estos eventos: ya vive de `SubagentChanged`; no se toca.

## Preguntas abiertas

- Si `ChildEvent` (texto y herramientas del hijo) debe reflejarse como `message` del flujo; hoy
  no se hace.
