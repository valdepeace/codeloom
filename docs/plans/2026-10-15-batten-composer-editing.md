# Composer editable — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convertir el input del prompt en un editor con cursor, edición en medio, multilínea (`Ctrl+J`) e historial (`↑/↓`), en batten y en la TUI.

**Architecture:** `state::Composer` gana cursor (índice de carácter) y operaciones de edición; nuevo `state::History`; `widgets::composer` pinta multilínea y coloca el cursor real. codeloom-tui sustituye su `String` por `Composer` + `History` y mapea las teclas.

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30, `unicode-width`.

**Spec:** `docs/specs/2026-10-15-batten-composer-editing-design.md`

## Global Constraints

- `state` sigue sin `ratatui`/`crossterm`; `batten` sin crossterm.
- Enter envía siempre; el salto de línea es `Ctrl+J`.
- El cursor es el real del terminal (`frame.set_cursor_position`), no un bloque.
- Sin wrap horizontal: las líneas lógicas se recortan al ancho.
- Comentarios en español.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los tests pasan en cada commit.

## Review Focus

- **Cursor en índice de carácter, no de byte**: editar con acentos/CJK no debe corromper el texto. Test en Task 1.
- **`cursor_row_col` con líneas y ancho de display**: la columna es de display, no de chars. Test en Task 1.
- **`↑/↓` con varias líneas vs historial**: con >1 línea mueven cursor; con 1 línea navegan historial; tras recordar, siguen navegando aunque el texto tenga líneas. Test en Task 4.
- **Cursor fuera del área**: con área pequeña, no se coloca (sin pánico). Test en Task 3.
- **`submit` vacío/solo-espacios** limpia buffer y cursor. Test en Task 1.

---

## File Structure

```
crates/batten/src/state/composer.rs   # ampliar
crates/batten/src/state/history.rs    # NUEVO
crates/batten/src/state/mod.rs        # + pub mod history; re-export
crates/batten/src/widgets/composer.rs # multilinea + cursor
crates/codeloom-tui/src/app.rs        # composer + history + up/down
crates/codeloom-tui/src/run.rs        # teclas
crates/codeloom-tui/src/render.rs     # widget de batten
crates/batten/examples/showcase/demos/composer.rs  # edicion + historial
```

---

### Task 1: `state::Composer` — cursor y edición

