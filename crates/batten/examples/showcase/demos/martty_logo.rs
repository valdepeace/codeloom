//! Ballena y wordmark de Martty, separados de la pantalla welcome.

use crate::demo::Demo;
use batten::widgets::{logo, Theme, ThemePreset};
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MarttyLogoDemo {
    pixels: Vec<Color>,
    width: u32,
    height: u32,
}

impl MarttyLogoDemo {
    pub fn new() -> Self {
        let image = image::load_from_memory(include_bytes!("../assets/martty-whale.png"))
            .expect("the checked-in Martty whale fixture is valid PNG")
            .to_rgb8();
        let (width, height) = image.dimensions();
        let pixels = image
            .pixels()
            .map(|pixel| Color::Rgb(pixel[0], pixel[1], pixel[2]))
            .collect();
        Self {
            pixels,
            width,
            height,
        }
    }
}

impl Demo for MarttyLogoDemo {
    fn title(&self) -> &str {
        "Martty · logo"
    }

    fn hint(&self) -> &str {
        "ballena pixelada + wordmark original; referencia extraída del welcome de Martty"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Martty);
        let whale_height = area.height.min(12).saturating_sub(7).max(1);
        let whale_width = ((u64::from(whale_height) * 2 * u64::from(self.width)
            + u64::from(self.height) / 2)
            / u64::from(self.height.max(1)))
        .min(u64::from(area.width)) as u16;
        let whale_x = area.x + area.width.saturating_sub(whale_width) / 2;
        logo::render_martty_whale(
            frame,
            Rect::new(whale_x, area.y, whale_width, whale_height),
            &self.pixels,
            self.width,
            self.height,
        );
        let wordmark_y = area.y.saturating_add(whale_height).saturating_add(1);
        frame.render_widget(
            Paragraph::new(logo::martty_wordmark(&theme, area.width))
                .alignment(Alignment::Center)
                .style(Style::default().bg(Color::Rgb(15, 17, 21))),
            Rect::new(area.x, wordmark_y, area.width, 6),
        );
        if area.height > wordmark_y.saturating_sub(area.y) + 6 {
            frame.render_widget(
                Paragraph::new(Line::from("https://martty.sh"))
                    .style(Style::default().fg(Color::Rgb(151, 157, 166)))
                    .alignment(Alignment::Center),
                Rect::new(area.x, wordmark_y + 6, area.width, 1),
            );
        }
    }
}
