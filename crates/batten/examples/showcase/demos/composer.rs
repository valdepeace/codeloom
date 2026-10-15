//! Demo del composer: edición con cursor e historial.

use crate::demo::Demo;
use crate::prompt_keys::{apply, Outcome};
use batten::state::{Composer, History};
use batten::widgets::{composer, Theme};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

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
        "escribe · Ctrl+J salto · Enter envía · ↑↓ historial/cursor"
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn typing_then_enter_appends_a_sent_line() {
        let mut d = ComposerDemo::new();
        d.composer.set_text("");
        d.handle_key(KeyCode::Char('h'));
        d.handle_key(KeyCode::Char('i'));
        d.handle_key(KeyCode::Enter);
        assert_eq!(d.sent, vec!["hi".to_string()]);
        assert!(d.composer.is_empty());
    }

    #[test]
    fn backspace_removes_a_char() {
        let mut d = ComposerDemo::new();
        d.composer.set_text("a");
        d.handle_key(KeyCode::Backspace);
        assert!(d.composer.is_empty());
    }

    #[test]
    fn up_recalls_history() {
        let mut d = ComposerDemo::new();
        d.composer.set_text("");
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
}
