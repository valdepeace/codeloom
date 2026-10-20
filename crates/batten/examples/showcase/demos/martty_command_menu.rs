//! Autocompletado de comandos/skills de Martty.

use crate::demo::Demo;
use batten::widgets::command_suggestions::{self, MarttyCommand};
use batten::widgets::{Theme, ThemePreset};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

const COMMANDS: [MarttyCommand<'static>; 16] = [
    MarttyCommand {
        usage: "/mode [id]",
        description: "agent mode · standard / code / mini",
        section: Some("session"),
        skill: false,
    },
    MarttyCommand {
        usage: "/effort [off|high|max]",
        description: "reasoning effort for this session",
        section: Some("session"),
        skill: false,
    },
    MarttyCommand {
        usage: "/permission [preset]",
        description: "permission preset picker · shift+tab",
        section: Some("session"),
        skill: false,
    },
    MarttyCommand {
        usage: "/plan [off]",
        description: "host plan mode (command passthrough)",
        section: Some("session"),
        skill: false,
    },
    MarttyCommand {
        usage: "/image <path> [text]",
        description: "send a local image",
        section: Some("input"),
        skill: false,
    },
    MarttyCommand {
        usage: "/clip [text]",
        description: "attach the clipboard image",
        section: Some("input"),
        skill: false,
    },
    MarttyCommand {
        usage: "/theme [dark|light]",
        description: "toggle the DeepSeek palette",
        section: Some("view"),
        skill: false,
    },
    MarttyCommand {
        usage: "/session",
        description: "show session + runtime info",
        section: Some("view"),
        skill: false,
    },
    MarttyCommand {
        usage: "/logo",
        description: "bring the whale back",
        section: Some("view"),
        skill: false,
    },
    MarttyCommand {
        usage: "/liang [on|off]",
        description: "toggle assistant pet",
        section: Some("view"),
        skill: false,
    },
    MarttyCommand {
        usage: "/quit",
        description: "exit dsh-tui",
        section: Some("session"),
        skill: false,
    },
    MarttyCommand {
        usage: "/agent-browser",
        description: "Browser automation CLI for AI agent",
        section: Some("skills"),
        skill: true,
    },
    MarttyCommand {
        usage: "/commit",
        description: "Create a focused git commit",
        section: Some("skills"),
        skill: true,
    },
    MarttyCommand {
        usage: "/review",
        description: "Review the current changes",
        section: Some("skills"),
        skill: true,
    },
    MarttyCommand {
        usage: "/test",
        description: "Run a targeted test suite",
        section: Some("skills"),
        skill: true,
    },
    MarttyCommand {
        usage: "/docs",
        description: "Search project documentation",
        section: Some("skills"),
        skill: true,
    },
];

pub struct MarttyCommandMenuDemo {
    selected: usize,
}

impl MarttyCommandMenuDemo {
    pub fn new() -> Self {
        Self { selected: 11 }
    }
}

impl Demo for MarttyCommandMenuDemo {
    fn title(&self) -> &str {
        "Martty · commands/skills"
    }

    fn hint(&self) -> &str {
        "↑/↓ navegar · Enter elegir · Esc índice"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Martty);
        frame.render_widget(
            Block::default().style(theme.style(batten::state::StyleToken::Default)),
            area,
        );
        let [chat, input, status] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(2),
            Constraint::Length(1),
        ])
        .areas(area);
        let transcript = vec![
            Line::from("ecosystem scale. Independent analyst reports consistently rank it first in the"),
            Line::from("Chinese enterprise AI market:"),
            Line::from("• Frost & Sullivan's 2025 China GenAI reports put Alibaba Cloud / Qwen at No.1"),
            Line::from("• Qwen has been named a leader for two consecutive years in competitiveness reports"),
            Line::raw(""),
            Line::from("DeepSeek — strong technical influence and efficiency. Its impact on AI economics"),
            Line::from("is arguably unmatched by any other Chinese lab."),
        ];
        frame.render_widget(
            Paragraph::new(transcript).style(theme.style(batten::state::StyleToken::Default)),
            chat,
        );
        let [popup_input] = Layout::vertical([Constraint::Length(1)]).areas(input);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("❯ ", Style::default().fg(Color::Rgb(86, 134, 254))),
                Span::styled("/", theme.style(batten::state::StyleToken::Default)),
            ]))
            .block(Block::default().style(theme.style(batten::state::StyleToken::Default))),
            popup_input,
        );
        frame.render_widget(
            Paragraph::new(
                "· Standard    Workspace Write    ask        ↵ command · deepseek-v4-pro",
            )
            .style(theme.style(batten::state::StyleToken::Muted)),
            status,
        );
        command_suggestions::render_martty(frame, input, chat, &COMMANDS, self.selected, &theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(COMMANDS.len() - 1),
            KeyCode::Enter => return true,
            _ => return false,
        }
        true
    }
}
