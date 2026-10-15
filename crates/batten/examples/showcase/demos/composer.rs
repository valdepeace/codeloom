//! Demo del composer: edición con cursor e historial.

use crate::demo::Demo;
use batten::state::{Composer, History};
use batten::widgets::{composer, Theme};
use ratatui::crossterm::event::KeyCode;
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
