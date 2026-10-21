# Layout y chrome por look (rail derecho de Crush) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que un preset cambie también la estructura de la pantalla: con el look Crush, un único rail derecho (sin sidebar izquierdo ni métricas), cabecera de una línea, chat sin marco y pie de atajos cortos.

**Architecture:** `Look` gana `layout: LayoutStyle` y `chrome: ChromeStyle`. `render.rs` despacha según ellos; el rail vive en un módulo nuevo `codeloom-tui/src/rail.rs`. Classic conserva exactamente el layout actual.

**Tech Stack:** Rust 2021, ratatui. Sigue `docs/plans/2026-10-20-codeloom-look-presets.md`.

**Spec:** `docs/specs/2026-10-20-codeloom-look-presets-design.md`.

## Global Constraints

- Tras cada tarea: `cargo build`, `cargo test` (workspace), `cargo clippy -- -D warnings` sin avisos nuevos, `rustfmt --edition 2021 --check` sobre los ficheros tocados (nunca `cargo fmt --all`).
- Entorno (Git Bash): `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe' CARGO_TARGET_DIR='E:/workspace/personal/loom/target' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0`. `E:` está casi lleno: **no crear otros directorios `target*`**.
- **No hacer commits ni `git add`.** Prohibido `git checkout/restore/stash/reset`.
- `codeloom-core` no se toca. Sin red en tests.
- El look Classic no cambia: los tests existentes de `render.rs` y `app.rs` (incluidos `hides_sidebar_when_narrow`, `shows_sidebar_when_wide`, `shows_modified_files_when_wide`, `ctrl_b_sidebar_is_visible_at_medium_width`) deben seguir verdes **sin modificarlos**. Si alguno rompe, arregla tu código; si crees que uno debe cambiar, no lo cambies y repórtalo.
- Comentarios en español de registro cuidado, sin andalucismos ni tacos.
- Decisión del usuario: el rail aparece solo si hay ancho (`>= 80` columnas en el área central) y Ctrl+B (y `/sidebar`) lo muestra/oculta mientras haya ancho.

## Review Focus

1. Terminal estrecha (< 80) con Crush: no hay rail y el chat ocupa todo; Ctrl+B no rompe nada (T2: `crush_without_width_for_the_rail_uses_the_whole_screen`).
2. Rail con pocas filas de alto: se recorta sin pánico (T2: `rail_does_not_panic_on_tiny_heights`).
3. Ctrl+B con look Classic sigue alternando el sidebar izquierdo como hoy, y con Crush alterna el rail (T2: `toggle_sidebar_follows_the_look`).
4. Cambiar de preset con `/theme` a mitad de sesión no deja el estado del otro layout activo (T2: el mismo test).
5. Listas largas (cientos de ficheros/tools/jobs) no desbordan ni ralentizan el rail (T2: `rail_caps_long_lists`).

---

### Task 1: `batten`: `LayoutStyle` y `ChromeStyle` en `Look`

**Files:**
- Modify: `crates/batten/src/widgets/theme.rs`
- Modify: `crates/batten/src/widgets/mod.rs` (re-export)

**Interfaces:**
- Produces en `batten::widgets`: `enum LayoutStyle { Workspace, ContextRail }` y `enum ChromeStyle { Panels, Flat }` (`Debug, Clone, Copy, PartialEq, Eq`); `Look` pasa a tener los campos `composer`, `tool`, `layout`, `chrome`. `Look::classic()` → `Workspace`/`Panels`; `Look::crush()` → `ContextRail`/`Flat`. Ambas siguen siendo `const fn`.

- [ ] **Step 1: Tests** en `theme.rs`: ampliar `only_the_crush_preset_has_its_own_look` (sin cambiar lo que ya asserta) con:

```rust
assert_eq!(Look::classic().layout, LayoutStyle::Workspace);
assert_eq!(Look::classic().chrome, ChromeStyle::Panels);
assert_eq!(Look::crush().layout, LayoutStyle::ContextRail);
assert_eq!(Look::crush().chrome, ChromeStyle::Flat);
```
- [ ] **Step 2:** `cargo test -p batten theme` → no compila.
- [ ] **Step 3:** Implementar los enums (con doc breve en español), los campos y las dos `const fn`. Reexportar `LayoutStyle` y `ChromeStyle` en `widgets/mod.rs`. Si algún sitio construye `Look { .. }` literal, ajústalo.
- [ ] **Step 4:** `cargo test -p batten` verde; rustfmt/clippy de batten.

---

### Task 2: `codeloom-tui`: rail derecho, cabecera plana, chat sin marco y pie de atajos

