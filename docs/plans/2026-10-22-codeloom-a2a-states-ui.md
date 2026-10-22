# Estados A2A de un agente en rail y showcase — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. En esta ejecución quien supervisa commitea, así que **no commitees**.

**Meta:** que el rail de loom y el showcase de `batten` pinten los ocho estados de tarea de A2A (`submitted`, `working`, `input-required`, `auth-required`, `completed`, `failed`, `canceled`, `rejected`), cada uno con glifo, etiqueta y color propios.

**Arquitectura:** `batten` (sin dependencia del core) define un enum visual `AgentState` con glifo, etiqueta y estilo; `codeloom-tui` proyecta `TaskState` (de `codeloom_core::task::subagent_task_state`) a `AgentState` y el rail lo usa en lugar de `SubagentStatus` más la marca ` ?`; el showcase muestra los ocho.

**Spec:** `docs/specs/2026-10-15-loom-agent-profile-a2a.md`, sección «Estado de tarea» (leerla antes).

## Restricciones globales

- `codeloom-core` no depende de `ratatui`/`crossterm`; `batten` no depende de `codeloom-core`. **Sin red en tests.**
- Comprobaciones: `cargo build`, `cargo test -p batten -p codeloom-tui -p codeloom-core -p codeloom-cli`, `cargo clippy -p batten -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`. Compilar también el ejemplo: `cargo build -p batten --examples`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF (`tr -cd '\r' < f | wc -c` debe dar 0).
- Comentarios en español «pijo», sin tacos ni andalucismos. No `git checkout/restore/stash/reset` ni push. No tocar `codeloom.toml` de la raíz.
- El modelo interno (`SubagentStatus`, esquema SQLite) **no cambia** en este plan.

## Review Focus

- Un hijo `Running` que espera input se ve como `input-required` (no como `working`); uno terminado con `awaiting_input` residual se ve como su estado final.
- `Interrupted` se ve como `failed`, por la proyección ya fijada en la spec.
- Los estados sin productor hoy (`submitted`, `auth-required`, `rejected`) se pintan igualmente en el showcase y en una prueba del rail con un `TaskState` fabricado, pero el runtime no los genera todavía.
- Glifos de una sola columna de anchura (la línea del rail no ensancha) y distinguibles entre sí sin color.
- El hijo seleccionado con `›` en el rail y la marca de espera siguen funcionando.

---

### Tarea 1: `AgentState` en batten y rail de loom

**Ficheros:** `crates/batten/src/state/` (módulo nuevo, p. ej. `agent_state.rs`, reexportado en `state/mod.rs`), `crates/codeloom-tui/src/rail.rs`, `crates/codeloom-tui/src/app.rs` solo si hace falta.

**Interfaces — produce:**
```rust
// batten::state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState { Submitted, Working, InputRequired, AuthRequired, Completed, Failed, Canceled, Rejected }
impl AgentState {
    pub const ALL: [AgentState; 8];
    pub fn glyph(self) -> &'static str;   // una columna: ○ ● ? ⚿ ✓ ✗ ⊘ ⊖ en ese orden (si alguno ocupa 2 columnas o falta en fuentes comunes, elegir otro y dejarlo anotado en un comentario)
    pub fn label(self) -> &'static str;   // «submitted», «working», «input-required», «auth-required», «completed», «failed», «canceled», «rejected»
}
// batten: estilo por estado a partir del perfil de estilos existente (fn style(self, styles: &<tipo de estilos del tema>) -> Style):
// Submitted→muted, Working→tool_running, InputRequired→warning, AuthRequired→warning, Completed→tool_success, Failed→tool_error, Canceled→tool_canceled, Rejected→tool_error.
// codeloom-tui: proyección TaskState → AgentState (función privada en rail.rs; match exhaustivo sin comodín).
```

**Comportamiento:**
- En `rail.rs`, el estado del hijo se calcula con `codeloom_core::task::subagent_task_state(subagent.status, app.awaiting_input.contains(&subagent.child.0))`, se proyecta a `AgentState` y se usa su glifo y estilo en lugar del `●` fijo y de la marca ` ?` posterior (esta última desaparece: el glifo `?` de `InputRequired` la sustituye). El hijo seleccionado con foco en el rail sigue llevando `›` de acento en lugar del glifo.
- Adaptar los tests existentes que comprueban la marca ` ?` (`rail_marks_subagents_waiting_for_input` y similares) a la nueva forma, sin perder su intención.

- [ ] **Paso 1: tests en rojo** (indicar si el rojo es de compilación o de aserción): en batten, `agent_state_all_has_eight_unique_glyphs_and_labels`, `agent_state_glyphs_are_one_column_wide`, `agent_state_style_maps_each_state`; en codeloom-tui, `rail_shows_input_required_glyph_for_waiting_running_child`, `rail_shows_failed_glyph_for_interrupted_child`, `rail_ignores_awaiting_input_for_finished_child`, `task_state_projection_covers_every_state` (incluye `Submitted`, `AuthRequired` y `Rejected` construidos a mano), `rail_selected_child_keeps_arrow_marker`.
- [ ] **Paso 2: implementar. Paso 3: verificar** `cargo build`, tests de los cuatro crates, clippy, rustfmt, LF. **Paso 4: no commitear.**

---

### Tarea 2: los ocho estados en el showcase

**Ficheros:** `crates/batten/examples/showcase/demos/` (demo nuevo o ampliación del más afín; localizar cómo se muestran los estados de herramienta con `tool_atom_fixtures.rs` y `demos/mod.rs`), `crates/batten/examples/showcase/app.rs` si hay registro de demos, `crates/batten/src/widgets/funcionalidad_table.rs` si lista funcionalidades.

**Comportamiento:** una vista del showcase que liste los ocho `AgentState::ALL` con glifo, etiqueta y su estilo, una fila por estado, siguiendo el patrón de los demás demos (registro, título, ayuda). Si la tabla de funcionalidades tiene una fila «Flock/subagentes» que dice «No hay runtime de subagentes implementado», actualizarla para reflejar que ahora existen los estados (sin afirmar más de lo cierto: el runtime produce cinco de los ocho).

- [ ] **Paso 1: tests en rojo:** `agent_states_demo_lists_all_eight_states` (el demo, pintado en un buffer, contiene las ocho etiquetas), `agent_states_demo_is_registered`.
- [ ] **Paso 2: implementar. Paso 3: verificar** igual que la tarea 1 más `cargo build -p batten --examples`. **Paso 4: no commitear.**
- [ ] **Paso 5: docs.** En la spec A2A, sección «Estado de tarea», un párrafo: los ocho estados se pintan en el rail y en el showcase; el runtime solo produce `working`, `input-required`, `completed`, `failed` y `canceled`; `submitted`, `rejected` y `auth-required` quedan pendientes de un productor (ver siguiente paso: variantes nuevas en `SubagentStatus` con su cadena en SQLite, sin cambio de esquema).
