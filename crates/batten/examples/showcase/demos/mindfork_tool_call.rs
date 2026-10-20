//! Llamada de herramienta de Mindfork, agrupada y plegable.

use crate::demo::Demo;
use batten::widgets::{tool_item, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MindforkToolCallDemo {
    expanded: bool,
    running: bool,
}

impl MindforkToolCallDemo {
    pub fn new() -> Self {
        Self {
            expanded: false,
            running: true,
        }
    }
}

impl Demo for MindforkToolCallDemo {
    fn title(&self) -> &str {
        "Mindfork · tool call"
    }

    fn hint(&self) -> &str {
        "r: ejecución/completada · x: mostrar/ocultar detalles"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let [heading, card] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new("note_save · guardar una preferencia")
                .style(Style::default().fg(Color::Rgb(201, 204, 210))),
            heading,
        );
        tool_item::render_mindfork(
            frame,
            card,
            "note_save",
            r#"{"content":"12 GB GPU; Q5_K_M at 16k context"}"#,
            "Note saved (id=6d665f64-656d-6f5f-7465-000000000001).",
            self.running,
            self.expanded,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('r') => self.running = !self.running,
            KeyCode::Char('x') => self.expanded = !self.expanded,
            _ => return false,
        }
        true
    }
}
