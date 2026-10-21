# Observabilidad de agentes (Fase O2) — Plan de implementación

> **Para agentes:** sub-skill requerida: `superpowers:subagent-driven-development` o `superpowers:executing-plans`. Una tarea cada vez, en orden. Al terminar y verificar cada tarea se commitea (regla del usuario).

**Meta:** entrar en la sesión de un subagente desde el rail y volver al padre.

**Arquitectura:** se reutiliza `SessionChoice::Resume` (que ya carga una sesión del almacén). La TUI añade una pila de padres y la selección del rail.

**Stack:** Rust, `ratatui`. Solo se toca `crates/codeloom-tui`.

**Spec:** `docs/specs/2026-10-21-codeloom-agent-observability-o2-design.md` (leerla antes).

## Restricciones globales

- Comprobaciones: `cargo build`, `cargo test -p codeloom-tui`, `cargo clippy -p codeloom-tui -- -D warnings`, `rustfmt --edition 2021 --check <ficheros tocados>`.
- Entorno de build (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. No crear otros `target*`. Formatear solo con `rustfmt --edition 2021 <ruta>`, nunca `cargo fmt --all`. Ficheros en LF.
- Comentarios en español «pijo», sin tacos ni andalucismos.
- La fluidez del composer es prioridad máxima: nada de temporizadores ni trabajo extra por fotograma.
- No hacer `git checkout/restore/stash/reset` ni push.

## Review Focus

- Con un turno en curso, `open_child` y `go_parent` no hacen nada.
- Abrir una sesión desde el selector vacía la pila; navegar entre padre e hijo no.
- Sin subagentes, con foco en el rail `Enter` y las flechas siguen actuando sobre el composer.
- La selección del rail no apunta a un hijo que ya no existe tras recargar sesión.

---

### Tarea 1: estado y navegación en `App`

**Ficheros:** `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/commands.rs`.

**Interfaces — produce:**
```rust
// App (campos nuevos, `Default`/`new` a vacío)
pub rail_selected: Option<i64>,        // clave de `subagents`
pub parent_stack: Vec<SessionId>,
nav_keep_stack: bool,                  // privado
// App (métodos)
pub fn rail_move(&mut self, delta: isize);   // mueve entre los hijos de `subagents` (orden de BTreeMap), sin dar la vuelta; sin hijos pone None; None -> primero
pub fn open_child(&mut self, child: i64) -> bool;  // false (sin efecto) si turn_in_flight, no hay sesión, o `child` no está en `subagents`
pub fn go_parent(&mut self) -> bool;               // false (sin efecto) si turn_in_flight o la pila está vacía
// commands.rs
Command::Parent   // "/parent", "Back to the parent session"
```
`open_child`: apila `session_id`, pone `nav_keep_stack = true` y `session_choice = Some(SessionChoice::Resume(SessionId(child)))`. `go_parent`: desapila, `nav_keep_stack = true`, `session_choice = Some(Resume(padre))`. En `load_session`: si `nav_keep_stack` está activo se consume (`take`) y no se toca la pila; si no, `parent_stack.clear()`. Además `load_session` pone `rail_selected = None`. En `load_subagents`, si `rail_selected` ya no está en el mapa, se pone a `None`.

En `run.rs`, dentro de `handle_key`, **antes** de la rama de transcript/composer y tras los atajos globales existentes:
- `Alt+Left` (cualquier foco) → `app.go_parent()`.
- Foco `Rail` y `!app.subagents.is_empty()` y sin permiso pendiente: `Up`/`Down` → `rail_move(-1/1)`; `Enter` → si `rail_selected` es `Some(c)`, `open_child(c)`; en los tres casos `return None`. `Enter` sin selección no hace nada.
- El comando `/parent` ejecuta `go_parent()`; si devuelve `false` pone `app.status = "no parent session".into()`. Registrar el comando donde se despachan los demás (`Command::Parent`) y en `COMMANDS`.

- [ ] **Paso 1: tests en rojo** (en `app.rs` y `run.rs`; indicar si el rojo es de compilación o de aserción):
  - `open_child_pushes_parent_and_requests_resume`, `open_child_refuses_while_turn_in_flight`, `open_child_refuses_unknown_child`.
  - `go_parent_pops_and_requests_resume`, `go_parent_refuses_with_empty_stack`, `go_parent_refuses_while_turn_in_flight`.
  - `normal_load_session_clears_parent_stack` y `navigation_load_keeps_parent_stack` (tras `open_child`, `load_session` conserva la pila; una segunda `load_session` la vacía).
  - `rail_move_walks_children_without_wrapping`, `rail_selection_dropped_when_child_disappears`.
  - `rail_keys_select_and_open_child` y `rail_enter_without_subagents_still_submits_composer` (en `run.rs`).
  - `alt_left_goes_to_parent_with_any_focus` y `parent_command_without_stack_sets_status`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test -p codeloom-tui`, clippy, rustfmt, LF. **Paso 4: commit** (español pijo + trailer `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).

---

### Tarea 2: rail — marcador, línea de retorno y ayuda

**Ficheros:** `crates/codeloom-tui/src/rail.rs`, el texto de ayuda donde estén las teclas (buscar `[ / ]` en `render.rs`), `docs/specs/2026-10-21-codeloom-agent-observability-o2-design.md` (sección Estado).

**Interfaces — consume:** `App.rail_selected`, `App.parent_stack`, `App.focus` de la tarea 1.

- Línea de cada subagente del rail: si su id es `rail_selected` y `app.focus == Focus::Rail`, el glifo inicial pasa de `● ` a `› ` con `app.theme.styles.accent` (la anchura no cambia: dos columnas).
- Si `!app.parent_stack.is_empty()`, justo bajo el título `CODELOOM` (tras la línea del `cwd`) se añade la línea tenue (`styles.muted`) `↩ /parent · Alt+←`, recortada a la anchura del rail.
- La ayuda lista `Alt+←` (volver al padre) y, en la sección del rail, `↑/↓ + Enter` (abrir subagente) y `/parent`.
- Estado de la spec: lo implementado, decisiones tomadas y limitaciones (transcript no vivo; sin árbol dibujado).

- [ ] **Paso 1: tests en rojo** (con el mismo ayudante `rows_of` de `rail.rs`): `selected_subagent_shows_marker_only_when_rail_focused`, `rail_shows_return_hint_inside_a_child`, `rail_has_no_return_hint_at_top_level`, `return_hint_is_clipped_to_rail_width`.
- [ ] **Paso 2: implementar.** **Paso 3: verificar** `cargo test -p codeloom-tui`, clippy, rustfmt, LF. **Paso 4: commit.**
