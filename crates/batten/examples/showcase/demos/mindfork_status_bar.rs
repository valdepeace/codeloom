//! Barra inferior de Mindfork basada en `chat-dark-en.png`.

use crate::demo::Demo;
use batten::widgets::{status_bar, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MindforkStatusBarDemo;

impl MindforkStatusBarDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for MindforkStatusBarDemo {
    fn title(&self) -> &str {
        "Mindfork · status bar"
    }

    fn hint(&self) -> &str {
        "barra de estado Mindfork · indicadores y atajos"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let bg = Style::default()
            .fg(Color::Rgb(201, 204, 210))
            .bg(Color::Rgb(15, 17, 21));
        let [body, footer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
        frame.render_widget(Paragraph::new("◆ Gemma 4 on a 12 GB GPU").style(bg), body);
        status_bar::render_mindfork(frame, footer);
    }
}
