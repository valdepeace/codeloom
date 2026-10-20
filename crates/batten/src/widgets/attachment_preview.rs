//! Preview de adjuntos de Martty con miniatura de píxeles en Ratatui.

use crate::widgets::raster::render_pixels;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Tarjeta de archivo sin miniatura, fallback para terminales sin imágenes.
pub fn render_martty_fallback(
    frame: &mut Frame,
    area: Rect,
    token: &str,
    filename: &str,
    dimensions: &str,
    size: &str,
    media_type: &str,
) {
    render_martty_preview(
        frame, area, token, filename, dimensions, size, media_type, None, 0, 0,
    );
}

/// Tarjeta de adjunto con miniatura rasterizada en media celda.
// La demo conserva juntos todos los metadatos independientes del adjunto.
#[allow(clippy::too_many_arguments)]
pub fn render_martty_preview(
    frame: &mut Frame,
    area: Rect,
    token: &str,
    filename: &str,
    dimensions: &str,
    size: &str,
    media_type: &str,
    pixels: Option<&[Color]>,
    image_width: u32,
    image_height: u32,
) {
    if area.width < 24 || area.height < 5 {
        return;
    }
    let thumbnail_rows = if pixels.is_some() && image_width > 0 && image_height > 0 {
        5u16.min(area.height.saturating_sub(5))
    } else {
        0
    };
    let muted = Style::default().fg(Color::Rgb(128, 128, 128));
    let block = Block::default()
        .title(Span::styled(
            format!(" {token} "),
            Style::default()
                .fg(Color::Rgb(79, 125, 255))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(54, 58, 66)))
        .style(Style::default().bg(Color::Rgb(28, 28, 28)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![Line::from(Span::styled(
        filename,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ))];
    lines.push(Line::from(Span::styled(
        format!("{dimensions} · {size} · {media_type}"),
        muted,
    )));
    lines.push(Line::from(Span::styled(
        "⌫ on the chip removes · enter sends",
        muted,
    )));
    frame.render_widget(
        Paragraph::new(lines),
        Rect::new(
            inner.x,
            inner.y + thumbnail_rows,
            inner.width,
            inner.height.saturating_sub(thumbnail_rows),
        ),
    );
    if let Some(pixels) = pixels {
        let thumb_width = ((u64::from(thumbnail_rows) * 2 * u64::from(image_width)
            + u64::from(image_height) / 2)
            / u64::from(image_height))
        .clamp(4, u64::from(inner.width)) as u16;
        render_pixels(
            frame,
            Rect::new(inner.x, inner.y, thumb_width, thumbnail_rows),
            pixels,
            image_width,
            image_height,
        );
    }
}
