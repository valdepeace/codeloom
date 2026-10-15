# Selección de líneas del transcript y copia — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Seleccionar líneas del transcript con el teclado, resaltarlas y copiarlas (OSC 52 en loom).

**Architecture:** `state::Selection { anchor, head }` en índices de línea del documento (independiente del scroll) que devuelve el texto. `transcript::render_with_selection` resalta con el token `selection` y desplaza la vista para que el extremo se vea (`Viewport::ensure_visible`). La app decide cómo copiar: loom escribe OSC 52; el showcase muestra el texto extraído.

**Tech Stack:** Rust, `ratatui` 0.30, `base64` 0.22 (ya en el lockfile), crossterm solo en loom/ejemplo.

**Spec:** `docs/specs/2026-10-15-batten-selection.md` y P2 de `docs/specs/2026-10-15-batten-reference-findings-showcase.md`. Decisiones de sus preguntas abiertas: selección por **líneas**; solo **teclado**; copia por **OSC 52** (decisión de la app); sin expandir/colapsar.

## Global Constraints

- `batten::state` sin `ratatui`/`crossterm`; batten sin crossterm.
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` y los tests del crate tocado (`cargo test -p batten`, `cargo test -p batten --examples`, `cargo test -p codeloom-tui`).
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104` o "Acceso denegado" al borrar un `.exe`, hay un ejecutable del usuario abierto: no matarlo; seguir con los checks por crate de arriba.
- No crear directorios de target temporales (el disco C: está lleno).
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- `extend` nunca sale de `[0, len)`, también con transcript vacío. Test en Task 1.
- Al mover el extremo fuera de la vista, la vista lo sigue. Test en Task 1.
- En loom, `Esc` con selección la quita; sin selección vuelve al composer. Test en Task 3.
- OSC 52 con texto no ASCII se codifica bien (base64 de los bytes UTF-8). Test en Task 3.

## Teclas (foco en el transcript)

`v`: empezar selección en la última línea / quitarla · `j`/`k` o `↓`/`↑` con selección: mover el extremo · `y` o `c`: copiar · `Esc`: quitar selección (sin selección: volver al composer).

---

### Task 1: batten — `Selection`, token y resaltado

**Files:**
- Create: `crates/batten/src/state/selection.rs`
- Modify: `crates/batten/src/state/mod.rs`, `crates/batten/src/state/transcript.rs`, `crates/batten/src/state/viewport.rs`, `crates/batten/src/widgets/theme.rs`, `crates/batten/src/widgets/profile.rs`, `crates/batten/src/widgets/transcript.rs`

**Interfaces:**
- Produces: `batten::state::Selection::{at(usize) -> Selection, head(), range() -> RangeInclusive<usize>, contains(usize), extend(isize, len), text(&Transcript) -> String}` (`Copy`); `Viewport::ensure_visible(start, end)`; `Styles.selection`, `StyleToken::Selection`; `batten::widgets::transcript::render_with_selection(frame, area, &Transcript, &mut Viewport, Option<&Selection>, &Theme)`.

- [ ] **Step 1: Crear `crates/batten/src/state/selection.rs`**