**Files:**
- Create: `crates/codeloom-tui/src/rail.rs`
- Modify: `crates/codeloom-tui/src/lib.rs` (`pub mod rail;`)
- Modify: `crates/codeloom-tui/src/app.rs` (campo `rail_hidden` y `toggle_sidebar`)
- Modify: `crates/codeloom-tui/src/render.rs`

**Interfaces:**
- Consumes: T1 (`Look.layout`, `Look.chrome`).
- Produces: `App.rail_hidden: bool` (público, `false` por defecto, también en `with_context`); `App::toggle_sidebar` con look `ContextRail` alterna `rail_hidden` y no toca `sidebar_expanded`; con otro look hace lo de siempre. `pub fn draw_context_rail(frame: &mut Frame, area: Rect, app: &App)` en `rail.rs`.

**Comportamiento**

*Layout (`render()` en `render.rs`):* con `LayoutStyle::Workspace` todo igual que hoy. Con `ContextRail`, sea `body = rows[1]`: si `body.width >= 80 && !app.rail_hidden` → `[chat, rail] = Layout::horizontal([Min(1), Length(32)])`, `draw_transcript(chat)` y `draw_context_rail(rail)`; si no → `draw_transcript(body)`. Nunca se dibujan el sidebar izquierdo ni las métricas en este layout.

*Chrome `Flat`:*
- Cabecera de **una** línea (`Constraint::Length(1)` en vez de 2): `" CODELOOM "` (heading en negrita, `accent`) + `"╱╱╱╱"` (muted) + `"  {cwd}"` + `"  • {model}"` + `"  • {estado}"`, donde `estado` es `app.status` tal cual (`ready`, `working`, `error: …`); más, solo si aplica, `"  • {n} queued"`, `"  • queue paused"` (warning) y `"  • {s} subagents · {j} jobs"` con los mismos contadores que `job_counts`. Reutiliza `job_counts`. Sin chip y sin segunda línea.
- `draw_transcript`: no se pinta el `Panel` "Conversation"; `inner` = `area` con 1 columna de margen a cada lado (`x + 1`, `width - 2`, con `saturating_sub`). La fila interna "CodeLoom · project conversation · tema" se oculta cuando `chrome == Flat` (hoy se oculta con `composer == Crush`; cámbialo a `chrome == Flat`, equivalente para Crush).
- Pie: si hay `slash_feedback`, ese texto (muted) y nada más; si no, los atajos como pares `(tecla en heading, etiqueta en muted)` separados por dos espacios: `enter send`, `ctrl+j newline`, `ctrl+k queue`, `ctrl+o fold`, `ctrl+t tools`, `ctrl+b sidebar`, `f2 settings`, `esc cancel`. Con `Panels` el pie actual no cambia. El texto especial de las sugerencias slash que se pinta al final de `render()` sobre `rows[2]` se mantiene para ambos chromes.

*Rail (`rail.rs`):* `Paragraph` con `Block::default().borders(Borders::LEFT).border_style(app.theme.styles.border)`, sin cajas internas, contenido (cada sección se omite si está vacía, con una línea en blanco entre secciones):
1. `CODELOOM` (accent, negrita) y debajo `app.cwd` (muted).
2. `MODEL` (accent, negrita), `app.model`, `"{phase} · {s:.1}s"` (muted; `s = app.phase_duration()`), `"step {cur}/{max}"` solo si `app.max_steps > 0`, y `"↑{input_tokens} ↓{output_tokens}"` (muted, totales de sesión).
3. `FILES` si `!app.changes.is_empty()`: hasta 8 líneas `"● {path}  +{added}  -{removed}"` (glifo `●` con estilo `success`); si hay más, una línea muted `"… +{n} more"`. Para iterar usa la API real de `FileChanges` (léela en `crates/batten/src/state/file_changes.rs`).
4. `TOOLS` si `!app.tools.is_empty()`: las últimas 6, `"● {name}"`.
5. `JOBS`: jobs de la sesión abierta (`owner == session_id`, `kind != "subagent"`), hasta 5, `"● {label}"`, más `"… +{n} more"` si sobran.
6. `SUBAGENTS`: subagentes cuyo `parent == session_id`, hasta 5, `"● {label}"`, más `"… +{n} more"`.
Estilo del glifo `●`: Tools: `Pending` → `tool_pending`, `Running` → `tool_running`, `Success` → `tool_success`, `Error` → `tool_error`, `Canceled` → `tool_canceled`, `AwaitingPermission` → `warning`. Jobs: `Running|Stopping` → `tool_running`, `Completed` → `tool_success`, `Failed` → `tool_error`, `Killed` → `tool_canceled`. Subagentes: `Running` → `tool_running`, `Completed` → `tool_success`, `Failed` → `tool_error`, `Killed|Interrupted` → `tool_canceled`. El contenido se recorta al alto del área (no hace falta scroll) y no debe entrar en pánico con altura 0 ni con anchos mínimos.

