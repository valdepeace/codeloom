//! Raster pequeño sobre las celdas RGB de un terminal.

use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::Frame;

/// Dibuja un buffer de píxeles RGB con un píxel superior/inferior por celda.
pub fn render_pixels(frame: &mut Frame, area: Rect, pixels: &[Color], width: u32, height: u32) {
    if width == 0
        || height == 0
        || pixels.len() < width as usize * height as usize
        || area.is_empty()
    {
        return;
    }
    let buffer = frame.buffer_mut();
    for row in 0..area.height {
        let top_y = (u64::from(row) * 2 * u64::from(height) / (2 * u64::from(area.height))) as u32;
        let bottom_y = (top_y + 1).min(height - 1);
        for col in 0..area.width {
            let x = (u64::from(col) * u64::from(width) / u64::from(area.width)) as u32;
            let top = pixels[(top_y * width + x) as usize];
            let bottom = pixels[(bottom_y * width + x) as usize];
            let cell = &mut buffer[(area.x + col, area.y + row)];
            cell.set_symbol("▀");
            cell.set_fg(top);
            cell.set_bg(bottom);
        }
    }
}
