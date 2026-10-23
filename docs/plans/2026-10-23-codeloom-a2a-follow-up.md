# Tareas enlazadas (A11): plan de implementación

> **Para quien ejecute:** una tarea cada vez; tras cada una, `cargo test -p codeloom-core -p
> codeloom-tui`, `cargo clippy -p codeloom-core -p codeloom-tui -p codeloom-cli -- -D warnings`,
> `rustfmt --edition 2021 --check` sobre los ficheros tocados y un commit. Nada de `cargo fmt --all`.

**Objetivo:** que el padre pueda lanzar un hijo como seguimiento de otro terminado, con el
resultado de este en su prompt y el enlace guardado.

**Spec:** `docs/specs/2026-10-23-codeloom-a2a-follow-up-design.md`.

## Restricciones globales

- `codeloom-core` no depende de `ratatui` ni de `crossterm`. Errores de librería con `thiserror`.
- Sin red real en tests. Nada sin cota llega al modelo (D4: 8 000 bytes).
- Textos para el modelo en inglés; comentarios en español.

## Revisión: lo que la spec implica y ninguna tarea cubre sola

- Un `follow_up_of` hacia una tarea viva no lanza nada (D3).
- Un `follow_up_of` hacia el hijo de otro padre no revela que existe (`TaskNotFound`).
- Un resultado de 100 KB no entra entero al prompt (D4).
- Un descriptor antiguo, sin la columna nueva, se lee con la lista vacía.
- El hijo nuevo comparte `context_id` con el anterior (D5).

### Tarea 1: el descriptor guarda las referencias

**Ficheros:** `crates/codeloom-core/src/session/mod.rs`, `session/sqlite.rs` y todo literal de
`SubagentDescriptor` que el compilador señale (tests de `core` y `tui`).

**Interfaces:** produce `SubagentDescriptor.reference_task_ids: Vec<SessionId>`, persistido en
`subagents.reference_task_ids` como JSON (`[3,5]`); ausente o `NULL` se lee como vacío.

- [x] Test `references_roundtrip_through_sqlite` y `an_old_row_without_references_reads_empty`
  (siguiendo el de `agent_origin` en `sqlite.rs`).
- [x] Añadir el campo, la columna con su migración `ALTER TABLE`, la escritura y la lectura.
- [x] Verificar y commitear.

### Tarea 2: el lanzador arranca un seguimiento

**Ficheros:** `crates/codeloom-core/src/subagent.rs`.

**Interfaces:** consume la tarea 1. Produce en el trait
`async fn start_follow_up(&self, parent, label, prompt, agent: Option<String>, follow_up_of: SessionId, cancel) -> Result<(SessionId, JobId)>`
(por defecto, error «subagent follow-up unavailable») y su implementación en
`InProcessSubagentLauncher`. `start` sigue igual y delega con referencias vacías.

- [x] Tests (nombre y aserciones):
  - `a_follow_up_carries_the_previous_result_in_its_prompt`: tras un hijo `completed`, el hijo
    nuevo recibe un primer mensaje que empieza por `[Follow-up of subagent`, contiene el
    resultado previo y termina con la petición; su descriptor guarda la referencia.
  - `a_follow_up_shares_the_context_id`: `context_id()` de ambos descriptores es el mismo.
  - `a_follow_up_of_a_running_child_is_refused`: error de herramienta, no se crea ningún hijo.
  - `a_follow_up_of_another_parents_child_is_not_found`: `TaskNotFound`, no se crea ningún hijo.
  - `the_previous_result_is_capped`: un resultado de 100 000 bytes entra con menos de
    `MAX_NOTICE_BYTES` + 400 bytes de encabezado.
- [x] Implementar: validar (`TaskNotFound`, terminal), construir el prompt (D4) con
  `limit_bytes`, guardar `reference_task_ids` y arrancar como `start`.
- [x] Verificar y commitear.

### Tarea 3: la herramienta `subagent`

**Ficheros:** `crates/codeloom-core/src/tools/subagent.rs`.

- [x] Tests: `follow_up_of` válido llama a `start_follow_up` con el id; `follow_up_of` ≤ 0 o no
  entero es un error de herramienta; sin `follow_up_of` se llama a `start` como antes.
- [x] Esquema (`follow_up_of`: entero, mínimo 1), descripción y `run`.
- [x] Verificar y commitear.

### Tarea 4: documentos

**Ficheros:** la spec (sección «Estado»), la del autonomía (Fase C), la del perfil A2A
(`reference_task_ids` ya no «nada lo escribe») y el roadmap (A11).

- [x] Estado real, commits, lo no visto en terminal real y lo que queda. Commitear.
