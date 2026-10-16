//! Chip: etiqueta corta con el color del tono de fondo (` ERROR `).

use super::theme::Theme;
use super::tone::tone_fill;
use crate::state::Tone;
use ratatui::text::Span;

/// `" text "` con fondo del color del tono y texto oscuro en negrita.
pub fn chip(text: &str, tone: Tone, theme: &Theme) -> Span<'static> {
    Span::styled(format!(" {text} "), tone_fill(tone, theme))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn pads_text_and_uses_tone_as_background() {
        let th = Theme::test();
        let c = chip("ERROR", Tone::Error, &th);
        assert_eq!(c.content, " ERROR ");
        assert_eq!(c.style.bg, Some(Color::Red));
        assert_eq!(c.style.fg, Some(Color::Black));
    }

    #[test]
    fn no_color_chip_uses_reverse_video() {
        use ratatui::style::Modifier;
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::NoColor;
        let c = chip("ERROR", Tone::Error, &th);
        assert_eq!(c.style.bg, None);
        assert!(c.style.add_modifier.contains(Modifier::REVERSED));
    }
}
