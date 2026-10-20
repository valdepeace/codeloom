//! Composición de chat/transcript de Crush.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub struct CrushChatDemo {
    full_shell: bool,
    show_error: bool,
}

impl CrushChatDemo {
    pub fn new(full_shell: bool) -> Self {
        Self {
            full_shell,
            show_error: false,
        }
    }

    fn render_transcript(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [user, assistant, tool, result, error] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Length(2),
            Constraint::Min(4),
            Constraint::Length(2),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "You",
                    theme.styles.user.add_modifier(Modifier::BOLD),
                )),
                Line::from("Add a regression test for older session records."),
            ]),
            user,
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Assistant",
                    theme.styles.assistant.add_modifier(Modifier::BOLD),
                )),
                Line::from("I'll inspect the session loader and keep the current format backwards compatible."),
                Line::from(Span::styled("The version field is optional in older records.", theme.styles.muted)),
            ])
            .wrap(Wrap { trim: false }),
            assistant,
        );
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("● ", theme.styles.success),
                Span::styled("View", theme.styles.tool_name),
                Span::styled("  src/session.rs", theme.styles.muted),
            ])),
            tool,
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("pub fn load_session(data: &[u8]) -> Result<Session> {"),
                Line::from("    decode_legacy_or_current(data)"),
                Line::from("}"),
                Line::from(Span::styled(
                    "… (4 lines hidden) [click or space to expand]",
                    theme.styles.muted,
                )),
            ])
            .block(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(theme.styles.border),
            ),
            result,
        );
        if self.show_error {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("ERROR ", theme.styles.error.add_modifier(Modifier::BOLD)),
                    Span::styled(
                        "Test failed: legacy version was not preserved",
                        theme.styles.error,
                    ),
                ])),
                error,
            );
        }
    }
}

impl Demo for CrushChatDemo {
    fn title(&self) -> &str {
        if self.full_shell {
            "Crush · sidebar + chat"
        } else {
            "Crush · transcript"
        }
    }

    fn hint(&self) -> &str {
        "e: alternar mensaje de error · chat con sidebar/composer o feed de mensajes"
    }

    fn fullscreen(&self) -> bool {
        self.full_shell
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.full_shell {
            self.render_transcript(frame, area, theme);
            return;
        }
        let [sidebar, main] =
            Layout::horizontal([Constraint::Length(32), Constraint::Min(1)]).areas(area);
        super::crush_context_sidebar::render_crush_sidebar(frame, sidebar, theme);
        let [feed, editor, footer] = Layout::vertical([
            Constraint::Min(8),
            Constraint::Length(5),
            Constraint::Length(1),
        ])
        .areas(main);
        self.render_transcript(frame, feed, theme);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "> ",
                    theme.styles.accent.add_modifier(Modifier::BOLD),
                )),
                Line::from("Add a regression test for the old format."),
                Line::from(Span::styled(
                    "::: Preserve the existing session schema.",
                    theme.styles.muted,
                )),
            ]),
            editor,
        );
        frame.render_widget(
            Paragraph::new(Line::from("ctrl+c quit   ↑↓ history   ctrl+p commands"))
                .style(theme.styles.muted),
            footer,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code != KeyCode::Char('e') {
            return false;
        }
        self.show_error = !self.show_error;
        true
    }
}
