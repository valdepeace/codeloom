//! Pantalla de bienvenida de Pawan basada en su snapshot 80×24.

use crate::demo::Demo;
use batten::widgets::{welcome, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::widgets::Block;
use ratatui::Frame;

pub struct PawanWelcomeDemo;

impl Demo for PawanWelcomeDemo {
    fn title(&self) -> &str {
        "Pawan · welcome"
    }

    fn hint(&self) -> &str {
        "tarjeta inicial centrada · snapshot 80×24"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            area,
        );
        welcome::render_pawan(frame, area, "test-model", "[CWD]", "0.5.23");
    }

    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
