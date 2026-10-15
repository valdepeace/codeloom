//! Demo de chips: uno por tono.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{chip, Theme};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ChipDemo;

impl ChipDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for ChipDemo {
    fn title(&self) -> &str {
        "chip"
    }

    fn hint(&self) -> &str {
        "un chip por tono"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let labels = ["NOTE", "INFO", "DONE", "WARN", "ERROR"];
        let mut spans = Vec::new();
        for (tone, label) in Tone::ALL.into_iter().zip(labels) {
            spans.push(chip(label, tone, theme));
            spans.push(Span::raw(" "));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
