# El estado del subagente es `TaskState` de A2A — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. En esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** que el estado persistido y en memoria de un subagente sea exactamente el `TaskState` de A2A (con su `TaskStatus`), que el runtime produzca los ocho estados y que la interfaz derive su aspecto del modelo sin alterarlo.

**Arquitectura:** `TaskState` y `TaskStatus` viven en `codeloom-core::task`; `SubagentStatus` desaparece; el almacén guarda el estado con los nombres del proto v1.0 y lee las cadenas antiguas; las transiciones pasan por un único punto que valida las reglas de A2A y publica `SubagentChanged`; la TUI deriva `AgentState` (presentación) de `TaskState`.

**Spec:** `docs/specs/2026-10-22-codeloom-a2a-task-model-design.md` (**leerla entera antes**) y, para los productores, `docs/specs/2026-10-22-codeloom-a2a-states-producers-design.md`. Regla que manda: **no inventar nada que A2A no contemple**; lo que la spec rotula como lectura o simplificación nuestra no se amplía, y la presentación nunca reescribe el modelo.

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`; `batten` no depende de `codeloom-core`. Errores de librería con `thiserror`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`. Si `clippy` muere con «memory allocation failed», repetir; si persiste, `RUST_MIN_STACK=67108864`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz. No reescribir planes ni specs históricos.
- `JobStatus` (trabajos de `bash` y similares) **no se toca**: no son tareas A2A.

## Review Focus

- Una base de datos con filas de cadenas antiguas (`running`, `completed`, `failed`, `killed`, `interrupted`, `unfinished`) se lee sin error con el estado equivalente de la spec; y la migración de columnas es idempotente (dos aperturas seguidas no fallan).
- Una tarea en estado terminal no admite transición posterior (el intento devuelve error o se ignora de forma documentada y probada), tampoco desde `complete_subagent`.
- `complete_subagent` ya no escribe la cadena `'completed'` a pelo.
- Validación de entrada (prompt vacío o de más de 32 768 bytes, etiqueta de más de 128) **no** crea tarea; solo la capacidad agotada crea una tarea `rejected`.
- `rejected` y `auth-required` no ponen `wake_pending`; `input-required` mantiene el despertar de hoy.
- `auth-required` e `input-required` terminan siempre (también si el hijo se cancela o el padre aborta).
- Cero referencias a `SubagentStatus` en `crates/` al terminar (salvo comentarios históricos que lo nombren a propósito).
- La presentación (rail, actividad) cubre los ocho estados con `match` exhaustivo sin comodín.
- **Trazabilidad A2A perfecta (regla del usuario, ver la spec):** donde la interfaz agrupe o oculte estados, el texto visible nombra entre paréntesis los estados originales de A2A con sus nombres exactos. Hoy ocurre al menos en la cabecera de actividad del subagente en el transcript (`subagent_tool_status` en `app.rs` agrupa varios estados bajo un `ToolStatus`). Cada punto de agrupación tiene su test; si encuentras otros (`grep` de `ToolStatus` y de las etiquetas del rail), trátalos igual y cuéntalos en el informe.

---

### Tarea 1: `TaskState`/`TaskStatus` como estado del subagente (núcleo, almacén e interfaz)

**Ficheros:** `crates/codeloom-core/src/task.rs`, `session/mod.rs`, `session/sqlite.rs`, `subagent.rs`, `event/mod.rs`, `crates/codeloom-tui/src/{app,rail,render,run}.rs`, `crates/codeloom-cli` solo si el compilador lo exige.

**Interfaces — produce:**
```rust
// task.rs
pub enum TaskState { Submitted, Working, InputRequired, AuthRequired, Completed, Failed, Canceled, Rejected }  // ya existe; añadir:
impl TaskState {
    pub const ALL: [TaskState; 8];
    pub fn proto_name(self) -> &'static str;                 // "TASK_STATE_SUBMITTED", ... (nombres del proto v1.0)
    pub fn from_proto_name(name: &str) -> Option<TaskState>;
    pub fn can_transition_to(self, next: TaskState) -> bool; // false si self.is_terminal(); si no, true (A2A no da tabla de transiciones: no inventar restricciones)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatus { pub state: TaskState, pub message: Option<String>, pub timestamp_ms: i64 }   // proto TaskStatus; message como texto hasta A5
impl TaskStatus { pub fn new(state: TaskState) -> Self; pub fn with_message(self, message: impl Into<String>) -> Self; }   // timestamp = now_ms()
// session/mod.rs
SubagentDescriptor { parent, child, job_id, label, status: TaskStatus, result: Option<String>, notice_delivered: bool }
async fn update_subagent_status(&self, parent: SessionId, child: SessionId, status: TaskStatus) -> Result<()>;   // error de almacén si el estado actual es terminal
// event/mod.rs: SubagentSettled { parent, child, status: TaskState }
```
Eliminar `SubagentStatus`, `From<SubagentStatus>`, `From<JobStatus> for TaskState` si queda sin uso y `subagent_task_state`.

