//! Demo del spinner (scramble + degradado).

use crate::demo::Demo;
use batten::widgets::{Anim, Theme};
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct SpinnerDemo {
    label: String,
    anim: Anim,
}

impl SpinnerDemo {
    pub fn new() -> Self {
        Self {
            label: "loading widgets".into(),
            anim: Anim::new(),
        }
    }
}

impl Demo for SpinnerDemo {
    fn title(&self) -> &str {
        "spinner"
    }

    fn hint(&self) -> &str {
        "animación automática"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from("scramble + degradado, avanza solo:")];
        lines.push(self.anim.render(&self.label, theme));
        f.render_widget(Paragraph::new(lines), area);
    }
}
