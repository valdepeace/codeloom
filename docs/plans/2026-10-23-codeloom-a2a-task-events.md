# Flujo de eventos de tarea (A9): plan de implementación

> **Para quien ejecute:** una tarea cada vez; tras cada una, `cargo test -p codeloom-core -p
> codeloom-a2a`, `cargo clippy -p codeloom-core -p codeloom-a2a -p codeloom-tui -p codeloom-cli --
> -D warnings`, `rustfmt --edition 2021 --check` y un commit. Nada de `cargo fmt --all`.

**Objetivo:** que `AgentTransport` ofrezca `subscribe_task`, con eventos derivados del bus.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-task-events-design.md`. **Requiere A7 hecha.**

## Restricciones globales

- `codeloom-core` sin tipos del cable ni red. Sin canal de interfaz paralelo: todo sale del bus.
- Nada sin cota: el búfer del bus manda; el atraso se resuelve con una instantánea.

## Revisión: lo que la spec implica y ninguna tarea cubre sola

- Suscribirse a una tarea ya terminal da `UnsupportedOperation` (proto), no se cuelga.
- Un suscriptor lento no hace crecer memoria y recibe una instantánea nueva.
- Dos suscriptores a la misma tarea ven lo mismo, y uno que se va no afecta al otro.
- Una tarea ajena o inexistente da `TaskNotFound` al suscribirse.
- Un cambio que no altera el estado (mismo `state`) no emite evento de estado.

### Tarea 0: contrastar el proto (hecha el 2026-10-23)

- [x] Proto leído y guardado como fixture; spec corregida: campos reales de los eventos, sin `final`,
  y suscribirse a una tarea terminal es `UnsupportedOperationError`. La regla de cierre en estados
  interrumpidos sigue siendo decisión nuestra (el proto no la fija).

### Tarea 1: tipos y derivación pura

**Ficheros:** `crates/codeloom-core/src/a2a.rs`, `crates/codeloom-core/src/transport.rs`.

**Interfaces:** produce `TaskStatusUpdateEvent { task_id, context_id, status, metadata }`,
`TaskArtifactUpdateEvent { task_id, context_id, artifact, append, last_chunk, metadata }`,
`StreamResponse` y
`fn eventos_de_cambio(anterior: &SubagentDescriptor, nuevo: &SubagentDescriptor) -> Vec<StreamResponse>`.

- [x] Pruebas: `a_state_change_emits_a_status_update`, `an_unchanged_state_emits_nothing`,
  `a_new_artifact_emits_an_artifact_update`, `events_keep_the_order_state_then_artifacts`.
- [x] Implementar; commitear.

### Tarea 2: el flujo sobre el bus

**Ficheros:** `crates/codeloom-core/src/transport.rs`.

**Interfaces:** produce `fn flujo_de_tarea(bus: &EventBus, actual: SubagentDescriptor) ->
BoxStream<'static, StreamResponse>`: instantánea, luego eventos, cierre tras el estado terminal.

- [x] Pruebas: `the_stream_starts_with_a_snapshot`, `the_stream_closes_after_the_terminal_state`,
  `input_required_keeps_the_stream_open`, `a_lagging_subscriber_gets_a_fresh_snapshot`,
  `two_subscribers_are_independent`.
- [x] Implementar con la suscripción al bus filtrada por `(parent, child)`; commitear.

### Tarea 3: `subscribe_task` en el contrato

**Ficheros:** `crates/codeloom-core/src/transport.rs`.

- [x] Añadir `subscribe_task(parent, SessionId) -> Result<BoxStream<...>>` al trait (con
  implementación por defecto que falla) y a `InProcessTransport`; ampliar `transport_contract`.
- [x] Pruebas: `subscribe_to_an_unknown_task_is_not_found`,
  `subscribe_to_a_terminal_task_is_unsupported`, `a_full_run_streams_working_then_completed`
  con un hijo real del lanzador de prueba; commitear.

### Tarea 4: tipos del cable y documentos

**Ficheros:** `crates/codeloom-a2a/src/wire.rs`, las specs.

- [x] `WireStreamResponse` con exactamente una de `task`, `message`, `statusUpdate`,
  `artifactUpdate` (el `oneof payload` del proto); pruebas de serialización y de rechazo de cero o dos claves.
- [x] «Estado» de la spec y roadmap (A9 [x]); commitear.
