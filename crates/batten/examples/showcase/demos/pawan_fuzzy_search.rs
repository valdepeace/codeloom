//! Búsqueda difusa de comandos de Pawan desde su snapshot Ratatui.

use crate::demo::Demo;
use batten::widgets::{fuzzy_search, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

const COMMANDS: [&str; 5] = [
    "/help — Show this help list",
    "/model — Switch model at runtime",
    "/clear — Clear chat history",
    "/provider — Configure model providers",
    "/quit — Exit Pawan",
];

pub struct PawanFuzzySearchDemo {
    query: String,
    selected: usize,
}

impl PawanFuzzySearchDemo {
    pub fn new() -> Self {
        Self {
            query: "help".into(),
            selected: 0,
        }
    }

    fn results(&self) -> Vec<String> {
        COMMANDS
            .iter()
            .filter(|command| command.to_lowercase().contains(&self.query.to_lowercase()))
            .map(|command| (*command).to_string())
            .collect()
    }
}

impl Demo for PawanFuzzySearchDemo {
    fn title(&self) -> &str {
        "Pawan · fuzzy search"
    }

    fn hint(&self) -> &str {
        "escribe para filtrar · ↑/↓ seleccionar · Enter elegir"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(ratatui::style::Color::Black)),
            area,
        );
        fuzzy_search::render_pawan(frame, area, &self.query, &self.results(), self.selected);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.results().len().saturating_sub(1))
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Enter => return true,
            KeyCode::Char(ch) => {
                self.query.push(ch);
                self.selected = 0;
            }
            _ => return false,
        }
        true
    }
}
