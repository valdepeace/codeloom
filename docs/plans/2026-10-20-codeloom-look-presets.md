# Looks de TUI (preset Crush) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Los presets de tema pasan a ser looks (paleta + variante por elemento) y el preset Crush ofrece composer sin caja con wrap/scroll correctos y tools inline en orden dentro del transcript.

**Architecture:** `Theme` gana `look: Look` (`ComposerStyle`, `ToolStyle`). El composer Crush usa una función pura de layout de filas visuales (`state/composer_layout.rs`) tanto para pintar como para colocar el cursor. Las tools son entradas del `Transcript` (nuevo `push_lines`/`replace`) con tres `LineKind` nuevos; `App` las mantiene sincronizadas con `ToolItem`.

**Tech Stack:** Rust 2021, ratatui, unicode-segmentation, unicode-width (ya en `batten`).

**Spec:** `docs/specs/2026-10-20-codeloom-look-presets-design.md`

## Global Constraints

- Tras cada tarea deben pasar `cargo build`, `cargo test`, `cargo fmt --check` y `cargo clippy -- -D warnings`. Los siete avisos previos de clippy en Batten pueden seguir ahí; **no se añaden nuevos**. Usar un `CARGO_TARGET_DIR` local del repo (el compartido llena `E:`). En Git Bash hace falta el override del enlazador MSVC (ver memoria `loom-build-env`).
- `codeloom-core` no puede depender de `ratatui` ni `crossterm`. Estos cambios solo tocan `batten` y `codeloom-tui`.
- Sin red en tests.
- Cambiar de look no toca el estado `batten::state::Composer` ni el bucle de entrada. Contrato de teclas intacto (Enter envía, Ctrl+J salto).
- El árbol de trabajo tiene cambios sin commitear del usuario en los ficheros que se tocan (p. ej. `Theme::preset` solo existe en el árbol, no en `HEAD`). **No** usar `git checkout`/`git restore`/`git stash`. **NO se hace ningún commit durante la ejecución**: se ignoran los steps "Commit" de todas las tareas y los cambios se dejan en el árbol sin `git add`. El usuario decide al final cómo commitear. Ejecutar `cargo fmt` solo sobre los ficheros de la tarea (`rustfmt <ruta>`), nunca `cargo fmt --all`.
- Comentarios de código y mensajes de commit en español de registro cuidado, sin andalucismos ni tacos. Commits terminan con `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- `Look::classic()` debe reproducir exactamente el comportamiento actual (los presets sin look propio no cambian).

## Review Focus

Cada línea tiene su test en la tarea indicada.

1. Línea más larga que el ancho con el cursor al final exacto de una fila llena (T2: `cursor_at_end_of_full_row_gets_its_own_row`).
2. Texto con grafemas anchos (CJK) y emojis que no caben al final de fila (T2: `wide_graphemes_wrap_without_splitting`).
3. Ancho 1 o 0 y grafema más ancho que la fila: no puede haber bucle infinito ni pánico (T2: `tiny_widths_do_not_panic_or_loop`).
4. Cambiar de preset a mitad de conversación no puede duplicar ni perder tools visibles (T6: `switching_look_hides_and_restores_tool_entries`).
5. Texto del asistente posterior a una tool debe quedar *después* de ella (T6: `text_after_a_tool_goes_below_it`).

---

### Task 1: `Look` dentro de `Theme`

**Files:**
- Modify: `crates/batten/src/widgets/theme.rs`
- Modify: `crates/batten/src/widgets/mod.rs:61` (re-export)

**Interfaces:**
- Produces (en `batten::widgets`): `enum ComposerStyle { Classic, Crush }`, `enum ToolStyle { Pinned, Inline }`, `struct Look { pub composer: ComposerStyle, pub tool: ToolStyle }` con `Look::classic()` y `Look::crush()` (`const fn`), todos `Debug, Clone, Copy, PartialEq, Eq`. `Theme.look: Look`.

- [ ] **Step 1: Escribir los tests** al final del `mod tests` de `theme.rs` (si no existe, créalo con `use super::*;`):

```rust
#[test]
fn only_the_crush_preset_has_its_own_look() {
    for preset in ThemePreset::ALL {
        let theme = Theme::preset(preset);
        let expected = if preset == ThemePreset::Crush {
            Look::crush()
        } else {
            Look::classic()
        };
        assert_eq!(theme.look, expected, "{}", preset.name());
    }
    assert_eq!(Theme::default_dark().look, Look::classic());
    assert_eq!(Theme::test().look, Look::classic());
    assert_eq!(Look::crush().composer, ComposerStyle::Crush);
    assert_eq!(Look::crush().tool, ToolStyle::Inline);
    assert_eq!(Look::classic().composer, ComposerStyle::Classic);
    assert_eq!(Look::classic().tool, ToolStyle::Pinned);
}
```

- [ ] **Step 2: Ejecutar** `cargo test -p batten only_the_crush_preset_has_its_own_look` → falla al compilar (`Look` no existe).

- [ ] **Step 3: Implementar** en `theme.rs`, antes de `pub struct Theme`:

```rust
/// Variante de presentación del composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerStyle {
    /// Caja redondeada con título y pie (comportamiento histórico).
    Classic,
    /// Sin marco, prompt `> ` y `::: ` por línea, con wrap y scroll vertical.
    Crush,
}