```rust
//! Selección de líneas del transcript, en coordenadas del documento (índices
//! de línea), independiente del scroll. El widget resalta; la app copia.

use super::transcript::Transcript;
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    anchor: usize,
    head: usize,
}

impl Selection {
    /// Selección de una sola línea.
    pub fn at(line: usize) -> Self {
        Self {
            anchor: line,
            head: line,
        }
    }

    /// Extremo móvil (el que siguen las teclas).
    pub fn head(&self) -> usize {
        self.head
    }

    /// Líneas seleccionadas, de menor a mayor.
    pub fn range(&self) -> RangeInclusive<usize> {
        self.anchor.min(self.head)..=self.anchor.max(self.head)
    }

    pub fn contains(&self, line: usize) -> bool {
        self.range().contains(&line)
    }

    /// Mueve el extremo `delta` líneas, acotado a `[0, len)`.
    pub fn extend(&mut self, delta: isize, len: usize) {
        let max = len.saturating_sub(1);
        self.head = self.head.saturating_add_signed(delta).min(max);
    }

    /// Texto plano de las líneas seleccionadas, separadas por `\n`.
    pub fn text(&self, transcript: &Transcript) -> String {
        self.range()
            .filter_map(|i| transcript.line(i))
            .map(|l| l.plain())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Line, LineKind};

    fn three() -> Transcript {
        let mut t = Transcript::new();
        for s in ["uno", "dos", "tres"] {
            t.push(Line::new(LineKind::Assistant, s));
        }
        t
    }

    #[test]
    fn range_is_ordered_and_text_joined() {
        let mut s = Selection::at(2);
        s.extend(-1, 3);
        assert_eq!(s.range(), 1..=2);
        assert!(s.contains(1) && !s.contains(0));
        assert_eq!(s.text(&three()), "dos\ntres");
    }

    #[test]
    fn extend_is_clamped() {
        let mut s = Selection::at(1);
        s.extend(-10, 3);
        assert_eq!(s.head(), 0);
        s.extend(10, 3);
        assert_eq!(s.head(), 2);
        let mut e = Selection::at(0);
        e.extend(5, 0);
        assert_eq!(e.head(), 0);
    }
}
```

- [ ] **Step 2: Registrar en `state/mod.rs`**

Añadir `pub mod selection;` antes de `pub mod sidebar;`, y `pub use selection::Selection;` antes de `pub use sidebar::{Logo, Section, Sidebar};`.

- [ ] **Step 3: Token `selection`**

1. `crates/batten/src/state/transcript.rs`, en `enum StyleToken`, después de `DiffHunk,`: añadir `Selection,`.
2. `crates/batten/src/widgets/theme.rs`:
   - `struct Styles`, después de `pub diff_hunk: Style,`: `pub selection: Style,`;
   - `impl Default for Styles`, después de `diff_hunk: Style::default().fg(Color::Cyan),`: `selection: Style::default().bg(Color::Indexed(238)),`;
   - `set_named_style`, después de `StyleToken::DiffHunk => s.diff_hunk,`: `StyleToken::Selection => s.selection,`;
   - `apply_override`, después de `"diff_hunk" => s.diff_hunk = style,`:
     `"selection" => s.selection = Style::default().bg(style.fg.unwrap_or(Color::Reset)),` (en TOML el color de `selection` es el fondo);
   - `token_to_key`, después de `StyleToken::DiffHunk => "diff_hunk",`: `StyleToken::Selection => "selection",`;
   - en `mod tests`, añadir:
     ```rust
     #[test]
     fn selection_token_is_a_background() {
         let th = Theme::test();
         assert_eq!(th.style(StyleToken::Selection).bg, Some(Color::Indexed(238)));
         let th = Theme::from_toml("[styles]\nselection = \"blue\"\n").unwrap();
         assert_eq!(th.style(StyleToken::Selection).bg, Some(Color::Blue));
     }
     ```
3. `crates/batten/src/widgets/profile.rs`, en el literal `Styles { ... }`, después de `diff_hunk: a(s.diff_hunk),`: `selection: a(s.selection),`.

- [ ] **Step 4: `Viewport::ensure_visible`**

En `crates/batten/src/state/viewport.rs`, dentro de `impl Viewport`, después de `to_top`, añadir:

```rust
    /// Desplaza lo mínimo para que se vean las filas `[start, end)`.
    pub fn ensure_visible(&mut self, start: usize, end: usize) {
        if start < self.top {
            self.top = start;
        } else if end > self.top + self.rows {
            self.top = end.saturating_sub(self.rows).min(self.max_top());
        }
        self.follow = self.top >= self.max_top();
    }
```

Y en su `mod tests`:

```rust
    #[test]
    fn ensure_visible_moves_the_minimum() {
        let mut v = vp(100, 10);
        v.ensure_visible(5, 6);
        assert_eq!(v.window(), 5..15);
        assert!(!v.is_following());
        v.ensure_visible(50, 52);
        assert_eq!(v.window(), 42..52);
        v.ensure_visible(98, 100);
        assert!(v.is_following());
    }
```

