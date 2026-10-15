# Foco y scroll de teclado al estilo Crush — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Separar el foco entre composer y transcript (`Tab`) y dar al transcript las teclas de scroll de Crush; `Esc` deja de cerrar loom.

**Architecture:** batten añade atajos de página a `Viewport` y `composer::render_focused` (sin foco: borde atenuado y sin cursor). loom añade `Focus` a `App` y enruta teclas según el foco. La demo `chat` hace lo mismo con `F5` (el showcase usa `Tab`).

**Tech Stack:** Rust, `ratatui` 0.30, crossterm (solo en loom y en el ejemplo).

**Spec:** diseño acordado en conversación (2026-10-15); referencia `crush/internal/ui/model/keys.go`. Solo teclado (sin ratón).

## Global Constraints

- `batten::state` sin `ratatui`/`crossterm`; batten sin crossterm (el ejemplo sí).
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104`, hay un ejecutable abierto: reportarlo, no matar procesos.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- Con foco en el transcript, las letras no llegan al composer. Tests en Tasks 2 y 3.
- `Esc` no cierra loom; `Ctrl+C`/`Ctrl+Q` sí. Test en Task 2.
- Páginas con `rows == 0` no se quedan quietas para siempre (mínimo 1 fila). Test en Task 1.

## Teclas (foco en transcript)

`↑`/`k` y `↓`/`j`: una fila · `u`/`d`: media página · `b`/`PgUp`: página arriba · `f`/`PgDn`/espacio: página abajo · `g`/`Home`: inicio · `G`/`End`: final · `Esc`: volver al composer.

---

### Task 1: batten — atajos de página y composer con foco

**Files:**
- Modify: `crates/batten/src/state/viewport.rs`, `crates/batten/src/widgets/composer.rs`

**Interfaces:**
- Produces: `Viewport::{page_up, page_down, half_page_up, half_page_down, to_top}`; `batten::widgets::composer::render_focused(frame, area, composer: &Composer, focused: bool, theme: &Theme)`. `composer::render` se mantiene (equivale a `focused = true`).

- [ ] **Step 1: `Viewport`**

En `crates/batten/src/state/viewport.rs`, dentro de `impl Viewport`, después de `scroll_down`, añadir:

```rust
    /// Una página arriba (las filas visibles; mínimo 1).
    pub fn page_up(&mut self) {
        self.scroll_up(self.rows.max(1));
    }

    /// Una página abajo (las filas visibles; mínimo 1).
    pub fn page_down(&mut self) {
        self.scroll_down(self.rows.max(1));
    }

    /// Media página arriba (mínimo 1 fila).
    pub fn half_page_up(&mut self) {
        self.scroll_up((self.rows / 2).max(1));
    }

    /// Media página abajo (mínimo 1 fila).
    pub fn half_page_down(&mut self) {
        self.scroll_down((self.rows / 2).max(1));
    }

    /// Al principio (deja de seguir el final si no cabe todo).
    pub fn to_top(&mut self) {
        self.scroll_up(usize::MAX);
    }
```

Y en su `mod tests`, añadir:

```rust
    #[test]
    fn page_helpers_move_by_rows() {
        let mut v = vp(100, 10);
        v.page_up();
        assert_eq!(v.window(), 80..90);
        v.half_page_up();
        assert_eq!(v.window(), 75..85);
        v.half_page_down();
        v.page_down();
        assert!(v.is_following());
        v.to_top();
        assert_eq!(v.window(), 0..10);
    }

    #[test]
    fn page_with_zero_rows_still_moves() {
        let mut v = vp(100, 0);
        v.page_up();
        assert_eq!(v.top(), 99);
    }
```

- [ ] **Step 2: `composer::render_focused`**

En `crates/batten/src/widgets/composer.rs`, sustituir la función `pub fn render(...) { ... }` entera por:

```rust
/// Composer con foco (borde de acento y cursor).
pub fn render(frame: &mut Frame, area: Rect, composer: &Composer, theme: &Theme) {
    render_focused(frame, area, composer, true, theme);
}

