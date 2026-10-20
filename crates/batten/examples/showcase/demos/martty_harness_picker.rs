//! Selector de harnesses de Martty.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

const HARNESS: [(&str, &str); 7] = [
    ("VT Code", "configured · /Users/demo/.martty/bin/vtcode acp"),
    ("dsh-acp", "configured · /Users/demo/.nvm/bin/dsh-acp"),
    (
        "Codex Harness",
        "configured · npx @agentclientprotocol/codex-acp",
    ),
    (
        "Bundled DeepSeek Harness",
        "configured · martty bundled runtime",
    ),
    (
        "Claude Agent",
        "configured · npx @agentclientprotocol/claude-agent-acp",
    ),
    ("Amp", "configured · /Users/demo/.martty/bin/amp-acp"),
    ("Devin", "configured · /Users/demo/.martty/bin/devin acp"),
];

pub struct MarttyHarnessPickerDemo {
    selected: usize,
}

impl MarttyHarnessPickerDemo {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Demo for MarttyHarnessPickerDemo {
    fn title(&self) -> &str {
        "Martty · harness picker"
    }
    fn hint(&self) -> &str {
        "↑/↓ navegar · /harness · referencia assets/screenshots/harness-switch.png"
    }
    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        batten::widgets::harness_picker::render_martty(f, area, &HARNESS, self.selected);
    }
    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(HARNESS.len() - 1),
            KeyCode::Enter | KeyCode::Esc => {}
            _ => return false,
        }
        true
    }
}
