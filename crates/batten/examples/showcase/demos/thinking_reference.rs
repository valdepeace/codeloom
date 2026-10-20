//! Variantes de thinking de Mindfork y Martty.

use crate::demo::Demo;
use batten::widgets::{thinking, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const THOUGHTS: [&str; 3] = [
    "Compare the requested file with the current branch.",
    "Inspect the entry point and terminal setup before summarizing.",
    "Keep the result concise and show the relevant source lines.",
];

pub struct ThinkingReferenceDemo {
    expanded: bool,
}

impl ThinkingReferenceDemo {
    pub fn new() -> Self {
        Self { expanded: false }
    }
}

impl Demo for ThinkingReferenceDemo {
    fn title(&self) -> &str {
        "thinking · Mindfork + Martty"
    }

    fn hint(&self) -> &str {
        "t: expandir/plegar ambas variantes"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [mindfork_title, mindfork, martty_title, martty] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(1),
            Constraint::Min(2),
        ])
        .areas(area);
        let heading = Style::default().fg(Color::Rgb(151, 157, 166));
        frame.render_widget(
            Paragraph::new(Line::from("Mindfork · thinking plegable")).style(heading),
            mindfork_title,
        );
        thinking::render_mindfork(frame, mindfork, &THOUGHTS, self.expanded);
        frame.render_widget(
            Paragraph::new(Line::from("Martty · resumen temporal"))
                .style(theme.style(batten::state::StyleToken::Heading)),
            martty_title,
        );
        thinking::render_martty(frame, martty, &THOUGHTS, "2.3s", self.expanded, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code != KeyCode::Char('t') {
            return false;
        }
        self.expanded = !self.expanded;
        true
    }
}
