//! Composer de Martty basado en las capturas `ui_start.png` y `agent-turn.png`.

use crate::demo::Demo;
use crate::prompt_keys::{apply, Outcome};
use batten::state::{Composer, History};
use batten::widgets::{composer, Theme, ThemePreset};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MarttyComposerDemo {
    composer: Composer,
    history: History,
    sent: Vec<String>,
}

impl MarttyComposerDemo {
    pub fn new() -> Self {
        Self {
            composer: Composer::new(),
            history: History::new(),
            sent: Vec::new(),
        }
    }
}

impl Demo for MarttyComposerDemo {
    fn title(&self) -> &str {
        "Martty · composer"
    }

    fn hint(&self) -> &str {
        "escribe · Ctrl+J salto · Enter envía · ↑↓ historial/cursor"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Martty);
        let composer_height = area.height.min(6);
        let [transcript, input] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(composer_height)]).areas(area);
        let lines = self
            .sent
            .iter()
            .map(|message| {
                Line::styled(
                    format!("❯ {message}"),
                    theme
                        .style(batten::state::StyleToken::Default)
                        .add_modifier(Modifier::BOLD),
                )
            })
            .collect::<Vec<_>>();
        f.render_widget(Paragraph::new(lines), transcript);
        composer::render_martty(f, input, &self.composer, true, &theme);
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
