# Showcase: modificadores y contrato del composer — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cerrar la primera entrega del spec de hallazgos (P1): el showcase transporta modificadores a las demos y las demos `composer` y `chat` cumplen el contrato aprobado del composer (edición en medio, `Ctrl+J` salto, `Enter` envía, historial con borrador; `↑/↓` mueven el cursor si hay varias líneas).

**Architecture:** `Demo::handle_key_event(code, mods)` con implementación por defecto que llama a `handle_key(code)` (las demás demos no cambian). `route` usa el nuevo método. Un módulo del ejemplo, `prompt_keys`, aplica el mismo mapeo de teclas que `codeloom-tui` sobre `Composer` + `History`.

**Tech Stack:** Rust, `ratatui` 0.30 (crossterm solo en el ejemplo).

**Spec:** `docs/specs/2026-10-15-batten-reference-findings-showcase.md` §4 P1 y §7 (cierre de la primera entrega); contrato en `docs/specs/2026-10-15-batten-composer-editing-design.md`.

## Global Constraints

- batten (la librería) sin crossterm; esto solo toca `examples/showcase`.
- Comentarios en español. Mensajes de commit: los indicados, tal cual.
- Cada commit pasa: `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- **Entorno (Git Bash en Windows):** antes de cualquier `cargo`, en el mismo comando:
  `export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER='C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Tools\MSVC\14.35.32215\bin\Hostx64\x64\link.exe'`.
  Si el enlazado falla con `LNK1104`, hay un ejecutable abierto: reportarlo, no matar procesos.
- No hacer `git push`. No tocar ficheros fuera de los listados. No editar Rust con scripts.

## Review Focus

- `Ctrl+<letra>` no escribe la letra en el composer. Test en Task 1.
- `↓` tras `↑` devuelve el borrador. Test en Task 1.
- `route` entrega los modificadores a la demo. Test en Task 1.

---

### Task 1: modificadores en `Demo` y `prompt_keys`

**Files:**
- Modify: `crates/batten/examples/showcase/demo.rs`, `crates/batten/examples/showcase/app.rs`, `crates/batten/examples/showcase/main.rs`
- Create: `crates/batten/examples/showcase/prompt_keys.rs`

**Interfaces:**
- Produces: `Demo::handle_key_event(&mut self, code: KeyCode, mods: KeyModifiers) -> bool` (por defecto `self.handle_key(code)`); `crate::prompt_keys::{apply(&mut Composer, &mut History, KeyCode, KeyModifiers) -> Outcome, Outcome { Ignored, Edited, Submitted(String) }}`.

- [ ] **Step 1: `demo.rs`**

1. Cambiar `use ratatui::crossterm::event::KeyCode;` por `use ratatui::crossterm::event::{KeyCode, KeyModifiers};`.
2. Dentro de `pub trait Demo`, después del método `handle_key`, añadir:
   ```rust
       /// Tecla con modificadores. Por defecto los ignora y llama a `handle_key`.
       fn handle_key_event(&mut self, code: KeyCode, _mods: KeyModifiers) -> bool {
           self.handle_key(code)
       }
   ```

- [ ] **Step 2: `app.rs` (`route`)**

En `route`, en el brazo `Focus::Demo => match code { ... other => { ... } }`, cambiar `d.handle_key(other);` por `d.handle_key_event(other, mods);`.

Y en su `mod tests`, añadir:

```rust
    struct Rec(std::rc::Rc<std::cell::Cell<Option<KeyModifiers>>>);

    impl Demo for Rec {
        fn title(&self) -> &str {
            "rec"
        }
        fn hint(&self) -> &str {
            ""
        }
        fn render(
            &mut self,
            _f: &mut ratatui::Frame,
            _a: ratatui::layout::Rect,
            _t: &batten::widgets::Theme,
        ) {
        }
        fn handle_key_event(&mut self, _code: KeyCode, mods: KeyModifiers) -> bool {
            self.0.set(Some(mods));
            true
        }
    }

    #[test]
    fn route_passes_modifiers_to_the_demo() {
        let seen = std::rc::Rc::new(std::cell::Cell::new(None));
        let mut show = Showcase::with_demos(vec![Box::new(Rec(seen.clone()))]);
        show.focus_demo_for_test();
        route(
            &mut show,
            KeyEventKind::Press,
            KeyCode::Char('j'),
            KeyModifiers::CONTROL,
        );
        assert_eq!(seen.get(), Some(KeyModifiers::CONTROL));
    }
