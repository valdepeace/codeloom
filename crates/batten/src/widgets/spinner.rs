//! Animación scrambles+gradiente para estados de "girando".

use crate::state::StyleToken;
use crate::widgets::Theme;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use std::time::Duration;

/// Intervalo entre frames. El bucle de eventos de la app lo usa para tickear.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(50);

const RUNES: &[u8] = b"0123456789abcdef#@$%&*+~";

/// Cualquier item que pueda estar "girando" y avanzar un frame.
pub trait Animatable {
    /// Devuelve `true` si hay una animación activa que consume CPU.
    fn spinning(&self) -> bool;

    /// Avanza un frame; devuelve `true` si sigue girando (o sea, si hay que
    /// re-armar el reloj).
    fn advance(&mut self) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Anim {
    pub frame: u64,
}

impl Anim {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn spinning(&self) -> bool {
        true // nunca se detiene sin ser consumido externamente
    }

    pub fn advance(&mut self) -> bool {
        self.frame = self.frame.wrapping_add(1);
        true
    }

    /// "Scramble" determinista del texto (mismo frame → mismo resultado) con
    /// degradado del color de acento a lo largo de la linea. Los espacios se
    /// conservan para no mover el layout.
    pub fn render(&self, text: &str, theme: &Theme) -> Line<'static> {
        let from = color_of(theme.style(StyleToken::Accent));
        let to = color_of(theme.style(StyleToken::ToolRunning));
        let n = text.chars().count().max(1);
        let spans: Vec<Span> = text
            .chars()
            .enumerate()
            .map(|(i, c)| {
                let t = if n <= 1 {
                    0.0
                } else {
                    i as f32 / (n - 1) as f32
                };
                let color = lerp_color(from, to, t);
                let shown = if c == ' ' {
                    ' '
                } else {
                    RUNES[((self.frame as usize * 7) + i) % RUNES.len()] as char
                };
                Span::styled(shown.to_string(), Style::default().fg(color))
            })
            .collect();
        Line::from(spans)
    }
}

fn color_of(style: Style) -> Color {
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

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let (ar, ag, ab) = to_rgb(a);
    let (br, bg, bb) = to_rgb(b);
    Color::Rgb(lerp(ar, br, t), lerp(ag, bg, t), lerp(ab, bb, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Theme;

    #[test]
    fn render_is_deterministic_for_a_frame() {
        let th = Theme::test();
        let a = Anim { frame: 3 }.render("view hello.txt", &th);
        let b = Anim { frame: 3 }.render("view hello.txt", &th);
        assert_eq!(a, b);
    }

    #[test]
    fn advance_changes_frame_and_sets_spinning() {
        let mut a = Anim::new();
        let _ = a.render("x", &Theme::test());
        assert!(a.spinning());
        let before = a.frame;
        let more = a.advance();
        assert_eq!(a.frame, before + 1);
        assert!(more);
    }

    #[test]
    fn render_keeps_spaces_so_layout_is_stable() {
        let line = Anim::new().render("ab cd", &Theme::test());
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert_eq!(text.chars().filter(|c| *c == ' ').count(), 1);
        assert_eq!(text.chars().count(), 5);
    }
}
