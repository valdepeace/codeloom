//! Demo de los spinners de una celda y de las dos barras de progreso.

use crate::demo::Demo;
use batten::widgets::spinner::{ARC, BARS, BOUNCE, BRAILLE, DOT_RING, PULSE};
use batten::widgets::{progress, progress_smooth, Anim, Theme};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const STYLES: [(&str, &[&str]); 6] = [
    ("braille", BRAILLE),
    ("dot_ring", DOT_RING),
    ("arc", ARC),
    ("bounce", BOUNCE),
    ("pulse", PULSE),
    ("bars", BARS),
];

pub struct SpinnersDemo {
    anim: Anim,
}

impl SpinnersDemo {
    pub fn new() -> Self {
        Self { anim: Anim::new() }
    }
}

impl Demo for SpinnersDemo {
    fn title(&self) -> &str {
        "spinners"
    }

    fn hint(&self) -> &str {
        "estilos de una celda y barras"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = STYLES
            .iter()
            .map(|(name, frames)| {
                Line::from(vec![
                    self.anim.glyph(frames, theme),
                    Span::raw(format!(" {name}")),
                ])
            })
            .collect();
        let ratio = ((self.anim.frame / 2) % 101) as f32 / 100.0;
        lines.push(Line::default());
        lines.push(progress(ratio, area.width, theme));
        lines.push(progress_smooth(ratio, area.width, theme));
        f.render_widget(Paragraph::new(lines), area);
    }
}
