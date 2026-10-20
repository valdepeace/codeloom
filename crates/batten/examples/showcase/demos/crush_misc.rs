//! Inicialización, detalles, inline editor, salida y navegación de Crush.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Clone, Copy)]
pub enum CrushMiscKind {
    Initialize,
    SessionDetails,
    ExitBanner,
    InlineEditor,
    SidebarScroll,
    Help,
}

pub struct CrushMiscDemo {
    kind: CrushMiscKind,
    selected: usize,
    offset: usize,
}

impl CrushMiscDemo {
    pub fn new(kind: CrushMiscKind) -> Self {
        Self {
            kind,
            selected: 0,
            offset: 0,
        }
    }
}

impl Demo for CrushMiscDemo {
    fn title(&self) -> &str {
        match self.kind {
            CrushMiscKind::Initialize => "Crush · initialize project",
            CrushMiscKind::SessionDetails => "Crush · compact session details",
            CrushMiscKind::ExitBanner => "Crush · exit banner",
            CrushMiscKind::InlineEditor => "Crush · inline editor",
            CrushMiscKind::SidebarScroll => "Crush · sidebar scrolling",
            CrushMiscKind::Help => "Crush · expanded keyboard help",
        }
    }

    fn hint(&self) -> &str {
        "↑↓: selección o scroll · estados visuales de Crush"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        match self.kind {
            CrushMiscKind::Initialize => {
                let [body, actions] =
                    Layout::vertical([Constraint::Min(5), Constraint::Length(1)]).areas(area);
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled(
                            "Would you like to initialize this project?",
                            theme.styles.heading.add_modifier(Modifier::BOLD),
                        )),
                        Line::from(Span::styled("~/src/loom", theme.styles.accent)),
                        Line::from("I examine your codebase and create a context file."),
                        Line::from(Span::styled(
                            "You can also initialize with ctrl+p.",
                            theme.styles.muted,
                        )),
                    ])
                    .wrap(Wrap { trim: false }),
                    body,
                );
                let yes = if self.selected == 0 {
                    "[ Yep! ]"
                } else {
                    "  Yep!  "
                };
                let no = if self.selected == 1 {
                    "[ Nope ]"
                } else {
                    "  Nope  "
                };
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(yes, theme.styles.success.add_modifier(Modifier::BOLD)),
                        Span::raw("   "),
                        Span::styled(no, theme.styles.muted),
                    ])),
                    actions,
                );
            }
            CrushMiscKind::SessionDetails => {
                let width = area.width.min(86);
                let height = area.height.min(18);
                let popup = Rect::new(
                    area.x + area.width.saturating_sub(width) / 2,
                    area.y + area.height.saturating_sub(height) / 2,
                    width,
                    height,
                );
                frame.render_widget(Clear, popup);
                let [title, model, sections, version] = Layout::vertical([
                    Constraint::Length(2),
                    Constraint::Length(4),
                    Constraint::Min(4),
                    Constraint::Length(1),
                ])
                .areas(Block::default().borders(Borders::ALL).inner(popup));
                frame.render_widget(
                    Block::default()
                        .title(" Session details · ctrl+d close ")
                        .borders(Borders::ALL)
                        .border_type(BorderType::Rounded)
                        .border_style(theme.styles.border),
                    popup,
                );
                frame.render_widget(
                    Paragraph::new(Span::styled(
                        "Refactor session loader",
                        theme.styles.heading,
                    )),
                    title,
                );
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from("Claude Sonnet 4.5  ·  Anthropic  ·  Thinking high"),
                        Line::from(Span::styled(
                            "Context 42%  ·  estimated",
                            theme.styles.muted,
                        )),
                    ]),
                    model,
                );
                let [files, lsp, mcp, skills] = Layout::horizontal([
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                    Constraint::Percentage(25),
                ])
                .areas(sections);
                for (rect, heading, body) in [
                    (files, "FILES", "session.rs +12 -2"),
                    (lsp, "LSP", "rust-analyzer"),
                    (mcp, "MCP", "github"),
                    (skills, "SKILLS", "review-pr"),
                ] {
                    frame.render_widget(
                        Paragraph::new(vec![
                            Line::from(Span::styled(heading, theme.styles.accent)),
                            Line::from(body),
                        ]),
                        rect,
                    );
                }
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "crush v0.98.2",
                        theme.styles.muted,
                    ))),
                    version,
                );
            }
            CrushMiscKind::ExitBanner => {
                let lines = [
                    Line::from(Span::styled(
                        "CRUSH",
                        theme.styles.accent.add_modifier(Modifier::BOLD),
                    )),
                    Line::from(Span::styled(
                        "Thanks for using Crush!  See you soon.",
                        theme.styles.muted,
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled("Session  ", theme.styles.accent),
                        Span::raw("Refactor session loader"),
                    ]),
                    Line::from(vec![
                        Span::styled("Continue ", theme.styles.accent),
                        Span::raw("crush -s 91a4c2f"),
                    ]),
                ];
                frame.render_widget(
                    Paragraph::new(lines.to_vec())
                        .block(Block::default().padding(ratatui::widgets::Padding::uniform(1))),
                    area,
                );
            }
            CrushMiscKind::InlineEditor => {
                let [prompt, editor] =
                    Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled(
                            "Which option should I use?",
                            theme.styles.heading,
                        )),
                        Line::from(Span::styled(
                            "Enter submit · Esc dismiss",
                            theme.styles.muted,
                        )),
                    ]),
                    prompt,
                );
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled("> ", theme.styles.accent)),
                        Line::from("Keep legacy records readable"),
                        Line::from(Span::styled(
                            "::: Add a fixture for the previous format",
                            theme.styles.muted,
                        )),
                    ])
                    .block(
                        Block::default()
                            .borders(Borders::BOTTOM)
                            .border_style(theme.styles.border),
                    ),
                    editor,
                );
            }
            CrushMiscKind::SidebarScroll => {
                let entries = [
                    "Session title",
                    "~/src/loom · main",
                    "Claude Sonnet 4.5",
                    "Thinking high",
                    "FILES",
                    "session.rs",
                    "ui.rs",
                    "LSP",
                    "rust-analyzer",
                    "MCP",
                    "github",
                    "filesystem",
                    "SKILLS",
                    "review-pr",
                    "summarize",
                    "test-helper",
                ];
                let visible = 8;
                let end = (self.offset + visible).min(entries.len());
                let lines: Vec<Line> = entries[self.offset..end]
                    .iter()
                    .enumerate()
                    .map(|(i, text)| {
                        let style = if text.chars().all(|c| c.is_ascii_uppercase() || c == ' ') {
                            theme.styles.accent
                        } else if i == 0 {
                            theme.styles.heading
                        } else {
                            theme.styles.default
                        };
                        Line::from(Span::styled(*text, style))
                    })
                    .collect();
                let thumb = ((self.offset * visible) / entries.len()).min(visible - 1);
                let [content, scroll] =
                    Layout::horizontal([Constraint::Min(1), Constraint::Length(1)]).areas(area);
                frame.render_widget(
                    Paragraph::new(lines).block(
                        Block::default()
                            .borders(Borders::LEFT)
                            .border_style(theme.styles.border),
                    ),
                    content,
                );
                let track = (0..visible)
                    .map(|i| {
                        Line::from(Span::styled(
                            if i == thumb { "█" } else { "│" },
                            theme.styles.scrollbar,
                        ))
                    })
                    .collect::<Vec<_>>();
                frame.render_widget(Paragraph::new(track), scroll);
            }
            CrushMiscKind::Help => {
                let lines = [
                    Line::from(vec![
                        Span::styled("ctrl+c ", theme.styles.heading),
                        Span::styled("quit", theme.styles.muted),
                        Span::raw("   "),
                        Span::styled("↑↓ ", theme.styles.heading),
                        Span::styled("history", theme.styles.muted),
                    ]),
                    Line::from(vec![
                        Span::styled("ctrl+p ", theme.styles.heading),
                        Span::styled("commands", theme.styles.muted),
                        Span::raw("   "),
                        Span::styled("shift+tab ", theme.styles.heading),
                        Span::styled("cycle mode", theme.styles.muted),
                    ]),
                    Line::from(vec![
                        Span::styled("ctrl+t ", theme.styles.heading),
                        Span::styled("tasks/queue", theme.styles.muted),
                        Span::raw("   "),
                        Span::styled("ctrl+d ", theme.styles.heading),
                        Span::styled("session details", theme.styles.muted),
                    ]),
                ];
                frame.render_widget(Paragraph::new(lines.to_vec()), area);
            }
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match (self.kind, code) {
            (CrushMiscKind::Initialize, KeyCode::Left | KeyCode::Right | KeyCode::Tab) => {
                self.selected = (self.selected + 1) % 2;
            }
            (CrushMiscKind::SidebarScroll, KeyCode::Up) => {
                self.offset = self.offset.saturating_sub(1)
            }
            (CrushMiscKind::SidebarScroll, KeyCode::Down) => self.offset = (self.offset + 1).min(8),
            _ => return false,
        }
        true
    }
}
