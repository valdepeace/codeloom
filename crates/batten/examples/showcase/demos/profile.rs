//! Demo de rebaja de colores: el mismo tema en los cuatro perfiles.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::profile::TermProfile;
use batten::widgets::{chip, status, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ProfileDemo;

impl ProfileDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for ProfileDemo {
    fn title(&self) -> &str {
        "profile"
    }

    fn hint(&self) -> &str {
        "truecolor · 256 · 16 · sin color"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let profiles = [
            TermProfile::TrueColor,
            TermProfile::Ansi256,
            TermProfile::Ansi16,
            TermProfile::NoColor,
        ];
        let cols = Layout::horizontal([Constraint::Ratio(1, 4); 4]).split(area);
        for (p, col) in profiles.into_iter().zip(cols.iter()) {
            let label = format!("{p:?}");
            let th = theme.for_profile(p);
            let mut lines = vec![Line::from(label)];
            for tone in Tone::ALL {
                lines.push(Line::from(chip("TAG", tone, &th)));
                lines.push(status(tone, "estado", "detalle", col.width, &th));
            }
            f.render_widget(Paragraph::new(lines), *col);
        }
    }
}
