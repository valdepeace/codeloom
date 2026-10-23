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

- **Literal:** `StreamResponse` lleva «exactly one of» `task`, `message`, `statusUpdate`,
  `artifactUpdate`.
- **Resumen** (spec leída en la tarea A4): el orden de los eventos está garantizado y rige para
  cualquier binding; puede haber pérdida al reconectar; se difunde a varios flujos.

## Lo que NO consta (y por tanto decidimos)

- **Los campos exactos** de `TaskStatusUpdateEvent` y `TaskArtifactUpdateEvent` en la v1.0 (no se
  leyó `a2a.proto`). De la v0.3 se recuerda un indicador `final` y, en el de artefactos, `append` y
  `lastChunk`; **no se sabe si la v1.0 los conserva**. Se modelan solo `task_id`, `context_id` y la
  carga (estado o artefacto); lo demás se decide al contrastar el proto (tarea 0 del plan).
- **Qué emite primero un flujo recién abierto.** Decisión nuestra: una instantánea `task` con el
  estado actual, para que el suscriptor no se pierda lo ya ocurrido.
- **Cuándo se cierra el flujo.** Duda abierta ya registrada (§3.1.2 frente a §11.7). Decisión
  nuestra: se cierra tras el estado terminal; en `input-required` y `auth-required` sigue abierto.
- **Cómo se reanuda tras una pérdida.** No consta. Decisión nuestra: si el suscriptor se atrasa
  (`Lagged` del bus), se le envía una instantánea nueva en lugar de un hueco silencioso.

## Decisiones (nuestras salvo indicación)

- **D1. Tipos en el núcleo** (`a2a.rs`): `TaskStatusUpdateEvent`, `TaskArtifactUpdateEvent` y
  `StreamResponse { Task, Message, StatusUpdate, ArtifactUpdate }`. Sin nombres del cable: los
  pone el adaptador (A4).
- **D2. La fuente es el bus.** Una función pura `eventos_de_cambio(anterior, nuevo)` convierte un
  par de descriptores en cero o más eventos: uno de estado si cambió `status.state`, uno de
  artefacto por cada artefacto nuevo. No se inventan transiciones.
- **D3. Un flujo por tarea** (`subscribe_task(parent, task) -> BoxStream<Result<StreamResponse>>`),
  filtrando la suscripción al bus por `(parent, child)`. Varios suscriptores a la vez son
  independientes.
- **D4. Orden:** el de publicación en el bus, que es por emisor. Se prueba que dos cambios
  sucesivos llegan en orden.
- **D5. Una tarea ya terminal** produce la instantánea y el cierre inmediato, sin esperar.
- **D6. Cota:** el búfer del bus ya está acotado (`EventBus::new(n)`); un suscriptor lento pierde
  eventos y recibe una instantánea, nunca se acumulan sin límite.

## Fuera de alcance, declarado

- SSE, JSON-RPC y los nombres `statusUpdate` / `artifactUpdate` del cable: A8 y A4 (los tipos del
  cable se añaden en la tarea 4).
- Notificaciones push.
- Reproducción histórica de eventos pasados: el bus no guarda historial.
- Que la interfaz consuma estos eventos: ya vive de `SubagentChanged`; no se toca.

## Preguntas abiertas

- Los campos reales del proto (tarea 0).
- Si `ChildEvent` (texto y herramientas del hijo) debe reflejarse como `message` del flujo; hoy
  no se hace.