/// Dónde se presentan las llamadas a herramientas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStyle {
    /// Tarjeta fija junto al composer (solo la última).
    Pinned,
    /// Entradas dentro del transcript, en orden cronológico.
    Inline,
}

/// Presentación de cada elemento; un preset es paleta más `Look`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub composer: ComposerStyle,
    pub tool: ToolStyle,
}

impl Look {
    pub const fn classic() -> Self {
        Self {
            composer: ComposerStyle::Classic,
            tool: ToolStyle::Pinned,
        }
    }

    pub const fn crush() -> Self {
        Self {
            composer: ComposerStyle::Crush,
            tool: ToolStyle::Inline,
        }
    }
}
```

Añadir el campo `pub look: Look,` a `Theme` (tras `hyperlinks`) y `look: Look::classic(),` en `default_dark()`. En `Theme::preset`, justo antes del `theme` final (`theme.glyphs.tool_pending = '●'; theme`) añadir `if preset == Crush { theme.look = Look::crush(); }`. Si `from_toml` u otro sitio construye `Theme { .. }` literal y deja de compilar, añadir `look: Look::classic()` ahí. Reexportar en `widgets/mod.rs`: `pub use theme::{ColorDepth, ComposerStyle, Glyphs, Look, Styles, Theme, ThemeError, ThemePreset, ToolStyle};`.

- [ ] **Step 4: Ejecutar** `cargo test -p batten` → todo en verde. Después `cargo fmt --all`.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets/theme.rs crates/batten/src/widgets/mod.rs
git commit -m "feat(batten): look por preset (composer y herramientas)"
```

---

### Task 2: Layout puro de filas visuales del composer

**Files:**
- Create: `crates/batten/src/state/composer_layout.rs`
- Modify: `crates/batten/src/state/mod.rs` (declarar módulo y re-exportar)

**Interfaces:**
- Produces (en `batten::state`): `struct VisualRow { pub start: usize, pub end: usize, pub line: usize, pub first: bool }`, `struct ComposerLayout { pub rows: Vec<VisualRow>, pub cursor: (usize, usize) }`, `fn layout(text: &str, cursor_graphemes: usize, first_width: usize, rest_width: usize) -> ComposerLayout`. `start..end` son bytes de `text` sin el `\n`; `line` es el índice de línea lógica; `first` marca la primera fila de su línea lógica; `cursor` es `(fila visual, columna en celdas)`. `first_width` se aplica a la línea lógica 0, `rest_width` al resto (ambos con mínimo 1).
- El cursor se expresa en grafemas, igual que `Composer::cursor()` (el `\n` cuenta como un grafema).

- [ ] **Step 1: Escribir los tests** en `composer_layout.rs` (`#[cfg(test)] mod tests`), con estos casos y aserciones exactas (helper `fn texts(t: &str, l: &ComposerLayout) -> Vec<&str> { l.rows.iter().map(|r| &t[r.start..r.end]).collect() }`):

