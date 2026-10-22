# Observabilidad O3 (causalidad y ficheros por hijo) — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. En esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** que el transcript diga quién despertó al padre y que el rail y el panel de cambios muestren los ficheros editados por cada hijo.

**Arquitectura:** `FileDiff` pasa a reenviarse de los hijos; la TUI acumula ficheros por sesión (`files_by_session`) y la causa del último despertar (`wake_cause`). Sin cambios de esquema ni de persistencia.

**Spec:** `docs/specs/2026-10-22-codeloom-agent-observability-o3-design.md` (leerla antes).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p codeloom-core -p codeloom-tui -p codeloom-cli`, `cargo clippy -p codeloom-core -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz.

## Review Focus

- El filtro de eventos del hijo sigue sin dejar pasar `Delta`, `ReasoningDelta`, `MessageAdded` ni `JobChanged`.
- El `FileDiff` de un hijo **no** toca `app.changes` del padre ni sus contadores.
- Ida y vuelta padre→hijo→padre: los ficheros del padre reaparecen en el panel.
- Cargar una sesión ajena a la navegación vacía `files_by_session`.
- Causa con hijo desconocido, y con varias causas pendientes.

---

### Tarea 1: ficheros por hijo

**Ficheros:** `crates/codeloom-core/src/event/mod.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/rail.rs`.

**Interfaces — produce:**
```rust
// event/mod.rs: LiveEvent::FileDiff pasa a ser relevante para hijos (is_child_relevant → true); actualizar el test de inclusión/exclusión.
// App
pub files_by_session: HashMap<i64, BTreeMap<String, (usize, usize)>>;   // sesión → ruta → (añadidas, quitadas); la última gana
pub fn child_file_count(&self, child: SessionId) -> usize;
// rail.rs
fn metrics_line(stats: &AgentStats, cost: Option<String>, files: usize, now: Instant, width: usize) -> String;   // grupo `Nf` tras el coste y antes del tiempo; solo si files > 0 y hay started_at
```

**Comportamiento:**
- `apply` de `LiveEvent::FileDiff` propio: además de `changes.upsert`, anota en `files_by_session[session_id]` (si hay sesión abierta).
- `apply_child_event`: un `FileDiff` de hijo anota en `files_by_session[child.0]` y **no** toca `changes`. Si `apply_child_event` filtra por `is_child_relevant`, ya pasará al cambiar el filtro; cuidar que el resto de variantes sigan igual.
- `load_session` (donde se reinicia `self.changes`, hacia la línea 1183): si no es navegación (`nav_keep_stack` falso al entrar), vaciar `files_by_session`; después, rellenar `self.changes` con `files_by_session` de la sesión cargada (`FileChange { path, added, removed }`). El orden de los ficheros es el del `BTreeMap`. Averiguar si en ese punto `nav_keep_stack` ya se ha consumido con `std::mem::take` (línea ~1270) y respetar el orden.
- `rail.rs`: pasar `app.child_file_count(subagent.child)` a `metrics_line`; actualizar los tests existentes de `metrics_line` con `0`.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): `file_diff_is_forwarded_from_children` (en el test de inclusión de eventos), `child_file_diff_accumulates_by_child_and_keeps_last_per_path`, `child_file_diff_does_not_touch_parent_changes`, `own_file_diff_is_recorded_for_the_open_session`, `parent_files_return_to_changes_panel_after_child_roundtrip`, `loading_an_unrelated_session_clears_files_by_session`, `metrics_line_places_file_count_between_cost_and_elapsed`, `rail_shows_file_count_for_a_child`, `rail_omits_file_count_without_files`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, tests de los tres crates, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: causa del despertar en el transcript

**Ficheros:** `crates/codeloom-tui/src/app.rs`.

**Interfaces — produce:**
```rust
// App
pub wake_causes: Vec<(i64, &'static str)>;   // (hijo, motivo) pendientes desde el último despertar; motivos: "completed" | "failed" | "asks input"
fn wake_cause_line(&self) -> Option<String>;  // «⟳ auto-turn · woken by «review» (completed)» ; con varias, la más reciente y « (+N more)»; hijo desconocido: «agent #<id>»
```

**Comportamiento:**
- Donde hoy `SubagentSettled` (Completed/Failed) y `SubagentInputRequested` ponen `wake_pending = true`, se añade además la causa a `wake_causes`.
- En `LiveEvent::TurnStarted` con `woken == true`, antes de limpiar nada, se emite en el transcript la línea de `wake_cause_line` con el mecanismo que la TUI ya usa para líneas informativas (localizar cómo se muestran avisos del sistema, p. ej. `push_notice_entry` u otro helper; reutilizarlo, no inventar un widget) y se vacía `wake_causes`. Un turno **no** despertado no emite línea y no toca `wake_causes`.
- `wake_causes` se vacía donde ya se descarta `wake_pending` por Esc o por recarga de sesión.
- El texto es una línea de interfaz: **no** entra en el historial del modelo ni se persiste.

- [ ] **Paso 1: tests en rojo:** `settled_child_records_wake_cause`, `input_request_records_wake_cause`, `killed_child_records_no_cause`, `woken_turn_emits_cause_line_and_clears_causes`, `user_turn_emits_no_cause_line`, `cause_line_with_several_causes_shows_latest_and_count`, `cause_line_uses_agent_id_for_unknown_child`, `escape_clears_wake_causes`.
- [ ] **Paso 2: implementar. Paso 3: verificar** igual que la tarea 1. **Paso 4: no commitear.**
- [ ] **Paso 5: docs.** Sección `## Estado` en la spec de O3 (implementado, decisiones tomadas, limitaciones reales) y una línea en `docs/specs/2026-10-21-codeloom-agent-observability-design.md` marcando O3 como implementada.
