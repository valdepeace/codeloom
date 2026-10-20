//! Glyph, wordmark y lockup de Mindfork en fondos oscuro y claro.

use crate::demo::Demo;
use batten::widgets::{logo, Theme};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MindforkLogoDemo;

impl Demo for MindforkLogoDemo {
    fn title(&self) -> &str {
        "Mindfork · logos"
    }

    fn hint(&self) -> &str {
        "glyph 16×16, wordmark pixelado y lockup · dark/light"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let [dark, light] =
            Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
        for (row, background, mind) in [
            (dark, Color::Rgb(15, 17, 21), Color::Rgb(228, 228, 231)),
            (light, Color::Rgb(245, 245, 245), Color::Rgb(39, 39, 42)),
        ] {
            let cells: [Rect; 3] = Layout::horizontal([
                Constraint::Percentage(33),
                Constraint::Percentage(34),
                Constraint::Percentage(33),
            ])
            .areas(row);
            for (cell, label, lines) in [
                (cells[0], "Glyph", logo::mindfork_glyph()),
                (cells[1], "Wordmark", logo::mindfork_wordmark(mind)),
                (cells[2], "Lockup", logo::mindfork_lockup(mind)),
            ] {
                let content = std::iter::once(Line::from(label))
                    .chain(lines)
                    .collect::<Vec<_>>();
                frame.render_widget(
                    Paragraph::new(content)
                        .alignment(ratatui::layout::Alignment::Center)
                        .style(Style::default().bg(background)),
                    cell,
                );
            }
        }
    }
}