```

- [ ] **Step 3: Crear `crates/batten/examples/showcase/prompt_keys.rs`**

```rust
//! Teclas del composer compartidas por las demos: el mismo contrato que
//! `codeloom-tui` (Enter envía, Ctrl+J salto, historial con borrador).

use batten::state::{Composer, History};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// La tecla no es del composer.
    Ignored,
    /// El texto o el cursor cambiaron (o Enter con texto vacío).
    Edited,
    /// Enter con texto: se envía y se guarda en el historial.
    Submitted(String),
}

/// Aplica una tecla al composer y su historial.
pub fn apply(c: &mut Composer, h: &mut History, code: KeyCode, mods: KeyModifiers) -> Outcome {
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    match code {
        KeyCode::Char('j') if ctrl => {
            c.newline();
            h.reset();
        }
        KeyCode::Enter => {
            return match c.submit() {
                Some(text) => {
                    h.push(text.clone());
                    Outcome::Submitted(text)
                }
                None => Outcome::Edited,
            };
        }
        KeyCode::Backspace => {
            c.backspace();
            h.reset();
        }
        KeyCode::Delete => {
            c.delete();
            h.reset();
        }
        KeyCode::Left => c.move_left(),
        KeyCode::Right => c.move_right(),
        KeyCode::Home => c.move_home(),
        KeyCode::End => c.move_end(),
        KeyCode::Up => {
            if !h.is_browsing() && c.lines() > 1 {
                c.move_up();
            } else {
                let current = c.text().to_string();
                if let Some(prev) = h.prev(&current) {
                    let prev = prev.to_string();
                    c.set_text(&prev);
                }
            }
        }
        KeyCode::Down => {
            if !h.is_browsing() {
                c.move_down();
            } else if let Some(next) = h.next() {
                let next = next.to_string();
                c.set_text(&next);
            }
        }
        KeyCode::Char(ch) if !ctrl => {
            c.insert_char(ch);
            h.reset();
        }
        _ => return Outcome::Ignored,
    }
    Outcome::Edited
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: KeyModifiers = KeyModifiers::NONE;

    fn typing(c: &mut Composer, h: &mut History, s: &str) {
        for ch in s.chars() {
            apply(c, h, KeyCode::Char(ch), NONE);
        }
    }

    #[test]
    fn ctrl_j_adds_a_line_and_enter_submits() {
        let (mut c, mut h) = (Composer::new(), History::new());
        typing(&mut c, &mut h, "a");
        apply(&mut c, &mut h, KeyCode::Char('j'), KeyModifiers::CONTROL);
        typing(&mut c, &mut h, "b");
        let out = apply(&mut c, &mut h, KeyCode::Enter, NONE);
        assert_eq!(out, Outcome::Submitted("a\nb".into()));
        assert_eq!(c.text(), "");
    }

    #[test]
    fn edits_in_the_middle() {
        let (mut c, mut h) = (Composer::new(), History::new());
        typing(&mut c, &mut h, "ac");
        apply(&mut c, &mut h, KeyCode::Left, NONE);
        typing(&mut c, &mut h, "b");
        assert_eq!(c.text(), "abc");
    }

    #[test]
    fn up_recalls_and_down_restores_the_draft() {
        let (mut c, mut h) = (Composer::new(), History::new());
        typing(&mut c, &mut h, "uno");
        apply(&mut c, &mut h, KeyCode::Enter, NONE);
        typing(&mut c, &mut h, "bo");
        apply(&mut c, &mut h, KeyCode::Up, NONE);
        assert_eq!(c.text(), "uno");
        apply(&mut c, &mut h, KeyCode::Down, NONE);
        assert_eq!(c.text(), "bo");
    }

    #[test]
    fn up_moves_the_cursor_when_multiline() {
        let (mut c, mut h) = (Composer::new(), History::new());
        c.set_text("x\ny");
        apply(&mut c, &mut h, KeyCode::Up, NONE);
        assert_eq!(c.cursor_row_col().0, 0);
        assert_eq!(c.text(), "x\ny");
    }

    #[test]
    fn ctrl_letters_are_not_typed() {
        let (mut c, mut h) = (Composer::new(), History::new());
        let out = apply(&mut c, &mut h, KeyCode::Char('x'), KeyModifiers::CONTROL);
        assert_eq!(out, Outcome::Ignored);
        assert_eq!(c.text(), "");
    }
}
```

- [ ] **Step 4: `main.rs`**

Añadir `mod prompt_keys;` después de `mod demos;`.

- [ ] **Step 5: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. Si clippy avisa de `dead_code` en `prompt_keys` (aún no lo usa ninguna demo), es lo esperado hasta la Task 2: añadir `#[allow(dead_code)]` encima de `pub fn apply` y de `pub enum Outcome` **solo** si clippy falla por eso, y quitarlos en la Task 2.