```rust
#[test]
fn empty_text_has_one_row_and_cursor_at_origin() {
    let l = layout("", 0, 10, 10);
    assert_eq!(l.rows.len(), 1);
    assert_eq!(l.cursor, (0, 0));
}

#[test]
fn logical_lines_become_rows() {
    let t = "ab\ncd";
    let l = layout(t, 4, 10, 10); // tras la 'c'
    assert_eq!(texts(t, &l), vec!["ab", "cd"]);
    assert_eq!(l.rows[1].line, 1);
    assert!(l.rows[1].first);
    assert_eq!(l.cursor, (1, 1));
}

#[test]
fn soft_wrap_splits_long_lines() {
    let t = "abcdefgh";
    let l = layout(t, 8, 3, 3);
    assert_eq!(texts(t, &l), vec!["abc", "def", "gh"]);
    assert!(l.rows[0].first && !l.rows[1].first && !l.rows[2].first);
    assert!(l.rows.iter().all(|r| r.line == 0));
    assert_eq!(l.cursor, (2, 2));
}

#[test]
fn cursor_on_a_wrap_boundary_belongs_to_the_next_row() {
    let t = "abcdef";
    let l = layout(t, 3, 3, 3); // entre 'c' y 'd'
    assert_eq!(l.cursor, (1, 0));
}

#[test]
fn cursor_at_end_of_full_row_gets_its_own_row() {
    let t = "abc";
    let l = layout(t, 3, 3, 3);
    assert_eq!(texts(t, &l), vec!["abc", ""]);
    assert_eq!(l.cursor, (1, 0));
}

#[test]
fn first_and_rest_widths_apply_per_logical_line() {
    let t = "abcd\nabcd";
    let l = layout(t, 0, 2, 4);
    assert_eq!(texts(t, &l), vec!["ab", "cd", "abcd"]);
}

#[test]
fn wide_graphemes_wrap_without_splitting() {
    let t = "日本語"; // 3 grafemas de 2 celdas
    let l = layout(t, 3, 5, 5);
    assert_eq!(texts(t, &l), vec!["日本", "語"]);
    assert_eq!(l.cursor, (1, 2));
}

#[test]
fn combining_marks_stay_with_their_base() {
    let t = "e\u{301}e\u{301}e\u{301}"; // 3 grafemas de 1 celda
    let l = layout(t, 2, 2, 2);
    assert_eq!(texts(t, &l), vec!["e\u{301}e\u{301}", "e\u{301}"]);
    assert_eq!(l.cursor, (1, 0));
}

#[test]
fn tiny_widths_do_not_panic_or_loop() {
    for w in [0usize, 1] {
        let t = "日本a\nb";
        let l = layout(t, 4, w, w);
        assert!(!l.rows.is_empty());
        assert!(l.cursor.0 < l.rows.len());
    }
}

#[test]
fn cursor_past_the_end_is_clamped_to_the_last_row() {
    let t = "ab";
    let l = layout(t, 99, 10, 10);
    assert_eq!(l.cursor, (0, 2));
}
```

- [ ] **Step 2: Ejecutar** `cargo test -p batten composer_layout` → falla (módulo no existe).

- [ ] **Step 3: Implementar** `layout`. Algoritmo (recorrer `text.split('\n')` con un `base` de bytes y un contador `seen` de grafemas previos; para cada línea lógica `idx` con ancho `w = (if idx == 0 { first_width } else { rest_width }).max(1)`):
  - Recorrer `line.grapheme_indices(true)`; mantener `row_start` (byte relativo), `row_width` y `first` (true al iniciar la línea).
  - Para cada grafema con anchura `gw = UnicodeWidthStr::width(g)`: si `row_width + gw > w && row_width > 0`, cerrar la fila actual (`VisualRow { start: base + row_start, end: base + off, line: idx, first }`), `row_start = off`, `row_width = 0`, `first = false`. **Después** de ese posible cierre, si `!cursor_set && seen + i == cursor_graphemes`, fijar `cursor = (rows.len(), row_width)`. Luego `row_width += gw`. (Un grafema más ancho que la fila ocupa una fila solo para él; así siempre hay progreso.)
  - Al acabar la línea: si `!cursor_set && seen + count == cursor_graphemes`: si `row_width >= w`, cerrar la fila actual y añadir una fila vacía `VisualRow { start: base+line.len(), end: base+line.len(), line: idx, first: false }` con `cursor = (rows.len() - 1, 0)`; si no, `cursor = (rows.len(), row_width)` y cerrar la fila normalmente. Si el cursor no es de esta línea, cerrar la fila normalmente.
  - Tras la línea: `base += line.len() + 1`, `seen += count + 1`.
  - Al final, si `!cursor_set`: `cursor = (rows.len() - 1, anchura en celdas de text[last.start..last.end])`.
  Cabecera del fichero: `//! Layout de filas visuales del composer (wrap por grafemas). Puro.` Importar `unicode_segmentation::UnicodeSegmentation` y `unicode_width::UnicodeWidthStr`. Derivar `Debug, Clone, PartialEq, Eq` en los dos structs. Registrar `pub mod composer_layout;` y `pub use composer_layout::{layout as composer_layout, ComposerLayout, VisualRow};` en `state/mod.rs`.

