//! Composer de Mindfork basado en el campo de `chat-dark-en.png`.

use crate::demo::Demo;
use crate::prompt_keys::{apply, Outcome};
use batten::state::{Composer, History};
use batten::widgets::{composer, Theme, ThemePreset};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MindforkComposerDemo {
    composer: Composer,
    history: History,
    sent: Vec<String>,
}

impl MindforkComposerDemo {
    pub fn new() -> Self {
        let mut composer = Composer::new();
        composer.set_text("And speculative decoding — is a 1B draft worth it on this card?");
        Self {
            composer,
            history: History::new(),
            sent: Vec::new(),
        }
    }
}

impl Demo for MindforkComposerDemo {
    fn title(&self) -> &str {
        "Mindfork · composer"
    }

    fn hint(&self) -> &str {
        "escribe · Ctrl+J salto · Enter envía · ↑↓ historial/cursor"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Mindfork);
        // Keep the model line above the transcript; the reference screenshot
        // places the composer and status bar at the very bottom.
        let composer_height = area.height.min(3);
        let [header, transcript, input, status] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(composer_height),
            Constraint::Length(1),
        ])
        .areas(area);
        f.render_widget(
            Paragraph::new(Line::from("◆ Gemma 4 on a 12 GB GPU")).style(
                Style::default()
                    .fg(Color::Rgb(201, 204, 210))
                    .bg(Color::Rgb(15, 17, 21)),
            ),
            header,
        );
        let messages = self
            .sent
            .iter()
            .map(|message| {
                Line::from(vec![
                    Span::styled(
                        "› YOU  ",
                        Style::default()
                            .fg(Color::Rgb(121, 169, 219))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        message.clone(),
                        Style::default().fg(Color::Rgb(201, 204, 210)),
                    ),
                ])
            })
            .collect::<Vec<_>>();
        f.render_widget(
            Paragraph::new(messages).style(Style::default().bg(Color::Rgb(15, 17, 21))),
            transcript,
        );
        composer::render_mindfork(f, input, &self.composer, true, &theme);
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("● chat  ● emb", Style::default().fg(Color::Rgb(111, 192, 130))),
                Span::styled("   Ctrl+W mouse: select   F1 help   Esc chats   Ctrl+N new   Ctrl+P settings   Ctrl+Q quit", Style::default().fg(Color::Rgb(133, 139, 146))),
            ]))
            .style(Style::default().bg(Color::Rgb(23, 25, 30))),
            status,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        self.handle_key_event(code, KeyModifiers::NONE)
    }

    fn handle_key_event(&mut self, code: KeyCode, mods: KeyModifiers) -> bool {
        match apply(&mut self.composer, &mut self.history, code, mods) {
            Outcome::Submitted(text) => {
                self.sent.push(text);
                true
            }
            Outcome::Edited => true,
            Outcome::Ignored => false,
        }
    }
}
