//! Selector de modelos de Pawan basado en su snapshot de 80×24.

use crate::demo::Demo;
use batten::widgets::model_picker::{self, PawanModel};
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

const MODELS: [PawanModel<'static>; 13] = [
    PawanModel {
        provider: "Meta",
        id: "meta/llama-3.1-70b-instruct",
        quality_score: 90,
    },
    PawanModel {
        provider: "Meta",
        id: "meta/llama-3.3-70b-instruct",
        quality_score: 93,
    },
    PawanModel {
        provider: "Meta",
        id: "meta/llama-4-maverick-17b-128e-instruct",
        quality_score: 92,
    },
    PawanModel {
        provider: "Deepseek",
        id: "deepseek-ai/deepseek-v3.2",
        quality_score: 91,
    },
    PawanModel {
        provider: "Deepseek",
        id: "deepseek-ai/deepseek-v4-flash",
        quality_score: 89,
    },
    PawanModel {
        provider: "Google",
        id: "google/gemma-3-12b-it",
        quality_score: 85,
    },
    PawanModel {
        provider: "Mistral",
        id: "mistralai/mistral-large-3-675b",
        quality_score: 94,
    },
    PawanModel {
        provider: "Qwen",
        id: "qwen/qwen3.5-397b-a17b",
        quality_score: 95,
    },
    PawanModel {
        provider: "Nvidia",
        id: "nvidia/llama-3.1-nemotron-ultra",
        quality_score: 91,
    },
    PawanModel {
        provider: "Stepfun",
        id: "stepfun-ai/step-3.7-flash",
        quality_score: 88,
    },
    PawanModel {
        provider: "Stepfun",
        id: "stepfun-ai/step-3.5-flash",
        quality_score: 87,
    },
    PawanModel {
        provider: "Z-Ai",
        id: "z-ai/glm-5.1",
        quality_score: 90,
    },
    PawanModel {
        provider: "OpenAI",
        id: "openai/gpt-oss-120b",
        quality_score: 75,
    },
];

pub struct PawanModelPickerDemo {
    selected: usize,
    query: String,
    searching: bool,
}

impl PawanModelPickerDemo {
    pub fn new() -> Self {
        Self {
            selected: 0,
            query: String::new(),
            searching: false,
        }
    }
}

impl Demo for PawanModelPickerDemo {
    fn title(&self) -> &str {
        "Pawan · model picker"
    }

    fn hint(&self) -> &str {
        "↑/↓ seleccionar · / filtrar · Enter elegir · Esc cerrar"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(Style::default().bg(ratatui::style::Color::Black)),
            area,
        );
        model_picker::render_pawan(frame, area, &MODELS, self.selected, &self.query, "fallback");
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(MODELS.len() - 1),
            KeyCode::Char('/') => {
                self.searching = true;
                self.query.clear();
                self.selected = 0;
            }
            KeyCode::Esc if self.searching => {
                self.searching = false;
                self.query.clear();
                self.selected = 0;
            }
            KeyCode::Backspace if self.searching => {
                self.query.pop();
                self.selected = 0;
            }
            KeyCode::Enter if self.searching => self.searching = false,
            KeyCode::Char(ch) if self.searching => {
                self.query.push(ch);
                self.selected = 0;
            }
            _ => return false,
        }
        true
    }
}