- [ ] **Step 4: Ejecutar** `cargo test -p batten composer_layout` → todos pasan. `cargo fmt --all`; `cargo clippy -p batten -- -D warnings` sin avisos nuevos.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/state/composer_layout.rs crates/batten/src/state/mod.rs
git commit -m "feat(batten): layout de filas visuales del composer"
```

---

### Task 3: Composer Crush con wrap, scroll y altura; despacho por look

**Files:**
- Modify: `crates/batten/src/widgets/composer.rs`

**Interfaces:**
- Consumes: `batten::state::{composer_layout, ComposerLayout}` (T2); `Theme.look.composer` (T1).
- Produces: `pub fn height_for_look(theme: &Theme, composer: &Composer, width: u16, max: u16) -> u16` y `pub fn render_for_look(frame: &mut Frame, area: Rect, composer: &Composer, focused: bool, model: &str, theme: &Theme)`. `render_crush` conserva su firma actual `(frame, area, composer, focused, placeholder, theme)` pero ahora usa el layout (corrige el cursor con wrap) y scroll vertical.

- [ ] **Step 1: Tests** (en el `mod tests` de `composer.rs`; usar `ratatui::backend::TestBackend`/`Terminal` como en los tests existentes del fichero; si no hay, crear helper `fn draw(c:&Composer, w:u16, h:u16, theme:&Theme) -> (String, Option<(u16,u16)>)` que renderice `render_crush` y devuelva el contenido y la posición del cursor con `terminal.get_cursor_position()`):

```rust
#[test]
fn classic_height_is_logical_lines_plus_frame_capped() {
    let theme = Theme::test();
    let mut c = Composer::new();
    c.set_text("a\nb\nc");
    assert_eq!(height_for_look(&theme, &c, 40, 20), 5);
    assert_eq!(height_for_look(&theme, &c, 40, 4), 4);
}

#[test]
fn crush_height_follows_wrapped_rows_between_3_and_15() {
    let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
    let mut c = Composer::new();
    assert_eq!(height_for_look(&theme, &c, 20, 30), 3);
    c.set_text(&"x".repeat(100)); // 100 celdas en filas de 18 -> 6 filas
    assert_eq!(height_for_look(&theme, &c, 20, 30), 6);
    c.set_text(&"x\n".repeat(40));
    assert_eq!(height_for_look(&theme, &c, 20, 30), 15);
    assert_eq!(height_for_look(&theme, &c, 20, 8), 8);
}

#[test]
fn crush_cursor_follows_the_wrapped_row() {
    let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
    let mut c = Composer::new();
    c.set_text("abcdefghij"); // ancho 8: "> " + 6 celdas por fila
    let (_, cursor) = draw(&c, 8, 4, &theme);
    assert_eq!(cursor, Some((2 + 4, 1))); // fila 1 ("ghij"), tras 4 celdas
}

#[test]
fn crush_keeps_the_cursor_row_visible_when_overflowing() {
    let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
    let mut c = Composer::new();
    c.set_text("1\n2\n3\n4\n5\n6");
    let (screen, cursor) = draw(&c, 20, 3, &theme);
    assert!(screen.contains('6'));
    assert!(!screen.contains("> 1"));
    assert_eq!(cursor.map(|p| p.1), Some(2));
}

#[test]
fn crush_uses_prompt_and_continuation_gutters() {
    let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
    let mut c = Composer::new();
    c.set_text("uno\ndos");
    let (screen, _) = draw(&c, 20, 3, &theme);
    assert!(screen.contains("> uno"));
    assert!(screen.contains("::: dos"));
}

#[test]
fn crush_empty_shows_the_placeholder() {
    let theme = Theme::preset(crate::widgets::ThemePreset::Crush);
    let (screen, cursor) = draw(&Composer::new(), 40, 3, &theme);
    assert!(screen.contains("> Describe what you want to build"));
    assert_eq!(cursor, Some((2, 0)));
}
```

- [ ] **Step 2: Ejecutar** `cargo test -p batten composer` → fallan (funciones ausentes / cursor erróneo).

- [ ] **Step 3: Implementar** en `composer.rs`:
  - `fn crush_gutter(row: &VisualRow) -> &'static str`: `line == 0` → `"> "` si `first` si no `"  "`; `line > 0` → `"::: "` si `first` si no `"    "`.
  - `fn crush_layout(composer: &Composer, width: u16) -> ComposerLayout`: llama a `crate::state::composer_layout(composer.text(), composer.cursor(), width.saturating_sub(2) as usize, width.saturating_sub(4) as usize)`.
  - Reescribir `render_crush` sin `Paragraph::wrap`: calcular `layout`, `scroll = layout.cursor.0.saturating_sub(area.height.saturating_sub(1) as usize)`, y pintar solo las filas `scroll..scroll+area.height` como `Line` de dos spans (gutter en `prompt`, texto `&text[row.start..row.end]` en `text`). Si `composer.is_empty()`: una línea `"> "` + `placeholder` en muted. Cursor: `x = area.x + gutter_width(row_del_cursor) + col`, `y = area.y + (cursor_row - scroll)`, con las mismas comprobaciones de límites que ya hay. Quitar los imports que dejen de usarse (`Wrap` sigue usándose en otras funciones del fichero: comprobarlo).
  - `height_for_look`: `Classic` → `(composer.lines() + 2).min(max as usize) as u16` (idéntico al `draw_transcript` actual); `Crush` → `(rows.len().clamp(3, 15)).min(max as usize) as u16` con `rows` de `crush_layout`.
  - `render_for_look`: `Classic` → `render_focused_with_model(frame, area, composer, focused, model, theme)`; `Crush` → `render_crush(frame, area, composer, focused, "Describe what you want to build…", theme)`.