**Comportamiento:**
- `sqlite.rs`: columna `status` guarda `proto_name()`. Añadir `status_message TEXT` y `status_timestamp_ms INTEGER` a `subagents` con `ALTER TABLE ... ADD COLUMN` **solo si faltan** (consultar `PRAGMA table_info(subagents)`), también en bases nuevas (`CREATE TABLE` las incluye). Lectura: aceptar nombres del proto y las cadenas antiguas (`running`→`Working`, `completed`→`Completed`, `failed`→`Failed`, `killed`→`Canceled`, `interrupted`/`unfinished`→`Failed`); cadena desconocida → error de almacén. `complete_subagent` usa `Completed`. Al abrir el almacén, todo estado **no terminal** (`TASK_STATE_SUBMITTED`, `_WORKING`, `_INPUT_REQUIRED`, `_AUTH_REQUIRED` y la cadena antigua `running`) pasa a `TASK_STATE_FAILED` con mensaje `"interrupted by application shutdown"` y marca de tiempo.
- `subagent.rs`: usar `TaskStatus`/`TaskState` en lugar de `SubagentStatus` (estado inicial `Working` por ahora; los productores llegan en la tarea 2). `JobStatus::Killed` → `Canceled`, `Failed` → `Failed`, `Completed` → `Completed`. `control` considera «en marcha» todo estado no terminal.
- TUI: `App.subagents` conserva `SubagentDescriptor`; `active_agents()` cuenta los no terminales; el mapeo de actividad (`app.rs`, donde hoy se convierte el estado a `ToolStatus`) es un `match` exhaustivo sobre `TaskState` (`Submitted`/`Working`/`InputRequired`/`AuthRequired` → en curso; `Completed` → éxito; `Failed`/`Rejected` → error; `Canceled` → cancelado). `rail.rs`: `AgentState` se deriva de `TaskState` con un `match` exhaustivo (presentación). Mientras `awaiting_input` siga existiendo en la TUI se mantiene tal cual hasta la tarea 2.
- **Trazabilidad:** la cabecera de actividad de un subagente en el transcript añade tras su etiqueta el estado A2A exacto entre paréntesis (p. ej. `review (working)`), de modo que `ToolStatus` (visual, agrupado) nunca sustituya al estado real. Si en algún punto se muestra una agrupación sin estado concreto, lista los originales: `en curso (submitted, working, input-required, auth-required)`. Los nombres son los de A2A, en minúsculas y con guion (`input-required`). El rail conserva un glifo por estado (1:1); si el rail llegara a compartir glifo entre estados, aplica la misma regla.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `proto_names_roundtrip_for_all_states`, `from_proto_name_rejects_unknown`, `terminal_states_cannot_transition`, `non_terminal_states_can_transition_anywhere`, `task_status_new_sets_timestamp`, `state_roundtrips_through_sqlite_with_message_and_timestamp`, `legacy_strings_are_read_as_equivalent_states` (inserta cada cadena antigua por SQL), `store_migration_adds_status_columns_to_an_old_database_and_is_idempotent`, `update_from_terminal_state_is_rejected`, `complete_subagent_writes_the_proto_name`, `reopened_store_marks_non_terminal_children_failed_with_message`, `killed_job_becomes_canceled`, `active_agents_counts_only_non_terminal`, `activity_status_covers_all_eight_states`, `agent_state_covers_all_eight_states`, `activity_header_names_the_exact_a2a_state_in_parentheses` (uno por cada uno de los ocho estados), `grouped_status_text_lists_original_a2a_states`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, tests de los cuatro crates, clippy, rustfmt, LF, y `grep -rn "SubagentStatus" crates` (vacío). **Paso 4: no commitear.**

---

