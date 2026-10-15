//! Demo de enlaces OSC 8 (clic con ctrl/cmd en terminales compatibles).

use crate::demo::Demo;
use batten::widgets::link::{file_url, link};
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;
use std::path::Path;

pub struct LinksDemo;

impl LinksDemo {
    pub fn new() -> Self {
        Self
    }
}

impl Demo for LinksDemo {
    fn title(&self) -> &str {
        "links"
    }

    fn hint(&self) -> &str {
        "ctrl+clic abre el enlace"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let rows = [
            ("https://ratatui.rs", "https://ratatui.rs".to_string()),
            ("Cargo.toml", file_url(Path::new("Cargo.toml"))),
            (
                "crates/batten/src/lib.rs",
                file_url(Path::new("crates/batten/src/lib.rs")),
            ),
        ];
        for (i, (label, url)) in rows.iter().enumerate() {
            let y = i as u16;
            if y >= area.height {
                break;
            }
            let row = Rect {
                x: area.x,
                y: area.y + y,
                width: area.width,
                height: 1,
            };
            f.render_widget(link(label, url, theme), row);
        }
    }
}
