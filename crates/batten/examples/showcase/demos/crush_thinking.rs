//! Thinking de Crush con cola plegada y duración final.

use crate::demo::Demo;
use batten::widgets::{thinking, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const THOUGHTS: [&str; 13] = [
    "I should inspect how sessions are restored.",
    "The existing loader validates the project path.",
    "There is already a regression test for missing files.",
    "This change can stay local to the session model.",
    "I will compare the serialized fields first.",
    "The version field is optional for older records.",
    "New records should preserve the current format.",
    "I can add a test for the old format as well.",
    "The UI only consumes the session identifier.",
    "No migration seems necessary.",
    "I'll update the loader and its test now.",
    "Then I'll check the diff for unrelated changes.",
    "The result should remain backwards compatible.",
];

pub struct CrushThinkingDemo {
    expanded: bool,
    complete: bool,
}

impl CrushThinkingDemo {
    pub fn new() -> Self {
        Self {
            expanded: false,
            complete: false,
        }
    }
}

impl Demo for CrushThinkingDemo {
    fn title(&self) -> &str {
        "Crush · thinking"
    }

    fn hint(&self) -> &str {
        "t: plegar/expandir · c: razonamiento en curso/completo"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [caption, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(if self.expanded {
                "Reasoning · expanded"
            } else {
                "Reasoning · last 10 lines"
            })
            .style(theme.styles.muted),
            caption,
        );
        thinking::render_crush(
            frame,
            body,
            &THOUGHTS,
            self.expanded,
            self.complete,
            "4.2s",
            theme,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('t') => self.expanded = !self.expanded,
            KeyCode::Char('c') => self.complete = !self.complete,
            _ => return false,
        }
        true
    }
}
