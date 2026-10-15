# Geometría del transcript y anclaje del scroll — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Que el transcript cuente filas visuales (no líneas lógicas) para pintar, hacer scroll y dibujar la scrollbar; que la lectura no se mueva al llegar contenido; y que loom use ese transcript con scroll.

**Architecture:** El widget compone todo el historial en un `Paragraph` con wrap, pide a ratatui las filas (`line_count`, feature `unstable-rendered-line-info`) y pinta con `.scroll((top, 0))`. `Viewport` guarda `top` absoluto en filas, `follow` y la geometría del último pintado (la fija el widget con `set_geometry`). loom sustituye su `Paragraph` por el widget de batten con `PgUp/PgDn`.

**Tech Stack:** Rust (edición 2021), `ratatui` 0.30 (`unstable-rendered-line-info`).

**Spec:** P0 de `docs/specs/2026-10-15-batten-reference-findings-showcase.md`; diseño acordado en conversación (2026-10-15).

## Global Constraints

- `batten::state` sin `ratatui`/`crossterm`.
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104`, hay un ejecutable abierto: reportarlo, no matar procesos.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts (python/sed).

## Review Focus

- Una línea lógica larga que ocupa varias filas no esconde la última línea al seguir el final. Test en Task 1.
- Leyendo arriba, al llegar contenido la vista no se mueve. Tests en Task 1.
- Contenido que cabe: `scroll_up` no rompe el seguimiento. Test en Task 1.
- En loom, con más mensajes que filas se ve el último; `PgUp` deja de seguir. Tests en Task 2.
- Límites conocidos (marcados `ponytail:`): en un resize solo se acota `top`; se compone todo el historial en cada frame.

---

### Task 1: `Viewport` en filas y transcript con geometría única (batten)

**Files:**
- Modify: `crates/batten/Cargo.toml`, `crates/batten/src/state/viewport.rs`, `crates/batten/src/widgets/transcript.rs`, `crates/batten/examples/showcase/demos/transcript.rs`, `crates/batten/examples/showcase/demos/chat.rs`

**Interfaces:**
- Produces: `Viewport::{new, set_geometry(&mut self, total: usize, rows: usize), follow_bottom, is_following(&self) -> bool, scroll_up(n), scroll_down(n), top(&self) -> usize, window(&self) -> Range<usize>, scroll(&self) -> Scroll}`. **Desaparece** `window(total, rows)` y `scroll(total, rows)`.
- `batten::widgets::transcript::render(frame, area, transcript: &Transcript, viewport: &mut Viewport, theme: &Theme)` (ahora `&mut Viewport`).

- [ ] **Step 1: Feature de ratatui**

En `crates/batten/Cargo.toml`, cambiar `ratatui = "0.30"` por:

```toml
ratatui = { version = "0.30", features = ["unstable-rendered-line-info"] }
```

- [ ] **Step 2: Reescribir `crates/batten/src/state/viewport.rs` entero**

```rust
//! Estado de scroll en filas visuales (no líneas lógicas). El widget fija la
//! geometría al pintar (`set_geometry`); las teclas mueven `top`.

use std::ops::Range;

/// Posición de scroll para pintar una barra: el `offset` es la primera fila
/// visible, `content` el total de filas y `viewport` las filas visibles.
/// Puro (sin ratatui): el widget lo traduce a su `ScrollbarState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
    pub offset: usize,
    pub content: usize,
    pub viewport: usize,
}