*`App`:* `toggle_sidebar` — `if self.theme.look.layout == batten::widgets::LayoutStyle::ContextRail { self.rail_hidden = !self.rail_hidden } else { self.sidebar_expanded = !self.sidebar_expanded }`.

- [ ] **Step 1: Tests** en `render.rs` (`mod tests`, con `buffer_of`; helper `fn crush_ctx() -> App` = `App::with_context("/proj", "mistral-nemo")` con `theme_preset = Crush` y `theme = Theme::preset(Crush)`):

```rust
#[test]
fn crush_layout_has_only_a_right_rail_when_wide() {
    let mut app = crush_ctx();
    let content = buffer_of(&mut app, 120, 40);
    assert!(content.contains("MODEL"));
    assert!(content.contains("mistral-nemo"));
    assert!(!content.contains("PROJECT"));      // secciones del sidebar izquierdo
    assert!(!content.contains("FLOCK"));
    assert!(!content.contains("Token usage"));  // panel de métricas
    assert!(!content.contains("Conversation")); // panel del chat
}

#[test]
fn crush_without_width_for_the_rail_uses_the_whole_screen() {
    let mut app = crush_ctx();
    let content = buffer_of(&mut app, 70, 24);
    assert!(!content.contains("MODEL"));
    assert!(content.contains("CODELOOM"));
}

#[test]
fn toggle_sidebar_follows_the_look() {
    let mut app = crush_ctx();
    assert!(buffer_of(&mut app, 120, 30).contains("MODEL"));
    app.toggle_sidebar();
    assert!(app.rail_hidden && !app.sidebar_expanded);
    assert!(!buffer_of(&mut app, 120, 30).contains("MODEL"));
    app.toggle_sidebar();
    assert!(buffer_of(&mut app, 120, 30).contains("MODEL"));
    let mut classic = App::with_context("/proj", "m");
    classic.toggle_sidebar();
    assert!(classic.sidebar_expanded && !classic.rail_hidden);
}

#[test]
fn crush_header_is_one_flat_line() {
    let mut app = crush_ctx();
    let content = buffer_of(&mut app, 120, 30);
    let first = content.lines().next().unwrap();
    assert!(first.contains("CODELOOM"));
    assert!(first.contains("/proj"));
    assert!(first.contains("ready"));
    assert!(!content.contains("WORKSPACE"));
}

#[test]
fn crush_footer_shows_short_key_hints() {
    let mut app = crush_ctx();
    let content = buffer_of(&mut app, 120, 30);
    assert!(content.contains("enter send"));
    assert!(content.contains("ctrl+j newline"));
}

#[test]
fn rail_lists_files_tools_jobs_and_subagents() {
    // Crush, ancho 120: tras un FileDiff ("src/main.rs", 3, 1), un ToolStarted ("c1","View"),
    // un JobChanged (job de la sesión abierta, label "build", Running) y un SubagentChanged
    // (parent = sesión abierta, label "review", Running), el contenido contiene
    // "FILES", "src/main.rs", "TOOLS", "View", "JOBS", "build", "SUBAGENTS", "review".
}

#[test]
fn rail_caps_long_lists() {
    // 40 FileDiff distintos: contiene "… +32 more" y no contiene el path del fichero nº 20.
}

#[test]
fn rail_does_not_panic_on_tiny_heights() {
    // buffer_of(&mut app, w, h) para (w,h) en [(120,1),(120,2),(120,3),(80,4),(200,5)] con Crush y datos cargados: no entra en pánico.
}
```
Completa los comentarios de los dos últimos y `rail_lists_...` con código real (construye `JobSnapshot`/`SubagentDescriptor` leyendo sus definiciones en `codeloom-core`; `app.session_id = Some(SessionId(1))`). Comprobar además que los tests de Classic listados en Global Constraints siguen verdes.

- [ ] **Step 2:** `cargo test -p codeloom-tui` → fallan/no compila.
- [ ] **Step 3:** Implementar según "Comportamiento". Mantén `draw_workspace_header`, `draw_workspace_metrics`, `draw_workspace_sidebar` intactos para Classic; añade `draw_flat_header` y el pie plano como funciones nuevas.
- [ ] **Step 4:** `cargo test` (workspace), `cargo clippy -- -D warnings`, `rustfmt --edition 2021 --check` de los ficheros tocados (si rustfmt reformatea código ajeno, deshazlo solo en esos hunks).

---

## Validación manual (la hace el usuario)

`/theme` hasta "Crush · Charmtone": un único rail a la derecha, cabecera de una línea, chat sin marco y pie de atajos cortos; Ctrl+B oculta/muestra el rail; reducir la terminal por debajo de 80 columnas lo quita; volver a Batten restaura el layout de tres columnas.
