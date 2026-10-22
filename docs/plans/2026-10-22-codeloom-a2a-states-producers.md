# Productores de estados A2A (`submitted`, `rejected`, `auth-required`) — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. En esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** que el runtime emita `submitted`, `rejected` y `auth-required` donde la especificación de A2A los define o ejemplifica, sin inventar nada más.

**Arquitectura:** dos variantes nuevas de `SubagentStatus` (`Submitted`, `Rejected`) con su cadena en SQLite y su proyección a `TaskState`; el lanzador de hijos reconoce la tarea como `Submitted` y la pasa a `Running` al arrancar, y registra una tarea `Rejected` si se agota la capacidad; `auth-required` es transitorio (eventos del bus, como `input-required`) y lo marca `ChildResponder::decide` mientras espera al padre.

**Spec:** `docs/specs/2026-10-22-codeloom-a2a-states-producers-design.md` (**leerla entera antes**; manda la regla «no inventar nada que no contemple A2A»; lo que la spec rotula como lectura nuestra o como nuestro no se amplía).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`; `batten` no depende de `codeloom-core`. Errores de librería con `thiserror`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p batten -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz. No reescribir planes ni specs históricos.
- Esquema SQLite sin cambios (`status TEXT NOT NULL`).

## Review Focus

- Los errores de **validación** del lanzador (prompt vacío o de más de 32 768 bytes, etiqueta de más de 128) siguen siendo error y **no** crean tarea ni descriptor.
- `Submitted` cuenta como «en marcha» donde hoy se mira `Running` para permitir `interrupt`/`send_message` (`control`), para marcar al reabrir como `Unfinished`, y para `active_agents()`. Revisar **cada** uso de `SubagentStatus::Running` (`grep -rn "SubagentStatus::Running" crates`) y decidir caso por caso; los que no cambien, dejarlos con un comentario si no es evidente.
- `Rejected` y `auth-required` **no** ponen `wake_pending` (el padre ya ve el error y el diálogo).
- Bases de datos existentes siguen leyéndose; las cadenas nuevas no rompen versiones previas de lectura de `Unfinished`/`interrupted`.
- `auth-required` termina siempre: al resolverse el permiso, también si el hijo se cancela o el padre aborta (como `ResolvedOnDrop` hace con `input-required`).
- La proyección sigue siendo un `match` exhaustivo sin comodín.

---

### Tarea 1: `Submitted` y `Rejected` en el modelo y en el lanzador

**Ficheros:** `crates/codeloom-core/src/session/mod.rs`, `session/sqlite.rs`, `crates/codeloom-core/src/task.rs`, `crates/codeloom-core/src/subagent.rs`, `crates/codeloom-core/src/tools/subagent.rs` (solo si hace falta), `crates/codeloom-tui/src/app.rs`, `rail.rs`, `render.rs` (solo lo que fuerce el compilador y los tests).

**Interfaces — produce:**
```rust
// session/mod.rs
pub enum SubagentStatus { Running, Completed, Killed, Failed, Unfinished, Submitted, Rejected }   // las dos nuevas al final
// sqlite.rs: cadenas "submitted" y "rejected" (escritura y lectura)
// task.rs: Submitted => TaskState::Submitted, Rejected => TaskState::Rejected (match exhaustivo, sin comodín)
```

**Comportamiento:**
- En `InProcessSubagentLauncher::start`: el descriptor se guarda con `status: Submitted` (en lugar de `Running`). Al comenzar a ejecutarse el cierre del trabajo (antes de `child_agent.run`), se persiste `Running` con `update_subagent_status` y se publica `LiveEvent::SubagentChanged(<descriptor con Running>)` para que la TUI lo vea. Un fallo al persistir se registra con `tracing::warn!` y no aborta el hijo.
- Capacidad agotada (`try_acquire_owned` falla): se crea la sesión del hijo, se guarda un descriptor con `status: Rejected`, `job_id: ""` y se publica `SubagentChanged` con él; después se devuelve el mismo error actual (`"maximum of 8 active subagents reached"`). Los errores de validación **no** pasan por aquí: siguen fallando antes, sin crear nada.
- `control`: `Submitted` se trata como en marcha. La reapertura del almacén marca como `Unfinished` tanto las filas `running` como las `submitted`.
- TUI: el `match` de actividad (`app.rs`, donde hoy `Killed | Unfinished => ToolStatus::Canceled`) asigna `Submitted` → el mismo estado que `Running` y `Rejected` → error. `active_agents()` cuenta `Submitted` y `Running`. `Rejected` no pone `wake_pending`.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `submitted_and_rejected_roundtrip_through_sqlite`, `submitted_projects_to_task_state_submitted`, `rejected_projects_to_task_state_rejected`, `launcher_saves_the_child_as_submitted_then_running`, `launcher_rejects_when_capacity_is_exhausted_and_records_a_rejected_task` (comprueba descriptor `Rejected`, evento `SubagentChanged` y error devuelto), `launcher_validation_errors_create_no_task`, `control_accepts_a_submitted_child`, `reopened_store_marks_submitted_children_unfinished`, `rejected_does_not_wake_the_parent`, `active_agents_counts_submitted`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, tests de los cuatro crates, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: `auth-required` mientras el hijo espera un permiso

**Ficheros:** `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-core/src/subagent_input.rs`, `crates/codeloom-core/src/task.rs`, `crates/codeloom-tui/src/app.rs`, `rail.rs`.

**Interfaces — produce:**
```rust
// event/mod.rs: dos eventos nuevos (nombres a elegir; no relevantes para hijos y sin efecto en wake_pending)
LiveEvent::SubagentAuthRequested { parent: SessionId, child: SessionId, summary: String }   // summary: herramienta y argumentos resumidos
LiveEvent::SubagentAuthResolved  { parent: SessionId, child: SessionId }
// task.rs: ampliar la proyección con la espera de autorización; p. ej.
pub fn subagent_task_state(status: SubagentStatus, awaiting_input: bool, awaiting_auth: bool) -> TaskState   // Running + awaiting_auth => AuthRequired; Running + awaiting_input => InputRequired; el resto como hoy
// App: awaiting_auth: HashMap<i64, String>  // hijo → resumen del mensaje de estado
```

**Comportamiento:**
- `ChildResponder::decide` publica `SubagentAuthRequested` antes de `self.parent_service.check(...)` y `SubagentAuthResolved` al terminar, **también si la futura se cancela** (guarda con `Drop`, como `ResolvedOnDrop`). El `summary` incluye el nombre de la herramienta y un resumen corto de sus argumentos, recortado.
- La TUI guarda/quita la marca en `awaiting_auth` (solo para hijos de la sesión abierta; un hijo terminado la pierde) y la pasa a `subagent_task_state` en el rail. Un `?` ya es el glifo de `input-required`; `auth-required` usa el suyo de `AgentState` (`⚿`).
- `auth-required` **no** pone `wake_pending`. La decisión de permiso no cambia: sigue en `PermissionService`.
- Mostrar el `summary` en la línea del hijo del rail no es obligatorio; basta con guardarlo en `awaiting_auth` y exponerlo con un método de `App` probado.

- [ ] **Paso 1: tests en rojo:** `child_responder_publishes_auth_requested_then_resolved`, `child_responder_resolves_auth_even_if_cancelled`, `auth_summary_names_the_tool`, `subagent_task_state_auth_required_when_running_and_awaiting_auth`, `subagent_task_state_ignores_auth_when_finished`, `auth_requested_marks_the_child_without_waking_the_parent`, `auth_resolved_clears_the_mark`, `rail_shows_auth_required_glyph`.
- [ ] **Paso 2: implementar. Paso 3: verificar** igual que la tarea 1. **Paso 4: no commitear.**

---

### Tarea 3: showcase y documentación

**Ficheros:** `crates/batten/examples/showcase/demos/agent_states.rs`, `crates/batten/src/widgets/funcionalidad_table.rs`, `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, `docs/plans/2026-10-22-codeloom-a2a-roadmap.md`, `docs/specs/2026-10-22-codeloom-a2a-states-producers-design.md`.

- [ ] **Paso 1:** la columna «productor» del demo dice «runtime» para los ocho estados; ajustar su test (`agent_states_demo_lists_all_eight_states`) y la fila «Flock/subagentes» de la tabla de funcionalidades (el runtime produce ya los ocho; sin afirmar más).
- [ ] **Paso 2:** en la spec A2A, actualizar el párrafo «los ocho estados se pintan…» con lo que produce el runtime y por qué; en la hoja de ruta, marcar A2 como hecha y anotar lo que quede abierto; en la spec de productores añadir `## Estado (fecha)` con lo implementado, decisiones tomadas y limitaciones reales.
- [ ] **Paso 3: verificar** `cargo test -p batten`, `cargo build -p batten --examples`, rustfmt, LF. **Paso 4: no commitear.**