- [ ] **Step 4: Ejecutar** `cargo test -p batten` y `cargo run -p batten --example showcase` no es necesario. Verde + `cargo fmt --all` + `cargo clippy -p batten -- -D warnings` sin avisos nuevos.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/widgets/composer.rs
git commit -m "feat(batten): composer Crush con wrap, scroll y altura por look"
```

---

### Task 4: `Transcript::push_lines` y `Transcript::replace`

**Files:**
- Modify: `crates/batten/src/state/transcript.rs`

**Interfaces:**
- Produces: `pub fn push_lines(&mut self, lines: Vec<Line>) -> EntryId` (entrada con líneas ya construidas, admite vacío) y `pub fn replace(&mut self, id: EntryId, lines: Vec<Line>)` (sustituye las líneas de una entrada existente, sube `rev`, reindexa; `id` inválido = no-op). Estas entradas no admiten `append`.

- [ ] **Step 1: Tests** en el `mod tests` existente:

```rust
#[test]
fn push_lines_creates_a_multi_line_entry() {
    let mut t = Transcript::new();
    let id = t.push_lines(vec![
        Line::new(LineKind::Banner, "a"),
        Line::new(LineKind::Banner, "b"),
    ]);
    t.push_entry(LineKind::User, Format::Plain, "z");
    assert_eq!(t.len(), 3);
    assert_eq!(t.entry_count(), 2);
    assert_eq!(t.entry_lines(id.index()).len(), 2);
    assert_eq!(t.line(2).unwrap().plain(), "z");
}

#[test]
fn replace_rewrites_one_entry_and_reindexes_the_rest() {
    let mut t = Transcript::new();
    t.push_entry(LineKind::User, Format::Plain, "antes");
    let id = t.push_lines(vec![Line::new(LineKind::Banner, "x")]);
    t.push_entry(LineKind::Assistant, Format::Plain, "despues");
    let (r0, r1) = (t.entry_rev(0), t.entry_rev(1));
    t.replace(
        id,
        vec![
            Line::new(LineKind::Banner, "x1"),
            Line::new(LineKind::Banner, "x2"),
            Line::new(LineKind::Banner, "x3"),
        ],
    );
    assert_eq!(t.entry_rev(0), r0);
    assert!(t.entry_rev(1) > r1);
    assert_eq!(t.len(), 5);
    assert_eq!(t.entry_start(2), 4);
    assert_eq!(t.line(4).unwrap().plain(), "despues");
    t.replace(id, Vec::new());
    assert_eq!(t.len(), 2);
    assert_eq!(t.line(1).unwrap().plain(), "despues");
}
```

- [ ] **Step 2: Ejecutar** `cargo test -p batten transcript` → no compila.

- [ ] **Step 3: Implementar.** Añadir a `EntryId` un `pub fn index(self) -> usize { self.0 }`. `push_lines`: construir `Entry { kind: primera línea o `LineKind::System`, format: Plain, source: texto de las líneas unido con '\n', lines, rev: 0 }` y `push_built`. `replace`: tomar `let rev = self.bump();`, `get_mut(id.0)` (si no existe, return), asignar `kind`/`source`/`lines` igual que arriba, `entry.rev = rev`, `self.reindex_from(id.0)`.

- [ ] **Step 4: Ejecutar** `cargo test -p batten` verde; `cargo fmt --all`.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/state/transcript.rs
git commit -m "feat(batten): entradas del transcript con líneas propias y reemplazables"
```

---

### Task 5: Líneas de tool para el transcript

**Files:**
- Create: `crates/batten/src/state/tool_lines.rs`
- Modify: `crates/batten/src/state/transcript.rs` (añadir variantes de `LineKind`)
- Modify: `crates/batten/src/state/mod.rs`
- Modify: `crates/batten/src/widgets/transcript.rs` (`prefix`, `kind_token`)

