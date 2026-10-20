//! Explorador de harnesses de Martty con filtro del catálogo.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

const INSTALLED: [(&str, &str); 4] = [
    ("VT Code (current)", "configured · ~/.martty/bin/vtcode acp"),
    ("dsh-acp", "configured · ~/.nvm/bin/dsh-acp"),
    (
        "Codex Harness",
        "configured · npx @agentclientprotocol/codex-acp",
    ),
    (
        "Cursor",
        "found locally · configure · ~/.local/bin/cursor-agent acp",
    ),
];
const AVAILABLE: [(&str, &str); 7] = [
    (
        "Agoragentic",
        "available via npx · first launch may download packages",
    ),
    (
        "Auggie CLI",
        "available via npx · first launch may download packages",
    ),
    (
        "Autohand Code",
        "available via npx · first launch may download packages",
    ),
    (
        "Cline",
        "available via npx · first launch may download packages",
    ),
    (
        "Codebuddy Code",
        "available via npx · first launch may download packages",
    ),
    (
        "Codex",
        "available via npx · first launch may download packages",
    ),
    (
        "DeepAgents",
        "available via npx · first launch may download packages",
    ),
];

pub struct MarttyHarnessAddDemo {
    query: String,
    selected: usize,
}

impl MarttyHarnessAddDemo {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            selected: 0,
        }
    }
    fn count(&self) -> usize {
        INSTALLED
            .iter()
            .chain(AVAILABLE.iter())
            .filter(|(name, detail)| {
                self.query.is_empty()
                    || name.to_lowercase().contains(&self.query.to_lowercase())
                    || detail.to_lowercase().contains(&self.query.to_lowercase())
            })
            .count()
    }
}

impl Demo for MarttyHarnessAddDemo {
    fn title(&self) -> &str {
        "Martty · add harness"
    }
    fn hint(&self) -> &str {
        "escribe para filtrar · ↑/↓ navegar · screenshot harness-add.png"
    }
    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        batten::widgets::harness_picker::render_martty_add(
            frame,
            area,
            &INSTALLED,
            &AVAILABLE,
            &self.query,
            self.selected,
        );
    }
    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.count().saturating_sub(1))
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Char(c) if !c.is_control() => {
                self.query.push(c);
                self.selected = 0;
            }
            KeyCode::Enter | KeyCode::Esc => {}
            _ => return false,
        }
        true
    }
}
