//! Miniatura de adjunto y tarjeta desde `image-preview.png` de Martty.

use crate::demo::Demo;
use batten::widgets::{attachment_preview, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

pub struct MarttyAttachmentPreviewDemo {
    pixels: Vec<Color>,
    width: u32,
    height: u32,
}

impl MarttyAttachmentPreviewDemo {
    pub fn new() -> Self {
        let image =
            image::load_from_memory(include_bytes!("../assets/martty-attachment-thumb.png"))
                .expect("the checked-in Martty preview crop is valid PNG")
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

impl Demo for MarttyAttachmentPreviewDemo {
    fn title(&self) -> &str {
        "Martty · attachment preview"
    }
    fn hint(&self) -> &str {
        "miniatura del screenshot de referencia · pixel cells en lugar de Kitty protocol"
    }
    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        frame.render_widget(
            Block::default().style(ratatui::style::Style::default().bg(Color::Rgb(14, 14, 18))),
            area,
        );
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "[image 1]",
                    ratatui::style::Style::default().fg(Color::Rgb(79, 125, 255)),
                )),
                Line::raw("composer draft · hover the chip or place the caret on it"),
                Line::raw("the preview card sits above the composer"),
            ]),
            Rect::new(area.x + 3, area.y + 2, area.width.saturating_sub(6), 3),
        );
        let card = Rect::new(
            area.x + 3,
            area.bottom().saturating_sub(12),
            38.min(area.width.saturating_sub(6)),
            10.min(area.height.saturating_sub(2)),
        );
        attachment_preview::render_martty_preview(
            frame,
            card,
            "image 1",
            "clipboard.png",
            "1420×1668 px",
            "457 KB",
            "image/png",
            Some(&self.pixels),
            self.width,
            self.height,
        );
    }
    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