**Interfaces:**
- Produces: `LineKind::ToolHead(ToolStatus)`, `LineKind::ToolBody`, `LineKind::ToolMore` (importar `super::tool::ToolStatus` en `transcript.rs`; `LineKind` sigue siendo `Copy + Eq`). `pub fn tool_lines(item: &ToolItem) -> Vec<Line>` en `batten::state`.
- Formato: cabecera `ToolHead(item.status())` con texto `"{name}  {resumen}"` (resumen = primera línea de `item.summary`, recortada a 80 grafemas con `…`; sin resumen, solo el nombre) y spans: nombre → `StyleToken::ToolName`, resumen → `StyleToken::Muted` (offsets en bytes). Cuerpo (solo si `item.output` es `Some`): una `ToolBody` por línea de salida; si no está `expanded` y hay más de 2 líneas, solo las 2 primeras más `ToolMore` con texto `"… ({n} lines hidden) [Ctrl+O to expand]"` (`n` = restantes). Salida vacía → sin cuerpo.
- Prefijos de widget (no forman parte del texto, no se copian): `ToolHead(_)` → `"● "`, `ToolBody` → `"│ "`, `ToolMore` → `"  "`. Tokens: `ToolHead(Pending)`→`ToolPending`, `Running`→`ToolRunning`, `Success`→`ToolSuccess`, `Error`→`ToolError`, `Canceled`→`ToolCanceled`, `AwaitingPermission`→`Warning`; `ToolBody`→`Muted`; `ToolMore`→`Dim`.

- [ ] **Step 1: Tests** en `tool_lines.rs`:

```rust
#[test]
fn header_only_while_there_is_no_output() {
    let mut t = ToolItem::new("c1", "View");
    t.set_summary("src/lib.rs");
    t.start();
    let lines = tool_lines(&t);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Running));
    assert_eq!(lines[0].plain(), "View  src/lib.rs");
    assert_eq!(lines[0].spans[0].token, StyleToken::ToolName);
    assert_eq!((lines[0].spans[0].start, lines[0].spans[0].len), (0, 4));
    assert_eq!(lines[0].spans[1].token, StyleToken::Muted);
}

#[test]
fn long_output_is_folded_to_two_lines_plus_a_hint() {
    let mut t = ToolItem::new("c1", "Bash");
    t.set_result(false);
    t.set_output("1\n2\n3\n4\n5");
    let lines = tool_lines(&t);
    let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        vec![
            LineKind::ToolHead(ToolStatus::Success),
            LineKind::ToolBody,
            LineKind::ToolBody,
            LineKind::ToolMore
        ]
    );
    assert_eq!(lines[3].plain(), "… (3 lines hidden) [Ctrl+O to expand]");
}

#[test]
fn expanded_shows_everything() {
    let mut t = ToolItem::new("c1", "Bash");
    t.set_result(true);
    t.set_output("1\n2\n3\n4");
    t.toggle_expanded();
    let lines = tool_lines(&t);
    assert_eq!(lines.len(), 5);
    assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Error));
    assert!(lines.iter().all(|l| l.kind != LineKind::ToolMore));
}

#[test]
fn short_output_has_no_hint_and_empty_output_has_no_body() {
    let mut t = ToolItem::new("c1", "Bash");
    t.set_output("a\nb");
    assert_eq!(tool_lines(&t).len(), 3);
    t.set_output("");
    assert_eq!(tool_lines(&t).len(), 1);
}

#[test]
fn summary_uses_its_first_line_and_is_truncated() {
    let mut t = ToolItem::new("c1", "Edit");
    t.set_summary(format!("{}\nsegunda", "x".repeat(200)));
    let head = &tool_lines(&t)[0];
    assert!(!head.plain().contains("segunda"));
    assert!(head.plain().ends_with('…'));
    assert!(head.plain().chars().count() <= "Edit  ".len() + 80);
}
```
Y en `widgets/transcript.rs` (`mod tests`): un test que pinte un `Transcript` con una entrada `push_lines(tool_lines(..))` y compruebe que la pantalla contiene `● View` y `│ ` pero que `selected text`/`plain()` de las líneas no contiene `●`.

- [ ] **Step 2: Ejecutar** `cargo test -p batten tool_lines` → no compila.

- [ ] **Step 3: Implementar** las variantes, `prefix`, `kind_token` (los únicos `match` exhaustivos sobre `LineKind`; el de `role` tiene comodín) y `tool_lines` según el formato anterior. Recorte de resumen por grafemas (`unicode_segmentation`). Registrar `pub mod tool_lines;` y `pub use tool_lines::tool_lines;` en `state/mod.rs`.

- [ ] **Step 4: Ejecutar** `cargo test -p batten` verde, `cargo fmt --all`, clippy sin avisos nuevos.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/src/state crates/batten/src/widgets/transcript.rs
git commit -m "feat(batten): líneas de herramienta para el transcript"
```

---

### Task 6: Tools inline en `App` y render por look

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`
- Modify: `crates/codeloom-tui/src/run.rs:401-406` (Ctrl+O)
- Modify: `crates/codeloom-tui/src/render.rs` (`draw_transcript`)