/// Composer; sin foco, borde atenuado y sin cursor.
pub fn render_focused(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    theme: &Theme,
) {
    let border = if focused {
        StyleToken::Accent
    } else {
        StyleToken::Dim
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title("prompt")
        .border_style(theme.style(border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = Paragraph::new(composer.text().to_string()).style(theme.style(StyleToken::Default));
    frame.render_widget(text, inner);

    if !focused {
        return;
    }
    let (row, col) = composer.cursor_row_col();
    if (row as u16) < inner.height && (col as u16) < inner.width {
        frame.set_cursor_position((inner.x + col as u16, inner.y + row as u16));
    }
}
```

Y en su `mod tests`, añadir:

```rust
    #[test]
    fn border_shows_focus() {
        use ratatui::style::Color;
        let c = Composer::new();
        for (focused, color) in [(true, Color::Rgb(0, 212, 255)), (false, Color::Indexed(240))] {
            let mut term = Terminal::new(TestBackend::new(20, 3)).unwrap();
            term.draw(|f| render_focused(f, f.area(), &c, focused, &Theme::test()))
                .unwrap();
            assert_eq!(term.backend().buffer()[(0, 0)].fg, color);
        }
    }
```

- [ ] **Step 3: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si algún test existente del composer comprueba el color del borde `Muted` con foco, reportarlo (no cambiar el valor esperado).

- [ ] **Step 4: Commit**

```bash
git add crates/batten/src
git commit -m "feat(batten): atajos de página en el viewport y composer con foco"
```

---

### Task 2: loom — foco entre composer y transcript

**Files:**
- Modify: `crates/codeloom-tui/src/app.rs`, `crates/codeloom-tui/src/run.rs`, `crates/codeloom-tui/src/render.rs`

**Interfaces:**
- Produces: `app::Focus { Composer, Transcript }` (por defecto `Composer`), `App.focus: Focus`, `App::toggle_focus()`.

- [ ] **Step 1: `app.rs`**

1. Antes de `pub struct PendingPermission`, añadir:
   ```rust
   /// Dónde va el teclado.
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
   pub enum Focus {
       #[default]
       Composer,
       Transcript,
   }
   ```
2. En `pub struct App`, después de `pub viewport: batten::state::Viewport,`, añadir `pub focus: Focus,`.
3. En `App::new`, después de `viewport: batten::state::Viewport::new(),`, añadir `focus: Focus::Composer,`.
4. En `impl App`, añadir:
   ```rust
       /// Alterna el foco entre composer y transcript.
       pub fn toggle_focus(&mut self) {
           self.focus = match self.focus {
               Focus::Composer => Focus::Transcript,
               Focus::Transcript => Focus::Composer,
           };
       }
   ```

- [ ] **Step 2: `run.rs`**

1. Añadir `use crate::app::Focus;` junto a `use crate::app::App;`.
2. Antes de `pub fn handle_key`, añadir:
   ```rust
   /// Teclas con el foco en el transcript (como Crush).
   fn transcript_key(app: &mut App, code: KeyCode) {
       let vp = &mut app.viewport;
       match code {
           KeyCode::Up | KeyCode::Char('k') => vp.scroll_up(1),
           KeyCode::Down | KeyCode::Char('j') => vp.scroll_down(1),
           KeyCode::Char('u') => vp.half_page_up(),
           KeyCode::Char('d') => vp.half_page_down(),
           KeyCode::PageUp | KeyCode::Char('b') => vp.page_up(),
           KeyCode::PageDown | KeyCode::Char('f') | KeyCode::Char(' ') => vp.page_down(),
           KeyCode::Home | KeyCode::Char('g') => vp.to_top(),
           KeyCode::End | KeyCode::Char('G') => vp.follow_bottom(),
           KeyCode::Esc => app.focus = Focus::Composer,
           _ => {}
       }
   }
   ```
3. En `handle_key`, justo después del bloque `if let Some(p) = app.permission.as_mut() { ... return None; }`, añadir:
   ```rust
       if code == KeyCode::Tab {
           app.toggle_focus();
           return None;
       }
       if app.focus == Focus::Transcript {
           transcript_key(app, code);
           return None;
       }
   ```
4. En el `match code {` de `handle_key`, sustituir el brazo
   ```rust
           KeyCode::Esc => {
               app.should_quit = true;
               None
           }
   ```
   por `KeyCode::Esc => None,`.
5. En `mod tests`, sustituir el test `esc_quits` entero por:
   ```rust
       #[test]
       fn esc_does_not_quit_but_ctrl_q_does() {
           let mut app = App::new();
           handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
           assert!(!app.should_quit);
           handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
           assert!(app.should_quit);
       }

       #[test]
       fn tab_moves_focus_and_transcript_keys_scroll() {
           let mut app = App::new();
           app.viewport.set_geometry(100, 10);
           handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
           assert_eq!(app.focus, Focus::Transcript);
           handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
           assert_eq!(app.composer.text(), "");
           assert!(!app.viewport.is_following());
           handle_key(&mut app, KeyCode::Char('G'), KeyModifiers::NONE);
           assert!(app.viewport.is_following());
           handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
           assert_eq!(app.focus, Focus::Composer);
           assert!(!app.should_quit);
       }
   ```

- [ ] **Step 3: `render.rs`**

En `draw_composer`, sustituir la llamada `batten::widgets::composer::render(frame, area, &app.composer, &app.theme);` por:

```rust
    let focused = app.focus == crate::app::Focus::Composer;
    batten::widgets::composer::render_focused(frame, area, &app.composer, focused, &app.theme);
```

- [ ] **Step 4: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/codeloom-tui/src
git commit -m "feat(tui): Tab cambia el foco y el transcript se navega con teclas de Crush"
```

---

### Task 3: demo `chat` — foco con `F5`

**Files:**
- Modify: `crates/batten/examples/showcase/demos/chat.rs`

- [ ] **Step 1: Estado**

En `pub struct ChatDemo`, después de `wait: u32,`, añadir `focus_transcript: bool,`; en `ChatDemo::new`, después de `wait: 0,`, añadir `focus_transcript: false,`.

- [ ] **Step 2: Teclas**

En `handle_key`, justo después del bloque `if let Some(d) = self.dialog.as_mut() { ... return true; }`, añadir:

```rust
        if code == KeyCode::F(5) {
            self.focus_transcript = !self.focus_transcript;
            return true;
        }
        if self.focus_transcript {
            let vp = &mut self.viewport;
            match code {
                KeyCode::Up | KeyCode::Char('k') => vp.scroll_up(1),
                KeyCode::Down | KeyCode::Char('j') => vp.scroll_down(1),
                KeyCode::Char('u') => vp.half_page_up(),
                KeyCode::Char('d') => vp.half_page_down(),
                KeyCode::PageUp | KeyCode::Char('b') => vp.page_up(),
                KeyCode::PageDown | KeyCode::Char('f') | KeyCode::Char(' ') => vp.page_down(),
                KeyCode::Home | KeyCode::Char('g') => vp.to_top(),
                KeyCode::End | KeyCode::Char('G') => vp.follow_bottom(),
                _ => return false,
            }
            return true;
        }
```

- [ ] **Step 3: Pintado y ayuda**

1. En `render`, cambiar `composer::render(f, rows[2], &self.composer, theme);` por
   `composer::render_focused(f, rows[2], &self.composer, !self.focus_transcript, theme);`.
2. En `hint`, cambiar el texto devuelto por
   `"Enter: turno · F6: permiso · F5: foco chat (j/k u/d g/G) · F2 reinicia · F3 pausa · F4 paso"`.

- [ ] **Step 4: Test**

En `mod tests`, añadir:

```rust
    #[test]
    fn f5_focuses_the_transcript_and_letters_scroll() {
        let mut d = ChatDemo::new();
        d.viewport.set_geometry(100, 10);
        d.handle_key(KeyCode::F(5));
        d.handle_key(KeyCode::Char('k'));
        assert_eq!(d.composer.text(), "");
        assert!(!d.viewport.is_following());
        d.handle_key(KeyCode::F(5));
        d.handle_key(KeyCode::Char('k'));
        assert_eq!(d.composer.text(), "k");
    }
```

- [ ] **Step 5: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/batten/examples/showcase/demos/chat.rs
git commit -m "feat(batten): foco del transcript en la demo chat con F5"
```
