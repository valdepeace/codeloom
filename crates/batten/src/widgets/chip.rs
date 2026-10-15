//! Chip: etiqueta corta con el color del tono de fondo (` ERROR `).

use super::gradient::color_of;
use super::theme::Theme;
use super::tone::tone_style;
use crate::state::Tone;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// `" text "` con fondo del color del tono y texto oscuro en negrita.
pub fn chip(text: &str, tone: Tone, theme: &Theme) -> Span<'static> {
    let bg = color_of(tone_style(tone, theme));
    Span::styled(
        format!(" {text} "),
        Style::default()
            .bg(bg)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_text_and_uses_tone_as_background() {
        let th = Theme::test();
        let c = chip("ERROR", Tone::Error, &th);
        assert_eq!(c.content, " ERROR ");
        assert_eq!(c.style.bg, Some(Color::Red));
        assert_eq!(c.style.fg, Some(Color::Black));
    }
}