**Files:**
- Modify: `crates/batten/src/state/composer.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `Composer` con `insert_char`, `newline`, `insert_str`, `backspace`, `delete`, `move_left/right/home/end/up/down`, `cursor`, `lines`, `cursor_row_col`, `set_text`, `text`, `is_empty`, `push_char`, `submit`.

- [ ] **Step 1: Escribir los tests**

En `crates/batten/src/state/composer.rs`, en `mod tests`, añade (dejando los actuales):

```rust
    #[test]
    fn inserts_in_the_middle() {
        let mut c = Composer::new();
        c.insert_str("hl");
        c.move_left();
        c.insert_char('e');
        assert_eq!(c.text(), "hel");
        assert_eq!(c.cursor(), 2);
    }

    #[test]
    fn backspace_and_delete_around_the_cursor() {
        let mut c = Composer::new();
        c.insert_str("abc");
        c.move_left();
        c.move_left(); // cursor en 1
        c.backspace();
        assert_eq!(c.text(), "bc");
        c.delete();
        assert_eq!(c.text(), "c");
    }

    #[test]
    fn home_and_end_go_to_line_bounds() {
        let mut c = Composer::new();
        c.insert_str("uno\ndos");
        c.move_home();
        assert_eq!(c.cursor(), 4);
        c.move_end();
        assert_eq!(c.cursor(), 7);
    }

    #[test]
    fn up_and_down_keep_the_column() {
        let mut c = Composer::new();
        c.insert_str("aaaa\nb");
        c.move_end(); // final de "b" (indice 6)
        c.move_up();
        assert_eq!(c.cursor_row_col(), (0, 1)); // col 1 (la linea "b" mide 1)
        c.move_down();
        assert_eq!(c.cursor_row_col(), (1, 1));
    }

    #[test]
    fn cursor_row_col_counts_display_width() {
        let mut c = Composer::new();
        c.insert_str("星x");
        // cursor al final (indice 2): columna = 2 (星) + 1 (x)
        assert_eq!(c.cursor_row_col(), (0, 3));
    }

    #[test]
    fn handles_accented_input_without_corrupting() {
        let mut c = Composer::new();
        c.insert_str("áé");
        c.move_left();
        c.insert_char('x');
        assert_eq!(c.text(), "áxé");
    }

    #[test]
    fn lines_counts_logical_lines() {
        let mut c = Composer::new();
        assert_eq!(c.lines(), 1);
        c.insert_str("a\nb\nc");
        assert_eq!(c.lines(), 3);
    }

    #[test]
    fn set_text_puts_the_cursor_at_the_end() {
        let mut c = Composer::new();
        c.insert_str("old");
        c.set_text("new\nline");
        assert_eq!(c.text(), "new\nline");
        assert_eq!(c.cursor(), 8);
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten state::composer`
Expected: FAIL (métodos no existen).

- [ ] **Step 3: Implementar**

Añade `use unicode_width::UnicodeWidthChar;` al principio del archivo y
**reemplaza** el `struct Composer` y su `impl` por:

```rust
#[derive(Debug, Default)]
pub struct Composer {
    buf: String,
    cursor: usize,
    preferred_col: Option<usize>,
}

impl Composer {
    pub fn new() -> Self {
        Self::default()
    }

    fn char_count(&self) -> usize {
        self.buf.chars().count()
    }

    /// Offset de byte del carácter en `idx` (o el final del buffer).
    fn byte_at(&self, idx: usize) -> usize {
        self.buf
            .char_indices()
            .nth(idx)
            .map(|(b, _)| b)
            .unwrap_or(self.buf.len())
    }

    /// Rangos (inicio, fin) en índices de carácter, por línea lógica.
    fn line_spans(&self) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut start = 0;
        for (i, c) in self.buf.chars().enumerate() {
            if c == '\n' {
                spans.push((start, i));
                start = i + 1;
            }
        }
        spans.push((start, self.char_count()));
        spans
    }

    fn row_of_cursor(&self) -> usize {
        self.line_spans()
            .iter()
            .position(|(s, e)| self.cursor >= *s && self.cursor <= *e)
            .unwrap_or(0)
    }

    pub fn insert_char(&mut self, c: char) {
        let at = self.byte_at(self.cursor);
        self.buf.insert(at, c);
        self.cursor += 1;
        self.preferred_col = None;
    }

    pub fn newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn insert_str(&mut self, s: &str) {
        let at = self.byte_at(self.cursor);
        self.buf.insert_str(at, s);
        self.cursor += s.chars().count();
        self.preferred_col = None;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.buf.replace_range(start..end, "");
        self.cursor -= 1;
        self.preferred_col = None;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.char_count() {
            return;
        }
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor + 1);
        self.buf.replace_range(start..end, "");
        self.preferred_col = None;
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.preferred_col = None;
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.char_count() {
            self.cursor += 1;
        }
        self.preferred_col = None;
    }

    pub fn move_home(&mut self) {
        let spans = self.line_spans();
        self.cursor = spans[self.row_of_cursor()].0;
        self.preferred_col = None;
    }

    pub fn move_end(&mut self) {
        let spans = self.line_spans();
        self.cursor = spans[self.row_of_cursor()].1;
        self.preferred_col = None;
    }

    pub fn move_up(&mut self) {
        let spans = self.line_spans();
        let row = self.row_of_cursor();
        if row == 0 {
            return;
        }
        let col = self.preferred_col.unwrap_or(self.cursor - spans[row].0);
        let (s, e) = spans[row - 1];
        self.cursor = s + col.min(e - s);
        self.preferred_col = Some(col);
    }

    pub fn move_down(&mut self) {
        let spans = self.line_spans();
        let row = self.row_of_cursor();
        if row + 1 >= spans.len() {
            return;
        }
        let col = self.preferred_col.unwrap_or(self.cursor - spans[row].0);
        let (s, e) = spans[row + 1];
        self.cursor = s + col.min(e - s);
        self.preferred_col = Some(col);
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn lines(&self) -> usize {
        self.line_spans().len()
    }

    /// (fila, columna en anchura de display) del cursor.
    pub fn cursor_row_col(&self) -> (usize, usize) {
        let spans = self.line_spans();
        let row = self.row_of_cursor();
        let (s, _) = spans[row];
        let col = self
            .buf
            .chars()
            .skip(s)
            .take(self.cursor - s)
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        (row, col)
    }

    pub fn set_text(&mut self, s: &str) {
        self.buf = s.to_string();
        self.cursor = self.char_count();
        self.preferred_col = None;
    }

    pub fn push_char(&mut self, c: char) {
        self.insert_char(c);
    }

    pub fn text(&self) -> &str {
        &self.buf
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn submit(&mut self) -> Option<String> {
        self.cursor = 0;
        self.preferred_col = None;
        if self.buf.trim().is_empty() {
            self.buf.clear();
            return None;
        }
        Some(std::mem::take(&mut self.buf))
    }
}
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten state::composer`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): cursor and editing in the composer state"
```

---

### Task 2: `state::History`

**Files:**
- Create: `crates/batten/src/state/history.rs`
- Modify: `crates/batten/src/state/mod.rs`

**Interfaces:**
- Consumes: nada.
- Produces: `History` con `new`, `with_limit`, `push`, `prev`, `next`, `reset`, `is_browsing`, `len`, `is_empty`.

- [ ] **Step 1: Escribir los tests**

Crea `crates/batten/src/state/history.rs`:

```rust
//! Historial de prompts enviados. Puro.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history_returns_none() {
        let mut h = History::new();
        assert!(!h.is_browsing());
        assert_eq!(h.prev("draft"), None);
        assert_eq!(h.next(), None);
    }

    #[test]
    fn prev_and_next_round_trip_with_draft() {
        let mut h = History::new();
        h.push("uno");
        h.push("dos");
        assert_eq!(h.prev("borrador"), Some("dos"));
        assert_eq!(h.prev("borrador"), Some("uno"));
        assert_eq!(h.prev("borrador"), Some("uno")); // tope arriba
        assert_eq!(h.next(), Some("dos"));
        assert_eq!(h.next(), Some("borrador")); // vuelve al borrador
        assert!(!h.is_browsing());
        assert_eq!(h.next(), None);
    }

    #[test]
    fn push_resets_browsing() {
        let mut h = History::new();
        h.push("uno");
        h.prev("x");
        assert!(h.is_browsing());
        h.push("dos");
        assert!(!h.is_browsing());
    }

    #[test]
    fn limit_drops_the_oldest() {
        let mut h = History::with_limit(2);
        h.push("uno");
        h.push("dos");
        h.push("tres");
        assert_eq!(h.len(), 2);
        assert_eq!(h.prev("x"), Some("tres"));
        assert_eq!(h.prev("x"), Some("dos"));
    }
}
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten state::history`
Expected: FAIL (no compila).

- [ ] **Step 3: Implementar**

Añade encima de `#[cfg(test)]`:

```rust
/// Historial de prompts enviados, con navegación tipo shell.
#[derive(Debug)]
pub struct History {
    entries: Vec<String>,
    index: Option<usize>,
    draft: String,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            index: None,
            draft: String::new(),
            limit: 100,
        }
    }
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Añade una entrada y resetea la navegación.
    pub fn push(&mut self, sent: impl Into<String>) {
        self.entries.push(sent.into());
        while self.entries.len() > self.limit {
            self.entries.remove(0);
        }
        self.reset();
    }

    pub fn is_browsing(&self) -> bool {
        self.index.is_some()
    }

    /// Hacia atrás (↑). Guarda `current` como borrador la primera vez.
    pub fn prev(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        match self.index {
            None => {
                self.draft = current.to_string();
                let i = self.entries.len() - 1;
                self.index = Some(i);
                Some(&self.entries[i])
            }
            Some(0) => Some(&self.entries[0]),
            Some(i) => {
                self.index = Some(i - 1);
                Some(&self.entries[i - 1])
            }
        }
    }

    /// Hacia delante (↓). Al pasar del final devuelve el borrador y termina.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&str> {
        match self.index {
            None => None,
            Some(i) if i + 1 < self.entries.len() => {
                self.index = Some(i + 1);
                Some(&self.entries[i + 1])
            }
            Some(_) => {
                self.index = None;
                Some(&self.draft)
            }
        }
    }

    pub fn reset(&mut self) {
        self.index = None;
        self.draft.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
```

En `state/mod.rs`, añade `pub mod history;` y `pub use history::History;`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten state::history`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): prompt history state"
```

---

### Task 3: `widgets::composer` — multilínea + cursor

**Files:**
- Modify: `crates/batten/src/widgets/composer.rs`

**Interfaces:**
- Consumes: `state::Composer` (Task 1), `Theme`.
- Produces: `render(frame, area, composer, theme)` con cursor real.

- [ ] **Step 1: Escribir los tests**

En `crates/batten/src/widgets/composer.rs`, `mod tests`, añade:

```rust
    #[test]
    fn places_the_cursor_at_the_end() {
        let mut c = Composer::new();
        c.insert_str("hi");
        let backend = TestBackend::new(20, 3);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!((pos.x, pos.y), (3, 1)); // borde(1) + "hi"(2), fila 1
    }

    #[test]
    fn places_the_cursor_on_the_second_line() {
        let mut c = Composer::new();
        c.insert_str("a\nb");
        let backend = TestBackend::new(20, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!((pos.x, pos.y), (2, 2));
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let c = Composer::new();
        let _ = buffer(&c, 0, 0);
        let _ = buffer(&c, 1, 1);
    }
```

(Los tests `shows_title_and_text`, `empty_composer_does_not_panic` y `buffer` ya existen.)

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten widgets::composer`
Expected: FAIL (el cursor no se coloca).

- [ ] **Step 3: Implementar**

Reemplaza `render` por:

```rust
pub fn render(frame: &mut Frame, area: Rect, composer: &Composer, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("prompt")
        .border_style(theme.style(StyleToken::Muted));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = Paragraph::new(composer.text().to_string()).style(theme.style(StyleToken::Default));
    frame.render_widget(text, inner);

    let (row, col) = composer.cursor_row_col();
    if (row as u16) < inner.height && (col as u16) < inner.width {
        frame.set_cursor_position((inner.x + col as u16, inner.y + row as u16));
    }
}
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten widgets::composer`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): composer widget draws the cursor"
```

---

### Task 4: codeloom-tui — migrar a `Composer` + `History`

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`
- Modify: `crates/codeloom-tui/src/run.rs`
- Modify: `crates/codeloom-tui/src/render.rs`

**Interfaces:**
- Consumes: `batten::state::{Composer, History}` (Tasks 1/2), `batten::widgets::composer::render` (Task 3).
- Produces: `App.composer`, `App.history`, `App::{newline, delete, move_left, move_right, move_home, move_end, up, down}`.

- [ ] **Step 1: Escribir los tests**

En `crates/codeloom-tui/src/run.rs`, `mod tests`, añade:

```rust
    #[test]
    fn ctrl_j_adds_a_line_and_enter_sends() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "a\nb");
        let sent = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(sent, Some("a\nb".to_string()));
    }

    #[test]
    fn up_recalls_history_when_single_line() {
        let mut app = App::new();
        for c in "hola".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "hola");
    }

    #[test]
    fn up_moves_the_cursor_when_multiline() {
        let mut app = App::new();
        app.composer.set_text("uno\ndos");
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.cursor_row_col().0, 0);
    }
