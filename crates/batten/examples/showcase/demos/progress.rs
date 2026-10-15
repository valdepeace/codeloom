//! Demo de la barra de progreso, animada por frames.

use crate::demo::Demo;
use batten::widgets::{progress, Anim, Theme};
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ProgressDemo {
    frame: u32,
}

impl ProgressDemo {
    pub fn new() -> Self {
        Self { frame: 0 }
    }
}

impl Demo for ProgressDemo {
    fn title(&self) -> &str {
        "progress"
    }

    fn hint(&self) -> &str {
        "avanza sola"
    }

    fn tick(&mut self, _anim: &Anim) {
        self.frame = self.frame.wrapping_add(1);
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let ratio = (self.frame % 101) as f32 / 100.0;
        let lines = vec![
            progress(ratio, area.width, theme),
            progress(0.0, area.width, theme),
            progress(1.0, area.width, theme),
        ];
        f.render_widget(Paragraph::new(lines), area);
    }
}