- [ ] **Step 5: `render_with_selection` (`crates/batten/src/widgets/transcript.rs`)**

1. Sustituir la función `pub fn render(...)` entera por estas dos:

```rust
/// Pinta el transcript sin selección.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut crate::state::Viewport,
    theme: &Theme,
) {
    render_with_selection(frame, area, transcript, viewport, None, theme);
}

/// Pinta el transcript. Una sola geometría (filas tras el wrap) alimenta el
/// pintado, el scroll y la scrollbar; la fija en `viewport`. Las líneas de
/// `selection` se resaltan y su extremo se mantiene visible.
pub fn render_with_selection(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut crate::state::Viewport,
    selection: Option<&crate::state::Selection>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = area.height as usize;
    let sel_style = theme.styles.selection;
    // ponytail: se compone todo el historial en cada frame; cachear por línea
    // si se nota en conversaciones largas.
    let lines: Vec<TLine> = (0..transcript.len())
        .filter_map(|i| transcript.line(i).map(|l| (i, l)))
        .flat_map(|(i, l)| {
            let mut out = render_line(l, theme);
            if selection.is_some_and(|s| s.contains(i)) {
                for tl in &mut out {
                    tl.style = tl.style.patch(sel_style);
                }
            }
            out
        })
        .collect();
    let wrap = Wrap { trim: false };
    let para = Paragraph::new(lines.clone()).wrap(wrap);
    let full = para.line_count(area.width);
    let overflow = full > rows && area.width >= 2;
    let text_area = if overflow {
        Rect {
            width: area.width - 1,
            ..area
        }
    } else {
        area
    };
    let total = if overflow {
        para.line_count(text_area.width)
    } else {
        full
    };
    viewport.set_geometry(total, rows);
    if let (Some(s), false) = (selection, lines.is_empty()) {
        let head = s.head().min(lines.len() - 1);
        let w = text_area.width;
        let start = Paragraph::new(lines[..head].to_vec()).wrap(wrap).line_count(w);
        let height = Paragraph::new(vec![lines[head].clone()])
            .wrap(wrap)
            .line_count(w);
        viewport.ensure_visible(start, start + height);
    }
    // ponytail: `Paragraph::scroll` usa u16; más de 65535 filas se quedan en el tope.
    let top = u16::try_from(viewport.top()).unwrap_or(u16::MAX);
    frame.render_widget(para.scroll((top, 0)), text_area);
    if overflow {
        let bar = Rect {
            x: area.x + area.width - 1,
            width: 1,
            ..area
        };
        crate::widgets::scrollbar::render(frame, bar, viewport.scroll(), theme);
    }
}
```

2. En su `mod tests`, añadir:

```rust
    #[test]
    fn selected_lines_are_highlighted() {
        use crate::state::Selection;
        let mut t = Transcript::new();
        for s in ["uno", "dos", "tres"] {
            t.push(Line::new(LineKind::Assistant, s));
        }
        let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
        let mut vp = Viewport::new();
        let sel = Selection::at(1);
        term.draw(|f| {
            render_with_selection(f, f.area(), &t, &mut vp, Some(&sel), &Theme::test())
        })
        .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(0, 1)].bg, ratatui::style::Color::Indexed(238));
        assert_ne!(buf[(0, 0)].bg, ratatui::style::Color::Indexed(238));
    }

    #[test]
    fn the_selection_head_is_scrolled_into_view() {
        use crate::state::Selection;
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
        let mut vp = Viewport::new();
        let sel = Selection::at(0);
        term.draw(|f| {
            render_with_selection(f, f.area(), &t, &mut vp, Some(&sel), &Theme::test())
        })
        .unwrap();
        let out: String = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(out.contains("line 00"));
        assert!(!vp.is_following());
    }
```

- [ ] **Step 6: Checks**

