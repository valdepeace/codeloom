//! Composer dinámico de Crush.

use crate::demo::Demo;
use batten::state::Composer;
use batten::widgets::{composer, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct CrushComposerDemo {
    composer: Composer,
    filled: bool,
}

impl CrushComposerDemo {
    pub fn new() -> Self {
        let mut composer = Composer::new();
        composer.set_text("Inspect the session handling\nand add a regression test.");
        Self {
            composer,
            filled: true,
        }
    }
}

impl Demo for CrushComposerDemo {
    fn title(&self) -> &str {
        "Crush · composer"
    }

    fn hint(&self) -> &str {
        "e: placeholder/contenido · textarea sin borde, prompt por línea"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [meta, editor] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "PROMPT",
                    theme.styles.accent.add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "multiline · no border · 3–15 rows",
                    theme.styles.muted,
                )),
            ]),
            meta,
        );
        if self.filled {
            composer::render_crush(frame, editor, &self.composer, true, "Ask Crush…", theme);
        } else {
            let empty = Composer::new();
            composer::render_crush(frame, editor, &empty, true, "Ask Crush…", theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code != KeyCode::Char('e') {
            return false;
        }
        self.filled = !self.filled;
        true
    }
}
