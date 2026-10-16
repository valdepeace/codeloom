//! Helpers de color compartidos por los widgets con degradado (spinner, logo).

use super::theme::Theme;
use ratatui::style::{Color, Style};

/// Color de primer plano del estilo, con un fallback si no tiene.
pub(crate) fn color_of(style: Style) -> Color {
    style.fg.unwrap_or(Color::Cyan)
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round() as u8
}

fn to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Cyan => (0, 200, 255),
        Color::Green => (0, 200, 120),
        _ => (200, 200, 200),
    }
}

/// Interpola entre dos colores; `t` en [0, 1].
pub(crate) fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    Color::Rgb(lerp(ar, br, t), lerp(ag, bg, t), lerp(ab, bb, t))
}

/// Estilo con primer plano `color` adaptado a la profundidad del tema
/// (sin color si el terminal no lo admite).
pub(crate) fn fg(theme: &Theme, color: Color) -> Style {
    match theme.adapt(color) {
        Some(c) => Style::default().fg(c),
        None => Style::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_color_hits_both_endpoints() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(10, 20, 30);
        assert_eq!(lerp_color(a, b, 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(lerp_color(a, b, 1.0), Color::Rgb(10, 20, 30));
    }

    #[test]
    fn color_of_falls_back_to_cyan() {
        assert_eq!(color_of(Style::default()), Color::Cyan);
    }
}
