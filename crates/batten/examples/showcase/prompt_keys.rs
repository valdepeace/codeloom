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
