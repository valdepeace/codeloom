# A8c: streaming SSE: plan de implementación

> **Para quien ejecute:** una tarea cada vez; tras cada una, `cargo test -p codeloom-a2a -p codeloom-core`,
> `cargo clippy -p codeloom-a2a -p codeloom-core -p codeloom-cli -- -D warnings`,
> `rustfmt --edition 2021 --check` de los ficheros tocados, y un commit. Nada de `cargo fmt --all`.

**Objetivo:** servir y consumir el flujo de tareas por SSE, sin canal paralelo y sin perder nunca el
seguimiento.

**Spec:** `docs/specs/2026-10-24-codeloom-a2a-streaming-design.md`.

## Restricciones globales

- `codeloom-core` sin `axum`/`hyper`/`tower`/`ratatui`. Sin red real en pruebas (bucle local y `wiremock`).
- Nada sin cota (decálogo): flujos abiertos, tamaño de línea y de evento, duración.
- La tarjeta solo anuncia `streaming: true` cuando el servidor lo soporta de verdad.

## Revisión: lo que la spec implica y ninguna tarea cubre sola

- `SubscribeToTask` de una tarea terminal → −32004; de una ajena o inexistente → −32001.
- El flujo empieza por el `Task` y se cierra al terminal, aunque la tarea ya fuera terminal al enviar.
- El flujo número 17 no se abre; liberar uno (soltar la conexión) devuelve el hueco.
- Un evento SSE enorme, sin salto de línea o mal formado no cuelga ni agota memoria del cliente.
- Un remoto que corta el flujo a medias deja la tarea siguiéndose por sondeo hasta el terminal.
- Sin `Authorization`, los flujos se rechazan igual que `SendMessage`.

### Tarea 1: el anfitrión suscribe (servidor, sin HTTP)

**Ficheros:** `crates/codeloom-a2a/src/serve/rpc.rs` (trait), `serve/host.rs`.
**Produce:** `TaskHost::subscribe(&self, id) -> Result<BoxStream<'static, WireStreamResponse>, HostError>`
y `TaskHost::start_streaming(prompt)` o equivalente que devuelva el flujo desde el `Task` inicial;
límite de 16 flujos con permiso ligado al flujo; texto acotado.
- [ ] Pruebas: `a_stream_starts_with_the_task_and_ends_at_the_terminal_state`,
  `subscribing_to_a_terminal_task_is_unsupported`, `another_sessions_task_cannot_be_subscribed`,
  `the_seventeenth_stream_is_refused_and_a_dropped_one_frees_its_slot`, `stream_text_is_bounded`.
- [ ] Implementar; commitear.

### Tarea 2: SSE en el enrutador y tarjeta con `streaming: true`

**Ficheros:** `serve/rpc.rs`, `serve/card.rs`.
- [ ] Pruebas con `tower::oneshot`: cabecera `text/event-stream`, cada evento `data: {"jsonrpc","id","result"}`,
  primer evento `task`, errores previos al flujo como JSON-RPC normal, sin credencial → 401,
  la tarjeta declara `streaming: true`.
- [ ] Implementar con `axum::response::sse` y latido de 15 s; commitear.

### Tarea 3: ida y vuelta del servidor con el SDK oficial

**Ficheros:** `crates/codeloom-a2a/tests/serve_round_trip.rs`, scratchpad del cliente del SDK.
- [ ] Prueba por bucle local que lee el SSE crudo; prueba viva a mano con el cliente `a2a-sdk` en modo
  `streaming=True`. Docs de A8b actualizados (ya no es −32004). Commitear.

### Tarea 4: lector SSE del cliente

**Ficheros:** `crates/codeloom-a2a/src/sse.rs` (nuevo), `Cargo.toml` (reqwest `stream`).
**Produce:** un decodificador de líneas SSE acotado (`LineaMax`, `EventoMax`) que da `serde_json::Value`
por evento `data:`.
- [ ] Pruebas: eventos partidos en varios trozos, CRLF, comentarios de latido ignorados, línea > 64 KiB
  → error, evento > 256 KiB → error, `data:` multilínea unido, cierre limpio.
- [ ] Implementar; commitear.

### Tarea 5: `SubscribeToTask` en el cliente y seguimiento con vuelta al sondeo

**Ficheros:** `codeloom-core/src/remote_task.rs` (método `subscribe` con valor por defecto «no soportado»),
`codeloom-a2a/src/client.rs`, `codeloom-core/src/subagent.rs` (bucle de seguimiento remoto), tarjeta
remota (guardar `streaming`).
- [ ] Pruebas con `wiremock`/servidor de bucle local: usa el flujo si la tarjeta declara streaming y no
  hace `GetTask`; sin streaming sondea como antes; un corte a mitad pasa al sondeo y acaba en el estado
  terminal; un evento de otra tarea o mal formado corta el flujo y sondea.
- [ ] Implementar; commitear.

### Tarea 6: verificación y docs

- [ ] Round-trip cliente propio ↔ servidor propio por flujo; «Estado» de la spec, roadmap (A8c [x]),
  README, y la sección de límites de A8b. Honestidad sobre lo no visto.