```

En `crates/codeloom-tui/src/app.rs`, `mod tests`, sustituye el test
`submit_clears_input` por:

```rust
    #[test]
    fn submit_clears_composer() {
        let mut app = App::new();
        app.push_char('h');
        app.push_char('i');
        assert_eq!(app.submit(), Some("hi".to_string()));
        assert!(app.composer.is_empty());
        assert_eq!(app.submit(), None);
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p codeloom-tui`
Expected: FAIL (`composer`/teclas no existen).

- [ ] **Step 3: Implementar**

En `app.rs`:

1. Sustituye el campo `pub input: String,` por:

```rust
    pub composer: batten::state::Composer,
    pub history: batten::state::History,
```

2. En `new()`, sustituye `input: String::new(),` por:

```rust
            composer: batten::state::Composer::new(),
            history: batten::state::History::new(),
```

3. Sustituye los métodos `push_char`/`backspace`/`submit` por:

```rust
    pub fn push_char(&mut self, c: char) {
        self.composer.insert_char(c);
        self.history.reset();
    }
    pub fn newline(&mut self) {
        self.composer.newline();
        self.history.reset();
    }
    pub fn backspace(&mut self) {
        self.composer.backspace();
        self.history.reset();
    }
    pub fn delete(&mut self) {
        self.composer.delete();
        self.history.reset();
    }
    pub fn move_left(&mut self) {
        self.composer.move_left();
    }
    pub fn move_right(&mut self) {
        self.composer.move_right();
    }
    pub fn move_home(&mut self) {
        self.composer.move_home();
    }
    pub fn move_end(&mut self) {
        self.composer.move_end();
    }

    /// ↑: cursor si hay varias líneas; si no, historial.
    pub fn up(&mut self) {
        if !self.history.is_browsing() && self.composer.lines() > 1 {
            self.composer.move_up();
            return;
        }
        let current = self.composer.text().to_string();
        if let Some(prev) = self.history.prev(&current) {
            let prev = prev.to_string();
            self.composer.set_text(&prev);
        }
    }

    /// ↓: historial si se está navegando; si no, cursor.
    pub fn down(&mut self) {
        if !self.history.is_browsing() {
            self.composer.move_down();
            return;
        }
        if let Some(next) = self.history.next() {
            let next = next.to_string();
            self.composer.set_text(&next);
        }
    }

    pub fn submit(&mut self) -> Option<String> {
        let text = self.composer.submit()?;
        self.history.push(text.clone());
        Some(text)
    }
```

En `run.rs`, dentro de `handle_key`, añade los brazos nuevos (antes de `KeyCode::Char(c)`):

```rust
        KeyCode::Char('j') if mods.contains(KeyModifiers::CONTROL) => {
            app.newline();
            None
        }
        KeyCode::Delete => {
            app.delete();
            None
        }
        KeyCode::Left => {
            app.move_left();
            None
        }
        KeyCode::Right => {
            app.move_right();
            None
        }
        KeyCode::Home => {
            app.move_home();
            None
        }
        KeyCode::End => {
            app.move_end();
            None
        }
        KeyCode::Up => {
            app.up();
            None
        }
        KeyCode::Down => {
            app.down();
            None
        }
```

En `render.rs`, sustituye `draw_composer` por:

```rust
fn draw_composer(frame: &mut Frame, area: Rect, app: &App) {
    batten::widgets::composer::render(frame, area, &app.composer, &app.theme);
}
```

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p codeloom-tui`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui
git commit -m "feat(tui): editable prompt with cursor and history"
```

---

### Task 5: showcase — composer con edición e historial

**Files:**
- Modify: `crates/batten/examples/showcase/demos/composer.rs`

**Interfaces:**
- Consumes: `state::{Composer, History}`, `widgets::composer::render`, `Demo`.
- Produces: nada nuevo.

- [ ] **Step 1: Escribir el test**

En `demos/composer.rs`, `mod tests`, añade:

```rust
    #[test]
    fn up_recalls_history() {
        let mut d = ComposerDemo::new();
        for c in "hey".chars() {
            d.handle_key(KeyCode::Char(c));
        }
        d.handle_key(KeyCode::Enter);
        d.handle_key(KeyCode::Up);
        assert_eq!(d.composer.text(), "hey");
    }

    #[test]
    fn left_and_delete_edit_in_the_middle() {
        let mut d = ComposerDemo::new();
        d.composer.set_text("ac");
        d.handle_key(KeyCode::Home);
        d.handle_key(KeyCode::Right);
        d.handle_key(KeyCode::Char('b'));
        assert_eq!(d.composer.text(), "abc");
    }
```

- [ ] **Step 2: Ejecutar para ver fallar**

Run: `cargo test -p batten --examples`
Expected: FAIL (no hay historial ni teclas de edición).

- [ ] **Step 3: Implementar**

En `demos/composer.rs`, sustituye el `struct ComposerDemo`, su `impl` y el
`impl Demo` por:

```rust
pub struct ComposerDemo {
    composer: Composer,
    history: History,
    sent: Vec<String>,
}

impl ComposerDemo {
    pub fn new() -> Self {
        let mut composer = Composer::new();
        composer.set_text("edita esta línea\n y esta otra");
        Self {
            composer,
            history: History::new(),
            sent: Vec::new(),
        }
    }
}

impl Demo for ComposerDemo {
    fn title(&self) -> &str {
        "composer"
    }

    fn hint(&self) -> &str {
        "←/→/Home/End: cursor · ↑/↓: historial · Enter: enviar"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(4)]).split(area);
        let body: Vec<Line> = self
            .sent
            .iter()
            .map(|s| Line::from(format!("you: {s}")))
            .collect();
        f.render_widget(Paragraph::new(body), rows[0]);
        composer::render(f, rows[1], &self.composer, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter => {
                if let Some(text) = self.composer.submit() {
                    self.history.push(text.clone());
                    self.sent.push(text);
                }
            }
            KeyCode::Backspace => {
                self.composer.backspace();
                self.history.reset();
            }
            KeyCode::Delete => {
                self.composer.delete();
                self.history.reset();
            }
            KeyCode::Left => self.composer.move_left(),
            KeyCode::Right => self.composer.move_right(),
            KeyCode::Home => self.composer.move_home(),
            KeyCode::End => self.composer.move_end(),
            KeyCode::Up => {
                let current = self.composer.text().to_string();
                if let Some(prev) = self.history.prev(&current) {
                    let prev = prev.to_string();
                    self.composer.set_text(&prev);
                }
            }
            KeyCode::Down => {
                if let Some(next) = self.history.next() {
                    let next = next.to_string();
                    self.composer.set_text(&next);
                }
            }
            KeyCode::Char(c) => {
                self.composer.insert_char(c);
                self.history.reset();
            }
            _ => return false,
        }
        true
    }
}
```

Ajusta los `use` del archivo para incluir:
`use batten::state::{Composer, History};`.

- [ ] **Step 4: Ejecutar para ver pasar**

Run: `cargo test -p batten --examples`
Expected: PASS (incluye el smoke).

- [ ] **Step 5: Commit**

```bash
git add crates/batten
git commit -m "feat(batten): composer editing and history in the showcase"
```

---

### Task 6: Verificación final

**Files:** ninguno.

- [ ] **Step 1: Checks**

Run: `cargo fmt --check`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

Run: `cargo test -p batten --examples && cargo test -p batten -p codeloom-core -p codeloom-tui`
Expected: PASS.

- [ ] **Step 2: Verificación manual**

Run: `cargo run -p codeloom-cli --bin loom`
Expected: escribir, mover el cursor con flechas, `Ctrl+J` añade línea sin enviar, `Enter` envía; `↑` recuerda lo enviado.

- [ ] **Step 3: Commit** (si hubo formato)

```bash
git add -A
git commit -m "chore: format after composer work"
```

---

## Criterios de aceptación

- [ ] El prompt permite editar con cursor, borrar en medio y varias líneas (`Ctrl+J`); `Enter` envía.
- [ ] `↑/↓` recuerdan lo enviado cuando hay una línea; con varias líneas mueven el cursor.
- [ ] El cursor se ve en la posición correcta.
- [ ] Todos los checks pasan.
