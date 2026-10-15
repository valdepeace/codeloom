//! Animación scrambles+gradiente para estados de "girando".

use crate::state::StyleToken;
use crate::widgets::Theme;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::time::Duration;

use super::gradient::{color_of, lerp_color};

/// Intervalo entre frames. El bucle de eventos de la app lo usa para tickear.
pub const FRAME_INTERVAL: Duration = Duration::from_millis(50);

const RUNES: &[u8] = b"0123456789abcdef#@$%&*+~";

/// Spinners de una celda (frames en orden). Se pintan con `Anim::glyph`.
pub const BRAILLE: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const DOT_RING: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
pub const ARC: &[&str] = &["◜", "◠", "◝", "◞", "◡", "◟"];
pub const BOUNCE: &[&str] = &["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"];
pub const PULSE: &[&str] = &["·", "•", "●", "•"];
pub const BARS: &[&str] = &[
    "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
];

/// Ticks del reloj (50 ms) por frame de un spinner de una celda.
const TICKS_PER_GLYPH: u64 = 2;

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

    /// Frame actual de un spinner de una celda. El color oscila entre
    /// `accent_from` y `accent_to` con un periodo de ~2 s. Sin frames, vacío.
    pub fn glyph(&self, frames: &[&'static str], theme: &Theme) -> Span<'static> {
        if frames.is_empty() {
            return Span::raw("");
        }
        let step = self.frame / TICKS_PER_GLYPH;
        let shown = frames[(step % frames.len() as u64) as usize];
        let phase = (step % 20) as f32 / 10.0;
        let t = if phase <= 1.0 { phase } else { 2.0 - phase };
        let from = color_of(theme.styles.accent_from);
        let to = color_of(theme.styles.accent_to);
        Span::styled(shown, Style::default().fg(lerp_color(from, to, t)))
    }
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

#[cfg(test)]
mod glyph_tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn advances_every_two_ticks_and_wraps() {
        let th = Theme::test();
        assert_eq!(Anim { frame: 0 }.glyph(BRAILLE, &th).content, "⠋");
        assert_eq!(Anim { frame: 1 }.glyph(BRAILLE, &th).content, "⠋");
        assert_eq!(Anim { frame: 2 }.glyph(BRAILLE, &th).content, "⠙");
        assert_eq!(Anim { frame: 20 }.glyph(BRAILLE, &th).content, "⠋");
    }

    #[test]
    fn starts_with_accent_from() {
        let th = Theme::test();
        let g = Anim { frame: 0 }.glyph(PULSE, &th);
        assert_eq!(g.style.fg, Some(color_of(th.styles.accent_from)));
    }

    #[test]
    fn empty_frames_are_empty() {
        assert_eq!(Anim::new().glyph(&[], &Theme::test()).content, "");
    }

    #[test]
    fn every_frame_is_one_column() {
        for frames in [BRAILLE, DOT_RING, ARC, BOUNCE, PULSE, BARS] {
            for f in frames {
                assert_eq!(f.width(), 1, "frame {f:?}");
            }
        }
    }
}
