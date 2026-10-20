//! Primitiva de botón de Batten y sus tonos de foco.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{button, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ButtonDemo {
    selected: usize,
}

impl ButtonDemo {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Demo for ButtonDemo {
    fn title(&self) -> &str {
        "Batten · buttons"
    }

    fn hint(&self) -> &str {
        "←/→ cambia el botón enfocado"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let buttons = [
            ("Permitir", Tone::Success),
            ("Preguntar", Tone::Warning),
            ("Denegar", Tone::Error),
        ];
        frame.render_widget(
            Paragraph::new(Line::from(button::button_row(
                &buttons,
                self.selected,
                theme,
            ))),
            area,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => self.selected = self.selected.saturating_sub(1),
            KeyCode::Right => self.selected = (self.selected + 1).min(2),
            _ => return false,
        }
        true
    }
}