- [ ] **Step 6: Commit**

```bash
git add crates/batten/examples/showcase
git commit -m "feat(batten): el showcase pasa modificadores y comparte las teclas del composer"
```

---

### Task 2: demos `composer` y `chat` con el contrato completo

**Files:**
- Modify: `crates/batten/examples/showcase/demos/composer.rs`, `crates/batten/examples/showcase/demos/chat.rs` (y `prompt_keys.rs` solo para quitar los `#[allow(dead_code)]` si se añadieron)

- [ ] **Step 1: `demos/composer.rs`**

1. Cambiar `use ratatui::crossterm::event::KeyCode;` por `use ratatui::crossterm::event::{KeyCode, KeyModifiers};` y añadir `use crate::prompt_keys::{apply, Outcome};`.
2. Sustituir el método `fn handle_key(&mut self, code: KeyCode) -> bool { ... }` entero por:
   ```rust
       fn handle_key(&mut self, code: KeyCode) -> bool {
           self.handle_key_event(code, KeyModifiers::NONE)
       }

       fn handle_key_event(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
           match apply(&mut self.composer, &mut self.history, code, mods) {
               Outcome::Submitted(text) => {
                   self.sent.push(text);
                   true
               }
               Outcome::Edited => true,
               Outcome::Ignored => false,
           }
       }
   ```
3. Si el `hint` de la demo no menciona `Ctrl+J`, cambiarlo a `"escribe · Ctrl+J salto · Enter envía · ↑↓ historial/cursor"`.
4. Si algún test existente de la demo falla porque `↑` con varias líneas ahora mueve el cursor en vez de recordar historial, es el contrato aprobado: reportarlo con el assert exacto (no cambiar el valor esperado).

- [ ] **Step 2: `demos/chat.rs`**

1. En el `use batten::state::{...}`, añadir `History`.
2. Cambiar `use ratatui::crossterm::event::KeyCode;` por `use ratatui::crossterm::event::{KeyCode, KeyModifiers};` y añadir `use crate::prompt_keys::{apply, Outcome};`.
3. En `pub struct ChatDemo`, después de `composer: Composer,`, añadir `history: History,`; en `ChatDemo::new`, después de `composer: Composer::new(),`, añadir `history: History::new(),`.
4. Renombrar el método existente `fn handle_key(&mut self, code: KeyCode) -> bool` a `fn handle_key_event(&mut self, code: KeyCode, mods: KeyModifiers) -> bool` (mismo cuerpo) y añadir antes de él:
   ```rust
       fn handle_key(&mut self, code: KeyCode) -> bool {
           self.handle_key_event(code, KeyModifiers::NONE)
       }
   ```
5. En el último `match code { ... }` de `handle_key_event` (el que tiene `KeyCode::F(6)`), **quitar** los brazos `KeyCode::Enter => { ... }`, `KeyCode::Backspace => self.composer.backspace(),` y `KeyCode::Char(c) => self.composer.push_char(c),`, y sustituir el brazo final `_ => return false,` por:
   ```rust
               _ => match apply(&mut self.composer, &mut self.history, code, mods) {
                   Outcome::Submitted(text) => self.start_turn(text),
                   Outcome::Edited => {}
                   Outcome::Ignored => return false,
               },
   ```
6. En `hint`, cambiar el texto a
   `"Enter: turno · Ctrl+J salto · F6 permiso · F5 foco chat · F2 reinicia · F3 pausa · F4 paso"`.
7. En `mod tests`, añadir:
   ```rust
       #[test]
       fn composer_contract_in_the_chat() {
           let mut d = ChatDemo::new();
           d.handle_key(KeyCode::Char('a'));
           d.handle_key_event(KeyCode::Char('j'), KeyModifiers::CONTROL);
           d.handle_key(KeyCode::Char('b'));
           d.handle_key(KeyCode::Enter);
           assert_eq!(d.transcript.line(0).unwrap().plain(), "a\nb");
           d.handle_key(KeyCode::Up);
           assert_eq!(d.composer.text(), "a\nb");
       }
   ```

- [ ] **Step 3: Quitar `allow(dead_code)`**

Si en la Task 1 se añadieron `#[allow(dead_code)]` en `prompt_keys.rs`, quitarlos.

- [ ] **Step 4: Checks**

Run: `cargo build && cargo test && cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/batten/examples/showcase
git commit -m "feat(batten): demos composer y chat con el contrato completo del composer"
```
