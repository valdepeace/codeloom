//! Componentes de actividad rehechos desde las referencias Ratatui.

use crate::demo::Demo;
use batten::widgets::{Theme, ThemePreset};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reference {
    Pawan,
    Martty,
    Mindfork,
}

pub struct ActivityReferenceDemo {
    selected: Reference,
    expanded: bool,
    thought_expanded: bool,
}

impl ActivityReferenceDemo {
    pub fn new() -> Self {
        Self {
            selected: Reference::Pawan,
            expanded: false,
            thought_expanded: false,
        }
    }

    fn pawan(&self) -> Vec<Line<'static>> {
        let accent = Color::Rgb(98, 102, 255);
        let success = Color::Rgb(80, 250, 123);
        let muted = Color::Gray;
        let result = [
            "Read src/main.rs",
            "Found fn main() at line 4",
            "Found App::run() at line 8",
            "Found setup_terminal() at line 21",
            "Found restore_terminal() at line 34",
            "No unsafe blocks in this file",
            "Imports: std::io, crossterm, ratatui",
            "File length: 42 lines",
            "UTF-8, LF line endings",
            "Read completed successfully",
        ];
        let shown = if self.expanded { result.len() } else { 6 };
        let mut lines = vec![Line::from(vec![
            Span::styled("  ╭─ ", Style::default().fg(accent)),
            Span::styled(
                "✓ ",
                Style::default().fg(success).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "read_file",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" src/main.rs", Style::default().fg(muted)),
            Span::styled("  · 148ms", Style::default().fg(Color::DarkGray)),
        ])];
        for row in result.iter().take(shown) {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(accent)),
                Span::styled(*row, Style::default().fg(Color::White)),
            ]));
        }
        if result.len() > shown {
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::default().fg(accent)),
                Span::styled(
                    format!("… {} more lines — press e to expand", result.len() - shown),
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
        lines.push(Line::styled("  ╰─", Style::default().fg(accent)));
        lines
    }

    fn martty(&self) -> Vec<Line<'static>> {
        let blue = Color::Rgb(86, 134, 254);
        let muted = Color::Rgb(151, 157, 166);
        let mut lines = vec![Line::styled(
            "✻ thought · 2.3s · 14 lines",
            Style::default().fg(muted),
        )];
        if self.thought_expanded {
            lines.extend([
                Line::styled(
                    "  Compare the requested file with the current branch.",
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
                Line::styled(
                    "  The entry point is src/main.rs; inspect its setup path.",
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
                Line::styled(
                    "  Then summarize the relevant functions.",
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
            ]);
        }
        lines.extend([
            Line::raw(""),
            Line::from(vec![
                Span::styled("⏺ ", Style::default().fg(Color::Green)),
                Span::styled(
                    "read_file",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("  src/main.rs", Style::default().fg(muted)),
                Span::styled(
                    if self.expanded { " ▾" } else { " ▸" },
                    Style::default().fg(blue),
                ),
            ]),
        ]);
        let output = [
            "use std::io;",
            "use ratatui::Terminal;",
            "",
            "fn main() {",
            "    let mut app = App::new();",
            "    app.run()?;",
            "}",
            "// 42 lines total",
        ];
        if self.expanded {
            lines.extend(output.iter().map(|line| {
                Line::from(vec![
                    Span::styled("│ ", Style::default().fg(blue)),
                    Span::styled(*line, Style::default().fg(muted)),
                ])
            }));
        } else {
            lines.extend(
                output
                    .iter()
                    .rev()
                    .take(4)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|line| {
                        Line::from(vec![
                            Span::styled("│ ", Style::default().fg(blue)),
                            Span::styled(*line, Style::default().fg(muted)),
                        ])
                    }),
            );
            lines.push(Line::styled(
                "│ last 4/8 lines · click to expand",
                Style::default().fg(muted).add_modifier(Modifier::ITALIC),
            ));
        }
        lines
    }

    fn mindfork(&self) -> Vec<Line<'static>> {
        let muted = Color::Rgb(133, 139, 146);
        let tool = Color::Rgb(220, 175, 97);
        let marker = if self.thought_expanded { "▾" } else { "▸" };
        let mut lines = vec![Line::from(vec![
            Span::styled(
                format!("{marker} thinking"),
                Style::default().fg(muted).add_modifier(Modifier::ITALIC),
            ),
            Span::styled(" · 3 lines", Style::default().fg(muted)),
            if self.thought_expanded {
                Span::raw("")
            } else {
                Span::styled("  Ctrl+T", Style::default().fg(muted))
            },
        ])];
        if self.thought_expanded {
            lines.extend([
                Line::styled(
                    "│ Check the source entry point and its terminal lifecycle.",
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
                Line::styled(
                    "│ The app builds the UI from a single run function.",
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
                Line::styled(
                    "│ Read the file before summarizing.",
                    Style::default().fg(muted).add_modifier(Modifier::ITALIC),
                ),
            ]);
        }
        let tool_marker = if self.expanded { "▾" } else { "▸" };
        lines.push(Line::from(vec![
            Span::styled("⚒ ", Style::default().fg(tool)),
            Span::styled(
                "read_file(src/main.rs)",
                Style::default().fg(tool).add_modifier(Modifier::BOLD),
            ),
            if self.expanded {
                Span::raw("")
            } else {
                Span::styled(
                    format!("  ·  Ctrl+O {tool_marker}"),
                    Style::default().fg(muted),
                )
            },
        ]));
        if self.expanded {
            lines.extend([
                Line::styled(
                    "│ arguments: {\"path\":\"src/main.rs\"}",
                    Style::default().fg(muted),
                ),
                Line::styled(
                    "│ result: 42 lines · Rust entry point and terminal setup",
                    Style::default().fg(muted),
                ),
            ]);
        }
        lines
    }
}

impl Demo for ActivityReferenceDemo {
    fn title(&self) -> &str {
        "actividad · referencias"
    }
    fn hint(&self) -> &str {
        "1/2/3: Pawan/Martty/Mindfork · x: expandir tool · t: thoughts · e: salida Pawan"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let (name, preset, lines) = match self.selected {
            Reference::Pawan => ("Pawan · tarjeta de tool", ThemePreset::Pawan, self.pawan()),
            Reference::Martty => (
                "Martty · reasoning y tool transcript",
                ThemePreset::Martty,
                self.martty(),
            ),
            Reference::Mindfork => (
                "Mindfork · bloques plegables",
                ThemePreset::Mindfork,
                self.mindfork(),
            ),
        };
        let theme = Theme::preset(preset);
        let [title, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        f.render_widget(
            Paragraph::new(Line::styled(
                name,
                theme.style(batten::state::StyleToken::Heading),
            )),
            title,
        );
        f.render_widget(
            Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false }),
            body,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('1') => self.selected = Reference::Pawan,
            KeyCode::Char('2') => self.selected = Reference::Martty,
            KeyCode::Char('3') => self.selected = Reference::Mindfork,
            KeyCode::Char('x') => self.expanded = !self.expanded,
            KeyCode::Char('e') => self.expanded = !self.expanded,
            KeyCode::Char('t') => self.thought_expanded = !self.thought_expanded,
            _ => return false,
        }
        true
    }
}
