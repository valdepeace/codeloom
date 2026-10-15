//! Demo de mensajes de estado, uno por tono.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{status, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct StatusDemo;

impl StatusDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for StatusDemo {
    fn title(&self) -> &str {
        "status"
    }

    fn hint(&self) -> &str {
        "un mensaje por tono"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = [
            ("Listo", "esperando instrucciones"),
            ("Conectado", "deepseek via openrouter"),
            ("Guardado", "3 archivos"),
            ("Contexto al 85%", "considera compactar"),
            ("Falló el build", "error[E0425] en main.rs"),
        ];
        let lines: Vec<_> = Tone::ALL
            .into_iter()
            .zip(rows)
            .map(|(tone, (title, desc))| status(tone, title, desc, area.width, theme))
            .collect();
        f.render_widget(Paragraph::new(lines), area);
    }
}
