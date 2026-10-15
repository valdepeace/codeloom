//! Demo de la fila de ayuda a ancho completo y a la mitad.

use crate::demo::Demo;
use batten::widgets::{key_help, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const ITEMS: [(&str, &str); 4] = [
    ("enter", "send"),
    ("ctrl+j", "newline"),
    ("↑↓", "history"),
    ("ctrl+c", "quit"),
];

pub struct KeyHelpDemo;

impl KeyHelpDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for KeyHelpDemo {
    fn title(&self) -> &str {
        "key_help"
    }

    fn hint(&self) -> &str {
        "ancho completo y mitad"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let lines = vec![
            key_help(&ITEMS, area.width, theme),
            key_help(&ITEMS, area.width / 2, theme),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
