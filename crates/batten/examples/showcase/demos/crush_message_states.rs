//! Estados secundarios de los mensajes de Crush.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Clone, Copy)]
pub enum CrushMessageKind {
    Pending,
    Plan,
    Refusal,
    Question,
    Summary,
    UserAttachment,
}

pub struct CrushMessageDemo {
    kind: CrushMessageKind,
    selected: usize,
}

impl CrushMessageDemo {
    pub fn new(kind: CrushMessageKind) -> Self {
        Self { kind, selected: 0 }
    }
}

impl Demo for CrushMessageDemo {
    fn title(&self) -> &str {
        match self.kind {
            CrushMessageKind::Pending => "Crush · pending assistant",
            CrushMessageKind::Plan => "Crush · plan card",
            CrushMessageKind::Refusal => "Crush · refusal/error",
            CrushMessageKind::Question => "Crush · inline question",
            CrushMessageKind::Summary => "Crush · assistant info",
            CrushMessageKind::UserAttachment => "Crush · user message with attachments",
        }
    }

    fn hint(&self) -> &str {
        "↑↓: recorrer opciones cuando la muestra contiene selección"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let lines = match self.kind {
            CrushMessageKind::Pending => vec![Line::from(vec![
                Span::styled("◌ ", theme.styles.accent),
                Span::styled("Thinking", theme.styles.muted),
            ])],
            CrushMessageKind::Plan => vec![
                Line::from(Span::styled(
                    "Plan",
                    theme.styles.accent.add_modifier(Modifier::BOLD),
                )),
                Line::from("1. Inspect the current session decoder"),
                Line::from("2. Add a legacy-format fixture"),
                Line::from("3. Verify current records still load"),
                Line::from(Span::styled(
                    "Plan ready · choose how to continue",
                    theme.styles.muted,
                )),
            ],
            CrushMessageKind::Refusal => vec![Line::from(vec![
                Span::styled("REFUSAL ", theme.styles.error.add_modifier(Modifier::BOLD)),
                Span::styled(
                    "I can't access that protected resource.",
                    theme.styles.error,
                ),
            ])],
            CrushMessageKind::Question => {
                let options = [
                    "Keep the old format",
                    "Add a migration",
                    "Ask before converting",
                ];
                let mut rows = vec![Line::from(Span::styled(
                    "Which compatibility strategy should I use?",
                    theme.styles.heading,
                ))];
                rows.extend(options.iter().enumerate().map(|(index, option)| {
                    let selected = index == self.selected % options.len();
                    Line::from(Span::styled(
                        format!("{} {option}", if selected { "›" } else { " " }),
                        if selected {
                            theme.styles.accent.add_modifier(Modifier::BOLD)
                        } else {
                            theme.styles.default
                        },
                    ))
                }));
                rows
            }
            CrushMessageKind::Summary => vec![
                Line::from(Span::styled("Session summary", theme.styles.heading)),
                Line::from("The decoder accepts both current and legacy records."),
                Line::from(Span::styled(
                    "3 files inspected · 2 tests added",
                    theme.styles.muted,
                )),
            ],
            CrushMessageKind::UserAttachment => vec![
                Line::from(Span::styled(
                    "You",
                    theme.styles.user.add_modifier(Modifier::BOLD),
                )),
                Line::from("Please review these session files."),
                Line::from(vec![
                    Span::styled("▧ ", theme.styles.accent),
                    Span::styled("session.rs  ", theme.styles.default),
                    Span::styled("▧ ", theme.styles.accent),
                    Span::styled("legacy.json", theme.styles.default),
                ]),
            ],
        };
        let mut paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        if matches!(self.kind, CrushMessageKind::Plan) {
            paragraph = paragraph.block(
                Block::default()
                    .title(" plan ")
                    .borders(Borders::ALL)
                    .border_style(theme.styles.accent),
            );
        }
        frame.render_widget(paragraph, area);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if !matches!(self.kind, CrushMessageKind::Question) {
            return false;
        }
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1) % 3,
            _ => return false,
        }
        true
    }
}