Run: `cargo test -p batten && cargo test -p batten --examples && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si `Wrap` no es `Copy` (error "use of moved value: wrap"), usar `wrap.clone()` en los usos; nada más. Si falla con "missing field `selection`", falta el campo en `profile.rs` (Step 3.3).

- [ ] **Step 7: Commit**

```bash
git add crates/batten/src
git commit -m "feat(batten): selección de líneas del transcript con resaltado"
```

---

### Task 2: demo `transcript` con selección

**Files:**
- Modify: `crates/batten/examples/showcase/demos/transcript.rs`

- [ ] **Step 1: Estado e imports**

1. Cambiar `use batten::state::{Line, LineKind, Transcript, Viewport};` por `use batten::state::{Line, LineKind, Selection, Transcript, Viewport};`.
2. Añadir `use ratatui::layout::{Constraint, Layout};` y `use ratatui::widgets::Paragraph;` (si `Rect` ya se importa de `ratatui::layout`, unificar en `use ratatui::layout::{Constraint, Layout, Rect};`).
3. En `pub struct TranscriptDemo`, después de `viewport: Viewport,`, añadir `selection: Option<Selection>,` y `copied: Option<String>,`; en `new()`, después de `viewport: Viewport::new(),`, añadir `selection: None,` y `copied: None,`.

- [ ] **Step 2: Pintado**

Sustituir el cuerpo de `render` por:

```rust
        let [body, status] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        transcript::render_with_selection(
            f,
            body,
            &self.transcript,
            &mut self.viewport,
            self.selection.as_ref(),
            theme,
        );
        let msg = match (&self.copied, &self.selection) {
            (Some(text), _) => format!("copiado: {}", text.replace('\n', " ⏎ ")),
            (None, Some(s)) => format!("seleccionadas {} líneas · y: copiar", s.range().count()),
            (None, None) => "v: seleccionar".into(),
        };
        f.render_widget(Paragraph::new(msg).style(theme.styles.muted), status);
```

- [ ] **Step 3: Teclas**

1. Cambiar `hint` a `"↓/↑: scroll · g/G · v: seleccionar · j/k: extender · y: copiar"`.
2. Sustituir el cuerpo de `handle_key` por:

```rust
        let len = self.transcript.len();
        if let Some(mut s) = self.selection {
            match code {
                KeyCode::Up | KeyCode::Char('k') => s.extend(-1, len),
                KeyCode::Down | KeyCode::Char('j') => s.extend(1, len),
                KeyCode::Char('y') => {
                    self.copied = Some(s.text(&self.transcript));
                    self.selection = None;
                    return true;
                }
                KeyCode::Char('v') => {
                    self.selection = None;
                    return true;
                }
                _ => return false,
            }
            self.selection = Some(s);
            return true;
        }
        match code {
            KeyCode::Char('v') if len > 0 => {
                self.copied = None;
                self.selection = Some(Selection::at(len - 1));
            }
            KeyCode::Down => self.viewport.scroll_down(1),
            KeyCode::Up => self.viewport.scroll_up(1),
            KeyCode::Char('g') => self.viewport.scroll_up(usize::MAX),
            KeyCode::Char('G') => self.viewport.follow_bottom(),
            _ => return false,
        }
        true
```

- [ ] **Step 4: Test**

En `mod tests`, añadir:

```rust
    #[test]
    fn select_extend_and_copy() {
        // 10 líneas de tipos variados + 30 de relleno (0..29): v selecciona la
        // última y k extiende a la penúltima.
        let mut d = TranscriptDemo::new();
        d.handle_key(KeyCode::Char('v'));
        d.handle_key(KeyCode::Char('k'));
        d.handle_key(KeyCode::Char('y'));
        assert!(d.selection.is_none());
        assert_eq!(
            d.copied.as_deref(),
            Some("línea de relleno 28\nlínea de relleno 29")
        );
    }
