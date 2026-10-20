//! Variantes estáticas de los diálogos más visibles de Crush.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Clone, Copy)]
pub enum CrushDialogKind {
    Commands,
    Files,
    Sessions,
    Models,
    Permission,
    Question,
    Providers,
    Mcp,
    Router,
    Themes,
    Notifications,
    PlanHandoff,
    Quit,
    ApiKey,
    OAuth,
    McpAuth,
    ThemeEditor,
    Arguments,
}

pub struct CrushDialogDemo {
    kind: CrushDialogKind,
    selected: usize,
}

impl CrushDialogDemo {
    pub fn new(kind: CrushDialogKind) -> Self {
        Self { kind, selected: 0 }
    }

    fn title_text(&self) -> &'static str {
        match self.kind {
            CrushDialogKind::Commands => "Commands",
            CrushDialogKind::Files => "Open file",
            CrushDialogKind::Sessions => "Sessions",
            CrushDialogKind::Models => "Select model",
            CrushDialogKind::Permission => "Permission required",
            CrushDialogKind::Question => "Question",
            CrushDialogKind::Providers => "Connect provider",
            CrushDialogKind::Mcp => "MCP servers",
            CrushDialogKind::Router => "Model router",
            CrushDialogKind::Themes => "Themes",
            CrushDialogKind::Notifications => "Notifications",
            CrushDialogKind::PlanHandoff => "Plan ready",
            CrushDialogKind::Quit => "Quit Crush?",
            CrushDialogKind::ApiKey => "Add API key",
            CrushDialogKind::OAuth => "Connect account",
            CrushDialogKind::McpAuth => "Authenticate MCP server",
            CrushDialogKind::ThemeEditor => "Edit theme",
            CrushDialogKind::Arguments => "Tool arguments",
        }
    }

    fn entries(&self) -> &'static [&'static str] {
        match self.kind {
            CrushDialogKind::Commands => &[
                "/init     Initialize this project",
                "/model    Change model",
                "/theme    Choose a theme",
                "/quit     Exit Crush",
            ],
            CrushDialogKind::Files => &["src/", "  app.rs", "  session.rs", "  tui/"],
            CrushDialogKind::Sessions => &[
                "Refactor auth middleware",
                "Fix retry handling",
                "Explore the codebase",
                "+ New session",
            ],
            CrushDialogKind::Models => &[
                "Claude Sonnet 4.5  ·  Anthropic",
                "Claude Haiku 4.5   ·  Anthropic",
                "GPT-5              ·  OpenAI",
                "Qwen Coder         ·  local",
            ],
            CrushDialogKind::Question => &[
                "Reuse the existing session format",
                "Add a migration step",
                "Keep compatibility only for new sessions",
            ],
            CrushDialogKind::Permission => &[
                "tool  Bash",
                "command  cargo test -p codeloom-core",
                "working directory  ~/src/loom",
                "",
                "Allow   ·   Allow for session   ·   Deny",
            ],
            CrushDialogKind::Providers => &[
                "Anthropic  ·  connected",
                "OpenAI",
                "OpenRouter",
                "GitHub Copilot",
                "Add API key",
            ],
            CrushDialogKind::Mcp => &[
                "github       ● connected",
                "filesystem   ● connected",
                "local-tools  ○ disabled",
                "Configure server…",
            ],
            CrushDialogKind::Router => &[
                "Enabled  ·  yes",
                "Default model  ·  Claude Sonnet 4.5",
                "Fallback  ·  Claude Haiku 4.5",
                "Edit model pool…",
            ],
            CrushDialogKind::Themes => &[
                "charmtone-panther  ·  current",
                "gruvbox-dark",
                "Create theme…",
                "Edit current theme…",
            ],
            CrushDialogKind::Notifications => &[
                "Automatic  ·  current",
                "Native",
                "OSC 99",
                "Terminal bell",
                "Disabled",
            ],
            CrushDialogKind::PlanHandoff => &[
                "Review the plan before starting the work.",
                "",
                "Start implementation",
                "Revise plan",
                "Cancel",
            ],
            CrushDialogKind::Quit => &[
                "The current session will be saved.",
                "",
                "Quit",
                "Keep working",
            ],
            CrushDialogKind::ApiKey => &[
                "Provider  ·  Anthropic",
                "API key  ·  sk-ant-••••••••••••",
                "Paste a key, then press Enter",
            ],
            CrushDialogKind::OAuth => &[
                "Open the browser to authorize Crush.",
                "Provider  ·  OpenAI",
                "Waiting for authorization…",
                "Esc to cancel",
            ],
            CrushDialogKind::McpAuth => &[
                "Server  ·  github",
                "Waiting for OAuth callback…",
                "Scopes  ·  repo, read:user",
            ],
            CrushDialogKind::ThemeEditor => &[
                "accent       #6b50ff",
                "success      #00ffb2",
                "warning      #f5ef34",
                "error        #eb4268",
                "Save changes   ·   Cancel",
            ],
            CrushDialogKind::Arguments => &[
                "file_path  ·  src/session.rs",
                "offset     ·  40",
                "limit      ·  20",
                "Enter to submit   ·   Esc to cancel",
            ],
        }
    }
}

