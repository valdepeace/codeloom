# Cancelar un subagente según A2A (A10): plan de implementación

> **Para quien ejecute:** una tarea cada vez; tras cada una, `cargo test -p codeloom-core`,
> `cargo clippy -p codeloom-core -- -D warnings` y `rustfmt --edition 2021 --check` sobre los
> ficheros tocados, y un commit. Nada de `cargo fmt --all`.

**Objetivo:** que cancelar a un hijo devuelva la tarea actualizada y falle con errores tipados.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-cancel-design.md`.

## Restricciones globales

- `codeloom-core` no depende de `ratatui` ni de `crossterm`. Errores de librería con `thiserror`.
- Sin red real en tests: proveedores simulados.
- Los textos de error que llegan al modelo van en inglés; los comentarios, en español.

## Revisión: lo que la especificación implica y ninguna tarea cubre sola

- Cancelar dos veces seguidas mientras el hijo aún se detiene no mata dos veces ni falla (D4).
- Cancelar a un hijo ya cancelado da `TaskNotCancelable` con su estado (D4).
- Cancelar un hijo de otro padre da `TaskNotFound`, no revela que existe (D5).
- Un hijo que no se asienta en `CANCEL_WAIT` no se declara cancelado (D2).
- Un hijo en `input-required` se cancela y su pregunta desaparece (D3, ya probado).

### Tarea 1: errores tipados

**Ficheros:** `crates/codeloom-core/src/error.rs`.

**Interfaces:** produce `CoreError::TaskNotFound(i64)` («task not found: {0}») y
`CoreError::TaskNotCancelable { child: i64, state: TaskState }` («task {child} is not
cancelable: it is {state:?}»), con `TaskState` de `crate::task`.

- [ ] Añadir las dos variantes y un test que compruebe sus textos.
- [ ] Verificar y commitear.

### Tarea 2: `SubagentLauncher::cancel`

**Ficheros:** `crates/codeloom-core/src/subagent.rs`.

**Interfaces:** consume los errores de la tarea 1. Produce en el trait
`async fn cancel(&self, parent: SessionId, child: SessionId) -> Result<SubagentDescriptor>`
(por defecto, error de herramienta «subagent cancel unavailable»), la constante
`CANCEL_WAIT: Duration = 2 s` y la implementación en `InProcessSubagentLauncher`.

- [ ] Tests (nombre y aserciones):
  - `cancel_returns_the_task_canceled`: hijo que pregunta (`ask_user`), `cancel` devuelve un
    descriptor con `status.state == Canceled`.
  - `cancel_of_a_finished_child_is_not_cancelable`: hijo completado → `TaskNotCancelable` con
    `state: Completed`; un segundo `cancel` de un hijo ya cancelado → `TaskNotCancelable` con
    `state: Canceled`.
  - `cancel_of_a_child_of_another_parent_is_not_found`: `TaskNotFound`.
  - `a_repeated_cancel_while_stopping_is_idempotent`: dos `cancel` seguidos sobre el mismo hijo
    vivo no fallan y ambos devuelven un descriptor.
  - `cancel_does_not_claim_success_if_the_child_does_not_settle`: con un hijo que ignora la
    cancelación (herramienta que no termina), `cancel` devuelve su estado actual, no `Canceled`.
    Para no esperar 2 s reales se usa `tokio::time::pause`.
- [ ] Implementar `cancel`: localizar el descriptor del padre (`TaskNotFound` si no está); si el
  estado es terminal, `TaskNotCancelable`; si el trabajo está `Running`, `kill`; en ambos casos
  esperar a que el descriptor sea terminal hasta `CANCEL_WAIT` y devolver el último leído.
- [ ] `control(.., "interrupt", ..)` delega en `cancel`.
- [ ] Verificar (también que los tests antiguos de `control` siguen en verde) y commitear.

### Tarea 3: la herramienta informa del estado

**Ficheros:** `crates/codeloom-core/src/tools/subagent_control.rs`.

- [ ] Test `interrupt_reports_the_state_it_ended_in` con el lanzador de prueba de ese fichero:
  respuesta `Subagent N: canceled.` si el descriptor devuelto es `Canceled`, y
  `Subagent N: cancel requested; still stopping (state: working).` si no.
- [ ] `run` llama a `cancel` para `interrupt` y formatea según D7; `send_message` no cambia.
- [ ] Verificar y commitear.

### Tarea 4: documentos

**Ficheros:** esta spec (sección «Estado»), `docs/specs/2026-10-15-loom-agent-profile-a2a.md`
(dudas 2, 3 y 4 → «resueltas como decisión nuestra, véase la spec de A10») y el roadmap (A10).

- [ ] Escribir el estado real, con commits, lo que no se ha visto en terminal real y lo que queda.
- [ ] Commitear.
