//! Demo de reglas: sin título, con título y con título ancho.

use crate::demo::Demo;
use batten::widgets::{rule, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct RuleDemo;

impl RuleDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for RuleDemo {
    fn title(&self) -> &str {
        "rule"
    }

    fn hint(&self) -> &str {
        "reglas al ancho del área"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let w = area.width;
        let lines = vec![
            rule(None, w, theme),
            rule(Some("Archivos"), w, theme),
            rule(Some("日本語のタイトル"), w, theme),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