impl Demo for CrushDialogDemo {
    fn title(&self) -> &str {
        match self.kind {
            CrushDialogKind::Commands => "Crush · commands dialog",
            CrushDialogKind::Files => "Crush · file picker",
            CrushDialogKind::Sessions => "Crush · sessions dialog",
            CrushDialogKind::Models => "Crush · model picker",
            CrushDialogKind::Permission => "Crush · permission dialog",
            CrushDialogKind::Question => "Crush · question dialog",
            CrushDialogKind::Providers => "Crush · provider dialog",
            CrushDialogKind::Mcp => "Crush · MCP dialog",
            CrushDialogKind::Router => "Crush · router dialog",
            CrushDialogKind::Themes => "Crush · theme dialog",
            CrushDialogKind::Notifications => "Crush · notifications dialog",
            CrushDialogKind::PlanHandoff => "Crush · plan handoff",
            CrushDialogKind::Quit => "Crush · quit confirmation",
            CrushDialogKind::ApiKey => "Crush · API key input",
            CrushDialogKind::OAuth => "Crush · OAuth flow",
            CrushDialogKind::McpAuth => "Crush · MCP authentication",
            CrushDialogKind::ThemeEditor => "Crush · theme editor",
            CrushDialogKind::Arguments => "Crush · arguments dialog",
        }
    }

    fn hint(&self) -> &str {
        "↑↓: mover selección · overlay redondeado; demo visual sin acciones conectadas"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.width < 8 || area.height < 4 {
            return;
        }
        let width = area.width.saturating_sub(4).min(72);
        let height = area.height.saturating_sub(4).min(18);
        let popup = Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::default()
            .title(Span::styled(
                format!(" {} ", self.title_text()),
                theme.styles.accent.add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(theme.styles.border)
            .style(theme.styles.default);
        let inner = block.inner(popup);
        frame.render_widget(block, popup);

        let entries = self.entries();
        let mut lines = Vec::new();
        if matches!(self.kind, CrushDialogKind::Permission) {
            lines.extend(
                entries
                    .iter()
                    .take(3)
                    .map(|s| Line::from(Span::styled(*s, theme.styles.muted))),
            );
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![
                Span::styled(" Allow ", theme.styles.success.add_modifier(Modifier::BOLD)),
                Span::styled("  Allow for session  ", theme.styles.accent),
                Span::styled(" Deny ", theme.styles.error),
            ]));
        } else {
            for (index, entry) in entries.iter().enumerate() {
                let selected = index == self.selected % entries.len();
                let style = if selected {
                    Style::default()
                        .fg(theme.styles.default.bg.unwrap_or_default())
                        .bg(theme.styles.accent.fg.unwrap_or_default())
                        .add_modifier(Modifier::BOLD)
                } else {
                    theme.styles.default
                };
                lines.push(Line::from(Span::styled(
                    format!("{} {entry}", if selected { "›" } else { " " }),
                    style,
                )));
            }
        }
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        let count = self.entries().len();
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1) % count,
            _ => return false,
        }
        true
    }
}
