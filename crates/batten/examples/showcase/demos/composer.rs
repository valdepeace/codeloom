//! Demo del composer: escribe y envía.

use crate::demo::Demo;
use batten::state::Composer;
use batten::widgets::{composer, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ComposerDemo {
    composer: Composer,
    sent: Vec<String>,
}

impl ComposerDemo {
    pub fn new() -> Self {
        Self {
            composer: Composer::new(),
            sent: Vec::new(),
        }
    }
}

impl Demo for ComposerDemo {
    fn title(&self) -> &str {
        "composer"
    }

    fn hint(&self) -> &str {
        "escribe · Enter: enviar · Backspace: borrar"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(3)]).split(area);
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
                    self.sent.push(text);
                }
            }
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Char(c) => self.composer.push_char(c),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn typing_then_enter_appends_a_sent_line() {
        let mut d = ComposerDemo::new();
        d.handle_key(KeyCode::Char('h'));
        d.handle_key(KeyCode::Char('i'));
        d.handle_key(KeyCode::Enter);
        assert_eq!(d.sent, vec!["hi".to_string()]);
        assert!(d.composer.is_empty());
    }

    #[test]
    fn backspace_removes_a_char() {
        let mut d = ComposerDemo::new();
        d.handle_key(KeyCode::Char('a'));
        d.handle_key(KeyCode::Backspace);
        assert!(d.composer.is_empty());
    }
}
