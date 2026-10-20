//! Primera reconstrucción de `assets/screenshots/chat-dark-en.png`; ajustar con captura del showcase.

use crate::demo::Demo;
use batten::widgets::{Theme, ThemePreset};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

const BG: Color = Color::Rgb(15, 17, 21);
const FG: Color = Color::Rgb(201, 204, 210);
const MUTED: Color = Color::Rgb(133, 139, 146);
const GREEN: Color = Color::Rgb(111, 192, 130);
const BLUE: Color = Color::Rgb(121, 169, 219);
const GOLD: Color = Color::Rgb(220, 175, 97);
const BORDER: Color = Color::Rgb(54, 58, 66);

pub struct MindforkReference {
    thoughts_open: bool,
    tool_open: bool,
}

impl MindforkReference {
    pub fn new() -> Self {
        Self {
            thoughts_open: true,
            tool_open: false,
        }
    }

    fn feed(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::styled("│  › YOU", Style::default().fg(BLUE).add_modifier(Modifier::BOLD)),
            Line::styled("│  Give me the bottom line — a small table plus a decision diagram, and save a note about my setup.", Style::default().fg(FG)),
            Line::raw("│"),
            Line::styled("│  ✦ ASSISTANT", Style::default().fg(GREEN).add_modifier(Modifier::BOLD)),
        ];
        lines.push(Line::styled(
            format!("│  {} thinking", if self.thoughts_open { "▾" } else { "▸" }),
            Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
        ));
        if self.thoughts_open {
            lines.push(Line::styled("│  │ The note should record the 12 GB budget and the Q5_K_M choice, so future sizing questions start from the hardware.", Style::default().fg(MUTED).add_modifier(Modifier::ITALIC)));
        }
        lines.extend([
            Line::styled("│  The short version:", Style::default().fg(FG)),
            Line::raw("│"),
            Line::styled("│   ┌────────┬────────────────┬─────────────────┐", Style::default().fg(MUTED)),
            Line::from(vec![Span::styled("│   │ Quant  │ 16k context?   │ Verdict         │", Style::default().fg(GOLD).add_modifier(Modifier::BOLD))]),
            Line::styled("│   ├────────┼────────────────┼─────────────────┤", Style::default().fg(MUTED)),
            Line::styled("│   │ Q5_K_M │ fits, ~1.5 GB  │ sweet spot      │", Style::default().fg(FG)),
            Line::styled("│   │ Q6_K   │ tight          │ quality first   │", Style::default().fg(FG)),
            Line::styled("│   └────────┴────────────────┴─────────────────┘", Style::default().fg(MUTED)),
            Line::raw("│"),
            Line::styled("│                  ╱──────────────────╲", Style::default().fg(MUTED)),
            Line::styled("│                  │ How much context? │", Style::default().fg(FG)),
            Line::styled("│                  ╲───────┬──────────╱", Style::default().fg(MUTED)),
            Line::styled("│             ┌────────────┼────────────┐", Style::default().fg(MUTED)),
            Line::styled("│            8k           16k           32k", Style::default().fg(FG)),
            Line::styled("│      ┌──────────────┐ ┌──────────────┐ ┌──────────────┐", Style::default().fg(MUTED)),
            Line::styled("│      │ Q6_K quality │ │ Q5_K_M sweet │ │ Q4_K_M headroom│", Style::default().fg(FG)),
            Line::styled("│      └──────────────┘ └──────────────┘ └──────────────┘", Style::default().fg(MUTED)),
            Line::raw("│"),
            Line::styled("│  Note saved. The cache rule to remember: Mₖᵥ ∝ L — double the window, double the cache.", Style::default().fg(FG)),
            Line::raw("│"),
            Line::from(vec![
                Span::styled("│  ⚒  ", Style::default().fg(GOLD)),
                Span::styled("note_save", Style::default().fg(GOLD).add_modifier(Modifier::BOLD)),
            ]),
            Line::styled("│  │  content: The user's GPU has 12 GB; Gemma 4 12B at Q5_K_M with a 16k context fits it.", Style::default().fg(MUTED)),
        ]);
        if self.tool_open {
            lines.push(Line::styled(
                "│  │  result: Note saved (id=6d665f64-656d-6f5f-7465-000000000001).",
                Style::default().fg(MUTED),
            ));
        } else {
            lines.push(Line::styled(
                "│  │  Note saved (id=6d665f64-656d-6f5f-7465-000000000001).",
                Style::default().fg(MUTED),
            ));
        }
        lines
    }
}

impl Demo for MindforkReference {
    fn title(&self) -> &str {
        "Mindfork · chat"
    }
    fn hint(&self) -> &str {
        "t: thoughts · x: tool details · Esc: índice"
    }
    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Mindfork);
        f.render_widget(Block::default().style(Style::default().bg(BG)), area);
        if area.width < 12 || area.height < 5 {
            return;
        }
        let rows = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(4),
            Constraint::Length(1),
        ])
        .split(area);
        let feed_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" ◆ Gemma 4 on a 12 GB GPU ")
            .style(Style::default().bg(BG));
        f.render_widget(
            Paragraph::new(Text::from(self.feed()))
                .wrap(Wrap { trim: false })
                .block(feed_block),
            rows[0],
        );

        let input = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER))
            .title(" input · Enter send · Shift+Enter newline ")
            .style(Style::default().bg(BG));
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("› ", theme.style(batten::state::StyleToken::Success)),
                Span::styled(
                    "And speculative decoding — is a 1B draft worth it on this card?",
                    Style::default().fg(FG),
                ),
            ]))
            .block(input),
            rows[1],
        );

        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled("● chat  ● emb", Style::default().fg(GREEN)),
            Span::styled("   Ctrl+W mouse: select   F1 help   Esc chats   Ctrl+N new   Ctrl+P settings   Ctrl+Q quit", Style::default().fg(MUTED)),
        ])).style(Style::default().bg(Color::Rgb(23, 25, 30))), rows[2]);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('t') => self.thoughts_open = !self.thoughts_open,
            KeyCode::Char('x') => self.tool_open = !self.tool_open,
            _ => return false,
        }
        true
    }
}
