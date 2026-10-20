//! Muestras específicas por familia de renderer de herramientas de Crush.

use crate::demo::Demo;
use batten::state::ToolStatus;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

#[derive(Clone, Copy)]
pub enum CrushToolKind {
    Shell,
    File,
    Search,
    Web,
    Agent,
    Mcp,
    Diagnostics,
    Todos,
    Generic,
}

pub struct CrushToolRendererDemo {
    kind: CrushToolKind,
    status: ToolStatus,
    expanded: bool,
}

impl CrushToolRendererDemo {
    pub fn new(kind: CrushToolKind) -> Self {
        Self {
            kind,
            status: ToolStatus::Success,
            expanded: false,
        }
    }

    fn name(&self) -> &'static str {
        match self.kind {
            CrushToolKind::Shell => "Bash / jobs",
            CrushToolKind::File => "View / Edit / Write",
            CrushToolKind::Search => "Grep / Glob / LS",
            CrushToolKind::Web => "WebSearch / Fetch",
            CrushToolKind::Agent => "Agent / AgenticFetch",
            CrushToolKind::Mcp => "MCP tools",
            CrushToolKind::Diagnostics => "Diagnostics / LSP",
            CrushToolKind::Todos => "Todos",
            CrushToolKind::Generic => "Generic fallback",
        }
    }

    fn content(&self, theme: &Theme) -> Vec<Line<'static>> {
        let muted = theme.styles.muted;
        let text = theme.styles.default;
        match self.kind {
            CrushToolKind::Shell => vec![
                Line::from(Span::styled(
                    "› cargo test -p codeloom-core",
                    theme.styles.accent,
                )),
                Line::from(Span::styled("running 18 tests", muted)),
                Line::from(Span::styled(
                    "test result: ok. 18 passed",
                    theme.styles.success,
                )),
            ],
            CrushToolKind::File => vec![
                Line::from(Span::styled(
                    "  41  pub fn load_session(data: &[u8]) {",
                    muted,
                )),
                Line::from("  42      decode_legacy_or_current(data)").style(text),
                Line::from(Span::styled("  43  }", muted)),
                Line::from(Span::styled("  +4 more lines · space to expand", muted)),
            ],
            CrushToolKind::Search => vec![
                Line::from(Span::styled("src/session.rs:42: fn load_session", text)),
                Line::from(Span::styled(
                    "tests/session.rs:87: load_legacy_record",
                    text,
                )),
                Line::from(Span::styled("2 matches in 2 files", muted)),
            ],
            CrushToolKind::Web => vec![
                Line::from(Span::styled(
                    "Session migration guide",
                    theme.styles.heading,
                )),
                Line::from(Span::styled(
                    "docs.example.dev/sessions · 3.2k chars",
                    muted,
                )),
                Line::from(Span::styled("Fetched successfully", theme.styles.success)),
            ],
            CrushToolKind::Agent => vec![
                Line::from(Span::styled(
                    "subagent: inspect-compatibility",
                    theme.styles.accent,
                )),
                Line::from(Span::styled("Checked decoder and legacy fixtures", text)),
                Line::from(Span::styled("Completed · 2 files reviewed", muted)),
            ],
            CrushToolKind::Mcp => vec![
                Line::from(Span::styled(
                    "github.search_repositories",
                    theme.styles.accent,
                )),
                Line::from(Span::styled("Found 3 matching repositories", text)),
                Line::from(Span::styled("server github · 1.1s", muted)),
            ],
            CrushToolKind::Diagnostics => vec![
                Line::from(Span::styled("src/session.rs", theme.styles.heading)),
                Line::from(Span::styled(
                    "warning  unused import at line 8",
                    theme.styles.warning,
                )),
                Line::from(Span::styled("1 diagnostic", muted)),
            ],
            CrushToolKind::Todos => vec![
                Line::from(Span::styled(
                    "✓ Inspect session decoding",
                    theme.styles.success,
                )),
                Line::from(Span::styled(
                    "◉ Add backwards compatibility test",
                    theme.styles.warning,
                )),
                Line::from(Span::styled("○ Run the test suite", muted)),
            ],
            CrushToolKind::Generic => vec![
                Line::from(Span::styled("tool_name", theme.styles.accent)),
                Line::from(Span::styled("input: { … }", muted)),
                Line::from(Span::styled("output: completed", text)),
            ],
        }
    }
}

impl Demo for CrushToolRendererDemo {
    fn title(&self) -> &str {
        match self.kind {
            CrushToolKind::Shell => "Crush · tool renderer · shell",
            CrushToolKind::File => "Crush · tool renderer · file",
            CrushToolKind::Search => "Crush · tool renderer · search",
            CrushToolKind::Web => "Crush · tool renderer · web",
            CrushToolKind::Agent => "Crush · tool renderer · agent",
            CrushToolKind::Mcp => "Crush · tool renderer · MCP",
            CrushToolKind::Diagnostics => "Crush · tool renderer · diagnostics/LSP",
            CrushToolKind::Todos => "Crush · tool renderer · todos",
            CrushToolKind::Generic => "Crush · tool renderer · generic",
        }
    }

    fn hint(&self) -> &str {
        "r: estado pendiente/corriendo/permiso/error/ok · e: salida plegada/expandida"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [header, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        let (icon, state_style) = match self.status {
            ToolStatus::Pending | ToolStatus::Running => ("◌", theme.styles.accent),
            ToolStatus::AwaitingPermission => ("!", theme.styles.warning),
            ToolStatus::Success => ("●", theme.styles.success),
            ToolStatus::Error => ("×", theme.styles.error),
            ToolStatus::Canceled => ("×", theme.styles.muted),
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{icon} "), state_style),
                Span::styled(
                    self.name(),
                    theme.styles.tool_name.add_modifier(Modifier::BOLD),
                ),
                Span::styled("  · src/session.rs", theme.styles.muted),
            ])),
            header,
        );
        let mut rows = match self.status {
            ToolStatus::Pending | ToolStatus::Running => vec![Line::from(Span::styled(
                "Waiting for tool response…",
                state_style,
            ))],
            ToolStatus::AwaitingPermission => vec![Line::from(Span::styled(
                "Requesting permission…",
                state_style,
            ))],
            ToolStatus::Error => vec![Line::from(Span::styled(
                "Tool returned an error. Press space to inspect details.",
                state_style,
            ))],
            ToolStatus::Canceled => vec![Line::from(Span::styled("Canceled.", state_style))],
            ToolStatus::Success => self.content(theme),
        };
        if !self.expanded && rows.len() > 2 {
            rows.truncate(2);
            rows.push(Line::from(Span::styled(
                "… output collapsed",
                theme.styles.muted,
            )));
        }
        frame.render_widget(Paragraph::new(rows), body);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('r') => {
                self.status = match self.status {
                    ToolStatus::Pending => ToolStatus::Running,
                    ToolStatus::Running => ToolStatus::AwaitingPermission,
                    ToolStatus::AwaitingPermission => ToolStatus::Success,
                    ToolStatus::Success => ToolStatus::Error,
                    ToolStatus::Error | ToolStatus::Canceled => ToolStatus::Pending,
                };
            }
            KeyCode::Char('e') => self.expanded = !self.expanded,
            _ => return false,
        }
        true
    }
}
