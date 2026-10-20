//! Flujo visual de eliminación segura de un harness en Martty.

use crate::demo::Demo;
use batten::widgets::harness_picker::{self, RemovalStep};
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct MarttyHarnessRemoveDemo {
    step: RemovalStep,
    selected: usize,
}

impl MarttyHarnessRemoveDemo {
    pub fn new() -> Self {
        Self {
            step: RemovalStep::Harness,
            selected: 0,
        }
    }
}

impl Demo for MarttyHarnessRemoveDemo {
    fn title(&self) -> &str {
        "Martty · remove harness"
    }
    fn hint(&self) -> &str {
        "Enter avanza · ↑/↓ elige modo · Esc retrocede · solo demo visual"
    }
    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        harness_picker::render_martty_removal(frame, area, self.step, self.selected);
    }
    fn handle_key(&mut self, code: KeyCode) -> bool {
        match (self.step, code) {
            (RemovalStep::Harness, KeyCode::Enter) => {
                self.step = RemovalStep::Mode;
                self.selected = 0;
            }
            (RemovalStep::Mode, KeyCode::Up | KeyCode::Down) => self.selected = 1 - self.selected,
            (RemovalStep::Mode, KeyCode::Enter) => self.step = RemovalStep::Confirm,
            (RemovalStep::Confirm, KeyCode::Enter) => {
                self.step = RemovalStep::Harness;
                self.selected = 0;
            }
            (RemovalStep::Mode | RemovalStep::Confirm, KeyCode::Esc) => {
                self.step = RemovalStep::Mode
            }
            (RemovalStep::Harness, KeyCode::Esc) => return false,
            _ => return false,
        }
        true
    }
}
