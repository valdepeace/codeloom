# Transporte `InProcess` (A7): plan de implementación

> **Para quien ejecute:** una tarea cada vez; tras cada una, `cargo test -p codeloom-core`,
> `cargo clippy -p codeloom-core -p codeloom-tui -p codeloom-cli -- -D warnings`,
> `rustfmt --edition 2021 --check` sobre los ficheros tocados y un commit. Nada de `cargo fmt --all`.

**Objetivo:** un contrato `AgentTransport` y su implementación en memoria sobre el lanzador.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-inprocess-transport-design.md`.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`, `crossterm` ni de tipos del cable. Errores con
  `thiserror`. Sin red en las pruebas.
- Nada sin cota llega al modelo: se reutilizan los topes del lanzador (A11 acota el resultado).
- Depende de A5, A10 y A11 (hechas).

## Revisión: lo que la spec implica y ninguna tarea cubre sola

- Reanudar una tarea que no está en `input-required` no entrega nada.
- Un `task_id` de otro padre no revela que existe (`TaskNotFound`).
- Más `Part`s de texto que preguntas pendientes, o menos: error y nada se entrega.
- Una `Part` que no sea de texto en un mensaje de reanudación se rechaza.
- `agent_card` de un agente inexistente es error.

### Tarea 1: el contrato

**Ficheros:** `crates/codeloom-core/src/transport.rs`, `lib.rs`.

**Interfaces:** produce `SendRequest { parent, agent, label, message, task_id, reference_task_ids }`
y el trait `AgentTransport` con `send_message(&self, SendRequest, CancellationToken) ->
Result<SubagentDescriptor>`, `get_task(parent, SessionId)`, `list_tasks(parent)`,
`cancel_task(parent, SessionId)` y `agent_card(Option<&str>) -> Result<AgentCard>`.

- [ ] Prueba de compilación con un transporte de juguete que implemente el trait.
- [ ] Escribir el trait con la documentación de cada operación; commitear.

### Tarea 2: enviar un mensaje nuevo, consultar y listar

**Ficheros:** `crates/codeloom-core/src/transport.rs`.

**Interfaces:** consume la tarea 1. Produce `InProcessTransport::new(launcher, registry)`.

- [ ] Pruebas: `a_new_message_starts_a_task`, `a_message_with_references_starts_a_follow_up`,
  `get_task_of_another_parent_is_not_found`, `list_tasks_returns_only_the_parents_children`.
- [ ] Implementar sobre `start`, `start_follow_up` y `list`; commitear.

### Tarea 3: reanudar, cancelar y tarjeta

**Ficheros:** `crates/codeloom-core/src/transport.rs`.

- [ ] Pruebas: `a_message_resumes_a_task_waiting_for_input`,
  `a_message_to_a_task_not_waiting_is_refused_and_delivers_nothing`,
  `a_resume_with_the_wrong_number_of_answers_is_refused`,
  `a_resume_with_a_non_text_part_is_refused`, `cancel_task_returns_the_canceled_task`,
  `agent_card_of_default_and_of_unknown`.
- [ ] Implementar sobre `answer`, `cancel` y el registro; commitear.

### Tarea 4: suite de contrato y documentos

**Ficheros:** `crates/codeloom-core/src/transport.rs` (módulo de pruebas) y las specs.

- [ ] `pub async fn transport_contract(t: &impl AgentTransport, ...)` (bajo `#[cfg(test)]`, reutilizable
  por A8 mediante una característica de prueba) que ejercite envío, consulta, listado, cancelación
  y errores; ejecutarla contra `InProcess`.
- [ ] Sección «Estado» de la spec y roadmap (A7 [x]); commitear.
