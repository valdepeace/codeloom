//! The Mindfork help dialog's independently renderable tab strip.

use crate::demo::Demo;
use batten::widgets::{help_overlay, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MindforkHelpTabsDemo {
    active: usize,
}

impl MindforkHelpTabsDemo {
    pub fn new() -> Self {
        Self { active: 1 }
    }
}

impl Demo for MindforkHelpTabsDemo {
    fn title(&self) -> &str {
        "Mindfork · help tabs"
    }
    fn hint(&self) -> &str {
        "←/→: mover tab activa · la segunda muestra el overflow estrecho"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let [label, wide, narrow] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Length(2),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(Line::from("Mindfork · help/about tab strip"))
                .style(Style::default().fg(Color::White)),
            label,
        );
        help_overlay::render_mindfork_tabs(frame, wide, self.active);
        let narrow = Rect::new(narrow.x, narrow.y, narrow.width.min(32), narrow.height);
        help_overlay::render_mindfork_tabs(frame, narrow, self.active);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => self.active = (self.active + 5) % 6,
            KeyCode::Right => self.active = (self.active + 1) % 6,
            _ => return false,
        }
        true
    }
}