/// Scroll del transcript. Por defecto sigue el final; si el usuario sube,
/// `top` queda fijo aunque llegue contenido nuevo.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    top: usize,
    follow: bool,
    total: usize,
    rows: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            top: 0,
            follow: true,
            total: 0,
            rows: 0,
        }
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lo llama el widget al pintar: filas totales del contenido y visibles.
    pub fn set_geometry(&mut self, total: usize, rows: usize) {
        self.total = total;
        self.rows = rows;
        // ponytail: en un resize solo se acota `top`; no se conserva la línea
        // que había arriba. Anclar a línea lógica si molesta.
        self.top = if self.follow {
            self.max_top()
        } else {
            self.top.min(self.max_top())
        };
    }

    fn max_top(&self) -> usize {
        self.total.saturating_sub(self.rows)
    }

    /// Vuelve al final y lo sigue.
    pub fn follow_bottom(&mut self) {
        self.follow = true;
        self.top = self.max_top();
    }

    pub fn is_following(&self) -> bool {
        self.follow
    }

    /// Sube `n` filas y deja de seguir el final. Si todo cabe, no hace nada.
    pub fn scroll_up(&mut self, n: usize) {
        if self.max_top() == 0 {
            return;
        }
        if self.follow {
            self.top = self.max_top();
        }
        self.follow = false;
        self.top = self.top.saturating_sub(n);
    }

    /// Baja `n` filas; al llegar al final vuelve a seguirlo.
    pub fn scroll_down(&mut self, n: usize) {
        self.top = self.top.saturating_add(n).min(self.max_top());
        if self.top >= self.max_top() {
            self.follow = true;
        }
    }

    /// Primera fila visible.
    pub fn top(&self) -> usize {
        self.top
    }

    /// Filas visibles `[top, top + rows)` acotadas al total.
    pub fn window(&self) -> Range<usize> {
        self.top..(self.top + self.rows).min(self.total)
    }

    /// Posición para la scrollbar.
    pub fn scroll(&self) -> Scroll {
        Scroll {
            offset: self.top,
            content: self.total,
            viewport: self.rows,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vp(total: usize, rows: usize) -> Viewport {
        let mut v = Viewport::new();
        v.set_geometry(total, rows);
        v
    }

    #[test]
    fn follows_bottom_by_default() {
        assert_eq!(vp(100, 10).window(), 90..100);
    }

    #[test]
    fn scroll_up_breaks_follow_and_clamps() {
        let mut v = vp(100, 10);
        v.scroll_up(25);
        assert!(!v.is_following());
        assert_eq!(v.window(), 65..75);
        v.scroll_up(10_000);
        assert_eq!(v.window(), 0..10);
    }

    #[test]
    fn reading_position_is_kept_when_content_grows() {
        let mut v = vp(100, 10);
        v.scroll_up(25);
        v.set_geometry(150, 10);
        assert_eq!(v.window(), 65..75);
        let mut following = vp(100, 10);
        following.set_geometry(150, 10);
        assert_eq!(following.window(), 140..150);
    }

    #[test]
    fn scrolling_down_to_the_end_resumes_follow() {
        let mut v = vp(100, 10);
        v.scroll_up(5);
        v.scroll_down(5);
        assert!(v.is_following());
        v.set_geometry(120, 10);
        assert_eq!(v.window(), 110..120);
    }

    #[test]
    fn content_that_fits_never_breaks_follow() {
        let mut v = vp(3, 10);
        v.scroll_up(1);
        assert!(v.is_following());
        assert_eq!(v.window(), 0..3);
    }

    #[test]
    fn resize_clamps_top() {
        let mut v = vp(100, 10);
        v.scroll_up(5);
        v.set_geometry(100, 50);
        assert_eq!(v.window(), 50..100);
    }

    #[test]
    fn handles_empty_and_zero_rows() {
        assert_eq!(vp(0, 10).window(), 0..0);
        assert_eq!(vp(3, 0).window(), 3..3);
    }

    #[test]
    fn scroll_reports_geometry() {
        assert_eq!(
            vp(100, 10).scroll(),
            Scroll {
                offset: 90,
                content: 100,
                viewport: 10
            }
        );
    }
}
```

- [ ] **Step 3: `render` del transcript (`crates/batten/src/widgets/transcript.rs`)**

Sustituir la función `pub fn render(...) { ... }` entera por:

```rust
/// Pinta el transcript. Una sola geometría (filas tras el wrap) alimenta el
/// pintado, el scroll y la scrollbar; la fija en `viewport`.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut crate::state::Viewport,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = area.height as usize;
    // ponytail: se compone todo el historial en cada frame; cachear por línea
    // si se nota en conversaciones largas.
    let lines: Vec<TLine> = (0..transcript.len())
        .filter_map(|i| transcript.line(i))
        .flat_map(|l| render_line(l, theme))
        .collect();
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
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

- [ ] **Step 4: Tests del widget**

En `mod tests` de `crates/batten/src/widgets/transcript.rs`:

1. Sustituir la función `buffer` por:
   ```rust
       fn draw(transcript: &Transcript, vp: &mut Viewport, w: u16, h: u16) -> String {
           let backend = TestBackend::new(w, h);
           let mut term = Terminal::new(backend).unwrap();
           term.draw(|f| render(f, f.area(), transcript, vp, &Theme::test()))
               .unwrap();
           term.backend()
               .buffer()
               .content
               .iter()
               .map(|c| c.symbol())
               .collect()
       }

       fn buffer(transcript: &Transcript, w: u16, h: u16) -> String {
           draw(transcript, &mut Viewport::new(), w, h)
       }
   ```
2. Añadir al final del módulo:
   ```rust
       #[test]
       fn wrapped_lines_count_as_rows_when_following() {
           let mut t = Transcript::new();
           t.push(Line::new(LineKind::User, "inicio"));
           t.push(Line::new(LineKind::Assistant, "palabra ".repeat(20)));
           t.push(Line::new(LineKind::Assistant, "fin"));
           let out = buffer(&t, 20, 3);
           assert!(out.contains("fin"), "la última línea debe verse");
           assert!(!out.contains("inicio"));
       }

       #[test]
       fn reading_up_keeps_the_view_while_content_grows() {
           let mut t = Transcript::new();
           for i in 0..50 {
               t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
           }
           let mut vp = Viewport::new();
           draw(&t, &mut vp, 30, 5);
           vp.scroll_up(10);
           let before = draw(&t, &mut vp, 30, 5);
           for i in 50..70 {
               t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
           }
           let after = draw(&t, &mut vp, 30, 5);
           assert!(before.contains("line 35"));
           assert!(after.contains("line 35"));
           assert!(!after.contains("line 69"));
       }

       #[test]
       fn following_shows_new_content() {
           let mut t = Transcript::new();
           for i in 0..50 {
               t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
           }
           let mut vp = Viewport::new();
           draw(&t, &mut vp, 30, 5);
           t.push(Line::new(LineKind::Assistant, "nueva"));
           assert!(draw(&t, &mut vp, 30, 5).contains("nueva"));
       }
   ```

- [ ] **Step 5: Demos del showcase**

1. `crates/batten/examples/showcase/demos/transcript.rs`:
   - en `render`, cambiar `&self.viewport` por `&mut self.viewport`;
   - sustituir el test `scroll_changes_the_window` por:
     ```rust
         #[test]
         fn scroll_changes_the_window() {
             let mut d = TranscriptDemo::new();
             d.viewport.set_geometry(d.transcript.len(), 5);
             d.handle_key(KeyCode::Char('G')); // al final
             let bottom = d.viewport.window();
             d.handle_key(KeyCode::Char('g')); // al principio
             let top = d.viewport.window();
             assert_ne!(bottom, top);
             assert_eq!(top.start, 0);
         }
     ```
2. `crates/batten/examples/showcase/demos/chat.rs`: en `render`, cambiar `&self.viewport` por `&mut self.viewport`.

- [ ] **Step 6: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS (si `fmt --check` falla, `cargo fmt` y repetir). Si `line_count` no existe, comprobar el Step 1.

- [ ] **Step 7: Commit**

```bash
git add crates/batten Cargo.lock
git commit -m "fix(batten): scroll del transcript en filas visuales y anclado al leer"
```

---

### Task 2: loom usa el transcript de batten con scroll

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/render.rs`, `crates/codeloom-tui/src/run.rs`

**Interfaces:**
- Consumes: `Viewport` y `transcript::render` de la Task 1.
- Produces: `App.viewport: batten::state::Viewport`, `App::transcript(&self) -> batten::state::Transcript`; `render::render(frame, app: &mut App)` (ahora `&mut`).

- [ ] **Step 1: `App` (`crates/codeloom-tui/src/app.rs`)**

1. En `pub struct App`, después de `pub changes: batten::state::FileChanges,`, añadir:
   ```rust
       /// Scroll del transcript (sigue el final salvo que el usuario suba).
       pub viewport: batten::state::Viewport,
   ```
2. En `App::new`, después de `changes: batten::state::FileChanges::new(),`, añadir `viewport: batten::state::Viewport::new(),`.
3. Dentro de `impl App`, añadir:
   ```rust
       /// Transcript de batten a partir de los items (una línea por línea de texto).
       pub fn transcript(&self) -> batten::state::Transcript {
           use batten::state::{Line, LineKind, Transcript};
           let mut t = Transcript::new();
           for item in &self.items {
               let kind = if item.role == "you" {
                   LineKind::User
               } else {
                   LineKind::Assistant
               };
               for l in item.text.lines() {
                   t.push(Line::new(kind, l));
               }
           }
           t
       }
   ```

- [ ] **Step 2: `render.rs`**

1. Sustituir la función `draw_transcript` entera por:
   ```rust
   fn draw_transcript(frame: &mut Frame, area: Rect, app: &mut App) {
       let block = Block::default().borders(Borders::TOP);
       let inner = block.inner(area);
       frame.render_widget(block, area);
       let t = app.transcript();
       batten::widgets::transcript::render(frame, inner, &t, &mut app.viewport, &app.theme);
   }
   ```
2. Cambiar la firma `pub fn render(frame: &mut Frame, app: &App)` por `pub fn render(frame: &mut Frame, app: &mut App)`.
3. Quitar del `use` los imports que queden sin usar (`Paragraph`/`Wrap`/`Line` si clippy o el compilador avisan); `Paragraph` se sigue usando en `draw_header`.
4. En `mod tests`:
   - cambiar la firma del helper a `fn buffer_of(app: &mut App, w: u16, h: u16) -> String` y dentro `term.draw(|f| render(f, app)).unwrap();`;
   - en cada test existente, declarar la app como `let mut app` y llamar `buffer_of(&mut app, ...)`;
   - añadir:
     ```rust
         fn chatty_app() -> App {
             let mut app = App::new();
             for i in 0..60 {
                 app.items.push(TranscriptItem {
                     role: "you",
                     text: format!("msg-{i:02}"),
                 });
             }
             app
         }

         #[test]
         fn newest_messages_are_visible_when_overflowing() {
             let mut app = chatty_app();
             let content = buffer_of(&mut app, 60, 20);
             assert!(content.contains("msg-59"));
             assert!(!content.contains("msg-00"));
         }

         #[test]
         fn page_up_scrolls_back_and_stops_following() {
             let mut app = chatty_app();
             buffer_of(&mut app, 60, 20);
             crate::run::handle_key(
                 &mut app,
                 crossterm::event::KeyCode::PageUp,
                 crossterm::event::KeyModifiers::NONE,
             );
             let content = buffer_of(&mut app, 60, 20);
             assert!(!app.viewport.is_following());
             assert!(!content.contains("msg-59"));
         }
     ```

- [ ] **Step 3: `run.rs`**

1. En `handle_key`, dentro del `match code {`, sustituir el brazo `KeyCode::Enter => app.submit(),` por:
   ```rust
           KeyCode::Enter => {
               let sent = app.submit();
               if sent.is_some() {
                   app.viewport.follow_bottom();
               }
               sent
           }
           KeyCode::PageUp => {
               app.viewport.scroll_up(10);
               None
           }
           KeyCode::PageDown => {
               app.viewport.scroll_down(10);
               None
           }
   ```
2. En `run`, dentro de `terminal.draw(|f| { ... })`, cambiar `render(f, &app);` por `render(f, &mut app);`.

- [ ] **Step 4: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui/src
git commit -m "fix(tui): transcript con scroll; los mensajes nuevos ya se ven"
```