```

- [ ] **Step 5: Checks**

Run: `cargo test -p batten --examples && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/batten/examples/showcase/demos/transcript.rs
git commit -m "feat(batten): selección y copia en la demo del transcript"
```

---

### Task 3: loom — seleccionar y copiar con OSC 52

**Files:**
- Modify: `crates/codeloom-tui/Cargo.toml`, `crates/codeloom-tui/src/lib.rs`, `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs`
- Create: `crates/codeloom-tui/src/clipboard.rs`

**Interfaces:**
- Produces: `clipboard::osc52(&str) -> String`; `App.selection: Option<batten::state::Selection>`, `App.clipboard: Option<String>`.

- [ ] **Step 1: Dependencia**

En `crates/codeloom-tui/Cargo.toml`, en `[dependencies]`, añadir `base64 = "0.22"`.

- [ ] **Step 2: Crear `crates/codeloom-tui/src/clipboard.rs`**

```rust
//! Copia al portapapeles con OSC 52: funciona por SSH y sin dependencias del
//! sistema; el terminal decide si lo acepta.

use base64::Engine;

/// Secuencia OSC 52 que pide al terminal copiar `text` al portapapeles.
pub fn osc52(text: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(text.as_bytes());
    format!("\x1b]52;c;{b64}\x07")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_utf8_as_base64() {
        assert_eq!(osc52("hi"), "\x1b]52;c;aGk=\x07");
        assert_eq!(osc52("ñ"), "\x1b]52;c;w7E=\x07");
    }
}
```

- [ ] **Step 3: Registrar en `lib.rs`**

Añadir `pub mod clipboard;` antes de `pub mod permission_flow;`.

- [ ] **Step 4: `App`**

En `crates/codeloom-tui/src/app.rs`:
1. En `pub struct App`, después de `pub focus: Focus,`, añadir:
   ```rust
       /// Líneas seleccionadas del transcript, si hay.
       pub selection: Option<batten::state::Selection>,
       /// Texto pendiente de enviar al portapapeles (lo consume el bucle).
       pub clipboard: Option<String>,
   ```
2. En `App::new`, después de `focus: Focus::Composer,`, añadir `selection: None,` y `clipboard: None,`.

- [ ] **Step 5: Teclas (`run.rs`)**

Al principio de la función `transcript_key`, antes de `let vp = &mut app.viewport;`, añadir:

```rust
    let len = app.transcript().len();
    if let Some(mut s) = app.selection {
        match code {
            KeyCode::Up | KeyCode::Char('k') => s.extend(-1, len),
            KeyCode::Down | KeyCode::Char('j') => s.extend(1, len),
            KeyCode::Char('y') | KeyCode::Char('c') => {
                app.status = format!("copiado ({} líneas)", s.range().count());
                app.clipboard = Some(s.text(&app.transcript()));
                app.selection = None;
                return;
            }
            KeyCode::Esc | KeyCode::Char('v') => {
                app.selection = None;
                return;
            }
            _ => return,
        }
        app.selection = Some(s);
        return;
    }
    if code == KeyCode::Char('v') && len > 0 {
        app.selection = Some(batten::state::Selection::at(len - 1));
        return;
    }
```

Y en `run`, dentro del bucle, justo antes de `terminal.draw(`, añadir:

```rust
        if let Some(text) = app.clipboard.take() {
            use std::io::Write;
            let mut out = std::io::stdout();
            let _ = out.write_all(crate::clipboard::osc52(&text).as_bytes());
            let _ = out.flush();
        }
```

En `mod tests` de `run.rs`, añadir:

```rust
    #[test]
    fn select_extend_copy_and_escape() {
        let mut app = App::new();
        for s in ["uno", "dos", "tres"] {
            app.items.push(crate::app::TranscriptItem {
                role: "you",
                text: s.into(),
            });
        }
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(app.clipboard.as_deref(), Some("dos\ntres"));
        assert!(app.selection.is_none());
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.selection.is_none());
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
    }
```

- [ ] **Step 6: Pintado (`render.rs`)**

En `draw_transcript`, sustituir la llamada a `batten::widgets::transcript::render(...)` por:

```rust
    batten::widgets::transcript::render_with_selection(
        frame,
        inner,
        &t,
        &mut app.viewport,
        app.selection.as_ref(),
        &app.theme,
    );
```

- [ ] **Step 7: Checks**

Run: `cargo test -p codeloom-tui && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/codeloom-tui Cargo.lock
git commit -m "feat(tui): seleccionar líneas del transcript y copiarlas con OSC 52"
```
