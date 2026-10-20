//! Selector visual de modelos de Martty, basado en `ui_modelsel.png`.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

const MODELS: [(&str, &str); 3] = [
    ("deepseek-v4-flash", "DeepSeek-V4-Flash"),
    ("deepseek-v4-pro", "DeepSeek-V4-Pro"),
    (
        "deepseek-v4-flash-vision-exp",
        "DeepSeek-V4-Flash-Vision-Exp",
    ),
];

pub struct MarttyModelPickerDemo {
    selected: usize,
}

impl MarttyModelPickerDemo {
    pub fn new() -> Self {
        Self { selected: 0 }
    }
}

impl Demo for MarttyModelPickerDemo {
    fn title(&self) -> &str {
        "Martty · model picker"
    }

    fn hint(&self) -> &str {
        "↑/↓ seleccionar · Enter confirmar · basado en website/public/ui_modelsel.png"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        batten::widgets::model_picker::render_martty(
            f,
            area,
            "model · enter select · esc close",
            &MODELS,
            self.selected,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(MODELS.len() - 1),
            KeyCode::Enter => {}
            _ => return false,
        }
        true
    }
}
