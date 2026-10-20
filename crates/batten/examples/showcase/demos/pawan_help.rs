//! Ayuda de teclado de Pawan basada en el snapshot 80×24.

use crate::demo::Demo;
use batten::widgets::{help_overlay, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

pub struct PawanHelpDemo;

impl Demo for PawanHelpDemo {
    fn title(&self) -> &str {
        "Pawan · help"
    }

    fn hint(&self) -> &str {
        "Navigation · Commands · Slash Commands"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(ratatui::style::Color::Black)),
            area,
        );
        help_overlay::render_pawan(frame, area);
    }

    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