**Interfaces:**
- Consumes: T1 (`Theme.look`), T3 (`height_for_look`, `render_for_look`), T4 (`push_lines`, `replace`), T5 (`tool_lines`).
- Produces en `App`: campo `tool_entries: std::collections::HashMap<String, (batten::state::EntryId, Vec<batten::state::Line>)>`; `fn tool_inline(&self) -> bool` (`theme.look.tool == ToolStyle::Inline`); `pub fn sync_tool(&mut self, call_id: &str)`; `pub fn sync_all_tools(&mut self)`.

Comportamiento de `sync_tool(call_id)`: buscar el `ToolItem` en `self.tools` (si no está, return); `lines = tool_lines(item)`; `shown = if tool_inline { lines.clone() } else { vec![] }`; si ya hay entrada y `cache == lines` y no cambia la visibilidad, no hacer nada (evita re-medir); si hay entrada, guardar `lines` en la caché y `transcript.replace(id, shown)`; si no hay entrada, `id = transcript.push_lines(shown)`, insertar `(id, lines)` y poner `self.assistant = None` (el texto posterior abre otra entrada debajo). `sync_all_tools()`: para cada entrada del mapa, `replace(id, if inline { cache.clone() } else { vec![] })`; después `sync_tool` para cada `ToolItem` vivo (actualiza la caché).

- [ ] **Step 1: Tests** en `app.rs` (`mod tests`, con `App::new()` y `app.theme = Theme::preset(ThemePreset::Crush)` para Inline):

```rust
#[test]
fn inline_tools_appear_in_order_between_messages() {
    let mut app = crush_app();
    app.push_user("hola");
    app.push_assistant("voy a mirar");
    app.apply(LiveEvent::ToolStarted { call_id: "c1".into(), name: "View".into() });
    app.push_assistant("hecho");
    let texts: Vec<String> = app.transcript.iter().map(|l| l.plain().to_string()).collect();
    let tool = texts.iter().position(|t| t.starts_with("View")).unwrap();
    let before = texts.iter().position(|t| t == "voy a mirar").unwrap();
    let after = texts.iter().position(|t| t == "hecho").unwrap();
    assert!(before < tool && tool < after);
}

#[test]
fn text_after_a_tool_goes_below_it() {
    let mut app = crush_app();
    app.push_assistant("uno");
    app.apply(LiveEvent::ToolStarted { call_id: "c1".into(), name: "View".into() });
    app.push_assistant("dos");
    assert_eq!(app.transcript.entry_count(), 3);
}

#[test]
fn tool_entry_updates_in_place_with_result_and_output() {
    let mut app = crush_app();
    app.apply(LiveEvent::ToolStarted { call_id: "c1".into(), name: "Bash".into() });
    let count = app.transcript.entry_count();
    app.apply_message_added(tool_result_message("c1", "linea 1\nlinea 2\nlinea 3", false));
    assert_eq!(app.transcript.entry_count(), count);
    let texts: Vec<String> = app.transcript.iter().map(|l| l.plain().to_string()).collect();
    assert!(texts.iter().any(|t| t == "linea 1"));
    assert!(texts.iter().any(|t| t.contains("lines hidden")));
}

#[test]
fn switching_look_hides_and_restores_tool_entries() {
    let mut app = crush_app();
    app.apply(LiveEvent::ToolStarted { call_id: "c1".into(), name: "View".into() });
    assert!(app.transcript.iter().any(|l| l.plain().starts_with("View")));
    app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Batten);
    app.sync_all_tools();
    assert!(!app.transcript.iter().any(|l| l.plain().starts_with("View")));
    app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
    app.sync_all_tools();
    assert!(app.transcript.iter().any(|l| l.plain().starts_with("View")));
}

#[test]
fn classic_look_keeps_the_transcript_free_of_tool_lines() {
    let mut app = App::new(); // tema Batten = Classic
    app.apply(LiveEvent::ToolStarted { call_id: "c1".into(), name: "View".into() });
    assert!(!app.transcript.iter().any(|l| l.plain().starts_with("View")));
    assert_eq!(app.tools.len(), 1);
}

#[test]
fn restoring_a_session_places_tools_in_conversation_order() {
    // Reutilizar el mensaje de prueba de `restores_transcript_history_and_tool_results`
    // con tema Crush: la línea de la tool queda entre el texto del asistente y el siguiente mensaje.
}
```
(`crush_app()` = `App::new()` con `theme = Theme::preset(Crush)` y `theme_preset = Crush`; `tool_result_message` = helper que construya un `Message` con rol `Tool` y un `ContentPart::ToolResult` como el test existente de la línea ~1053. Completar el último test copiando la construcción de ese test.)
En `render.rs` (`mod tests`): con tema Crush y una tool en curso, `buffer_of(app, 80, 24)` contiene `"● View"` **una sola vez** y no contiene `"Waiting"`/tarjeta fija; con Classic sigue apareciendo la tarjeta fija (reutilizar `live_tool_card_separates_status_arguments_and_output`, que debe seguir verde sin cambios). Con Crush, un composer de texto de 100 `x` en 40 columnas mantiene el cursor dentro de la pantalla (`terminal.get_cursor_position()` dentro del panel).