### Tarea 2: productores y transiciones (`submitted`, `rejected`, `input-required`, `auth-required`, `canceled`)

**Ficheros:** `crates/codeloom-core/src/{subagent,subagent_input,event/mod}.rs`, `crates/codeloom-core/src/tools/subagent.rs` (solo si hace falta), `crates/codeloom-tui/src/{app,rail}.rs`.

**Comportamiento (todo vía un único punto del lanzador que valida `can_transition_to`, persiste con `update_subagent_status` y publica `LiveEvent::SubagentChanged(descriptor)`):**
- `start`: el descriptor nace `Submitted` al reconocer la tarea; al comenzar a ejecutarse el cierre del trabajo pasa a `Working`. Un fallo al persistir se registra con `tracing::warn!` y no aborta al hijo.
- Capacidad agotada: se crea la sesión del hijo y un descriptor `Rejected` con mensaje `"maximum of 8 active subagents reached"` y `job_id` vacío; se publica `SubagentChanged` y se devuelve el error actual al padre. Los errores de validación fallan antes, sin crear nada.
- `input-required`: mientras `ChildResponder::ask` espera, el hijo está en `InputRequired` con el texto de la pregunta como mensaje; al resolverse (respuesta, descarte o cancelación, también por `Drop`) vuelve a `Working`. Se conservan los eventos `SubagentInputRequested/Resolved` para el despertar del padre.
- `auth-required`: mientras `ChildResponder::decide` espera al servicio de permisos del padre, el hijo está en `AuthRequired` con un resumen (herramienta y argumentos recortados) como mensaje; al resolverse vuelve a `Working`. No pone `wake_pending`. La decisión sigue en `PermissionService`.
- Cancelar (`interrupt` o el `Esc` del padre) lleva la tarea a `Canceled`; `Completed`/`Failed` como hoy.
- TUI: la marca `awaiting_input` se elimina y se deriva de `status.state == InputRequired`; la espera de autorización se deriva de `AuthRequired`. El glifo sale de `AgentState`. `wake_pending` solo lo activan `Completed`, `Failed` e `InputRequired` (como hoy), nunca `Rejected`, `Canceled` ni `AuthRequired`.

- [ ] **Paso 1: tests en rojo:** `launcher_saves_the_child_as_submitted_then_working`, `launcher_rejects_when_capacity_is_exhausted_and_records_a_rejected_task`, `launcher_validation_errors_create_no_task`, `ask_moves_the_child_to_input_required_and_back`, `ask_resolves_input_required_even_if_cancelled`, `decide_moves_the_child_to_auth_required_with_a_summary_and_back`, `decide_resolves_auth_required_even_if_cancelled`, `auth_summary_names_the_tool`, `interrupt_moves_the_task_to_canceled`, `settled_canceled_does_not_wake_the_parent`, `rejected_does_not_wake_the_parent`, `auth_required_does_not_wake_the_parent`, `input_required_still_wakes_the_parent`, `rail_derives_input_and_auth_from_the_task_state`.
- [ ] **Paso 2: implementar. Paso 3: verificar** igual que la tarea 1. **Paso 4: no commitear.**

---

### Tarea 3: showcase y documentación

**Ficheros:** `crates/batten/examples/showcase/demos/agent_states.rs`, `crates/batten/src/widgets/funcionalidad_table.rs`, `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`, `docs/specs/2026-10-22-codeloom-a2a-task-model-design.md`, `docs/specs/2026-10-22-codeloom-a2a-states-producers-design.md`.

- [ ] **Paso 1:** la columna «productor» del demo dice «runtime» para los ocho estados (ajustar su test) y la fila «Flock/subagentes» de la tabla de funcionalidades refleja que el runtime ya produce los ocho, sin afirmar más.
- [ ] **Paso 2:** en la spec A2A, sustituir el párrafo «proyección en el borde» de «Estado de tarea» por el modelo nuevo (el estado ES `TaskState`; nombres del proto persistidos; `Killed` y `Unfinished` ya no existen) conservando el historial como nota. En la hoja de ruta, marcar A1 y A2 como absorbidas por este plan y hechas, y anotar lo abierto. En la spec del modelo y en la de productores, añadir `## Estado (fecha)` con lo implementado, decisiones tomadas y limitaciones reales.
- [ ] **Paso 3: verificar** `cargo test -p batten`, `cargo build -p batten --examples`, rustfmt, LF. **Paso 4: no commitear.**
