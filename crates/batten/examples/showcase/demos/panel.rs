//! Demo de paneles, uno por tono.

use crate::demo::Demo;
use batten::state::Tone;
use batten::widgets::{render_panel, Panel, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct PanelDemo;

impl PanelDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for PanelDemo {
    fn title(&self) -> &str {
        "panel"
    }

    fn hint(&self) -> &str {
        "un panel por tono"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = Layout::vertical([Constraint::Length(3); 5]).split(area);
        for (tone, row) in Tone::ALL.into_iter().zip(rows.iter()) {
            let panel = Panel {
                title: "Permisos",
                info: "bash",
                tone,
            };
            let inner = render_panel(f, *row, &panel, theme);
            f.render_widget(Paragraph::new(format!("{tone:?}")), inner);
        }
    }
}