- [ ] **Step 2: Ejecutar** `cargo test -p codeloom-tui` → fallan/no compila.

- [ ] **Step 3: Implementar.**
  - `app.rs`: añadir el campo e inicializarlo (`HashMap::new()`) en `new()`; vaciarlo en `restore_messages` junto a `self.tools.clear()`.
  - Llamar a `sync_tool(&call_id)` tras cada mutación de un `ToolItem`: en `ToolStarted` (tras `tool.start()`), en `ToolFinished` (tras `set_result`), en `apply_message_added` (tras `set_summary` y tras `set_result/set_output`), y en `restore_messages` (tras `set_summary`, tras `set_result/set_output`). En `restore_messages`, tras el bucle de `cancel()` de las tools que giran, llamar a `sync_all_tools()`. `sync_tool` toma `&str`: clonar el `call_id` antes si hay conflicto de préstamo con `tool_mut`.
  - `cycle_theme`: tras reasignar `self.theme`, llamar a `self.sync_all_tools()`.
  - `run.rs` Ctrl+O: tras `tool.toggle_expanded();` obtener el `call_id` (clonado) de `app.tools.back()` y llamar `app.sync_tool(&call_id)`.
  - `render.rs` `draw_transcript`: sustituir el cálculo de `tool_rows` por `let pinned = app.theme.look.tool == batten::widgets::ToolStyle::Pinned;` y calcular `tool_rows` solo si `pinned` (si no, `0`); sustituir `composer_height` por `batten::widgets::composer::height_for_look(&app.theme, &app.composer, inner.width, max_composer_height)`; dibujar la tarjeta solo si `pinned`; sustituir la llamada a `render_focused_with_model` por `batten::widgets::composer::render_for_look(...)` con los mismos argumentos. En look Crush omitir la fila superior "CodeLoom · project conversation · tema": poner la fila 0 a `Constraint::Length(0)` cuando `app.theme.look.composer == ComposerStyle::Crush`, y mostrar el nombre del tema activo tras `/theme` en `app.slash_feedback` (en el handler de `/theme`, `run.rs`, fijar `app.slash_feedback = Some(format!("Theme: {}", app.theme_preset.name()))`; comprobar con `grep -n "cycle_theme" crates/codeloom-tui/src/run.rs` el punto exacto y que el test existente de `/theme` sigue verde, ajustándolo si asertaba otra cosa).

- [ ] **Step 4: Ejecutar** `cargo build`, `cargo test` (todo el workspace), `cargo fmt --all --check`, `cargo clippy -- -D warnings` → verde, sin avisos nuevos.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui/src/app.rs crates/codeloom-tui/src/run.rs crates/codeloom-tui/src/render.rs
git commit -m "feat(tui): look Crush con herramientas inline y composer sin caja"
```

---

### Task 7: Cierre documental

**Files:**
- Modify: `docs/specs/2026-10-20-codeloom-look-presets-design.md`
- Modify: `docs/plans/2026-10-19-codeloom-tui-sections.md` (estado de la sección Conversación)

- [ ] **Step 1:** En la spec, sustituir el bloque "Cabeceras de rol" por una nota: las cabeceras "You"/"Assistant" ya existen en `widgets/transcript.rs` (`render_line`, decoración no seleccionable) y no requieren trabajo; y en "Etiqueta de modelo" anotar que el modelo ya aparece en la cabecera global (`draw_workspace_header`), por lo que el composer Crush no lo repite. Añadir una sección "Estado" con la lista de tareas hechas y la nota "validación visual en terminal real: pendiente".
- [ ] **Step 2:** En la tabla de `2026-10-19-codeloom-tui-sections.md`, fila Conversación: añadir "look Crush (composer sin caja, tools inline)"; mantener "validación visual pendiente".
- [ ] **Step 3: Commit**

```bash
git add docs/specs/2026-10-20-codeloom-look-presets-design.md docs/plans/2026-10-19-codeloom-tui-sections.md
git commit -m "docs: estado del look Crush"
```

---

## Validación manual (la hace el usuario, no un agente)

`cargo run -p codeloom-cli --bin loom`, `/theme` hasta "Crush · Charmtone": composer vacío, multilínea, texto largo envuelto, pegado grande, terminal estrecha; una conversación con varias tools (plegadas y con Ctrl+O), y rotar todos los presets comprobando que Classic no cambia.
