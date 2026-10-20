//! Diff y acciones de navegación de código de Crush.

use crate::demo::Demo;
use batten::state::Diff;
use batten::widgets::{diff, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct CrushDiffDemo {
    diff: Diff,
    scroll: usize,
}

impl CrushDiffDemo {
    pub fn new() -> Self {
        Self {
            diff: Diff::new(
                "pub fn load_session(data: &[u8]) {\n    decode_current(data)\n}\n",
                "pub fn load_session(data: &[u8]) {\n    decode_legacy_or_current(data)\n}\n",
                2,
            ),
            scroll: 0,
        }
    }
}

impl Demo for CrushDiffDemo {
    fn title(&self) -> &str {
        "Crush · diff output"
    }

    fn hint(&self) -> &str {
        "↑↓: desplazar · diff de salida plegable en la tool View/Edit"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [header, body] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled("Edit src/session.rs", theme.styles.tool_name)),
                Line::from(Span::styled("+1  -1 · unified view", theme.styles.muted)),
            ]),
            header,
        );
        diff::render(frame, body, &self.diff, self.scroll, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Down => {
                self.scroll = (self.scroll + 1).min(self.diff.lines().len().saturating_sub(1))
            }
            _ => return false,
        }
        true
    }
}

pub struct CrushCodeNavigationDemo {
    selected: usize,
}

impl Demo for CrushCodeNavigationDemo {
    fn title(&self) -> &str {
        "Crush · code navigation tools"
    }

    fn hint(&self) -> &str {
        "↑↓: Definition / References / Call hierarchy / Symbols / Rename / Replace"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        const TOOLS: [&str; 6] = [
            "Definition",
            "References",
            "Call hierarchy",
            "Symbols",
            "Rename",
            "Replace symbol",
        ];
        let [list, detail] =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Percentage(58)])
                .areas(area);
        let rows = TOOLS
            .iter()
            .enumerate()
            .map(|(i, name)| {
                Line::from(Span::styled(
                    format!("{} {name}", if i == self.selected { "›" } else { " " }),
                    if i == self.selected {
                        theme.styles.accent.add_modifier(Modifier::BOLD)
                    } else {
                        theme.styles.default
                    },
                ))
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(rows), list);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(TOOLS[self.selected], theme.styles.heading)),
                Line::from("src/session.rs:42: load_session"),
                Line::from(Span::styled("1 result · rust-analyzer", theme.styles.muted)),
            ]),
            detail,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(5),
            _ => return false,
        }
        true
    }
}

impl CrushCodeNavigationDemo {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}
