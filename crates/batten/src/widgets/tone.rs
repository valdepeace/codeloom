//! Resolución de `Tone` a estilo, icono y etiqueta según el tema.

use super::gradient::color_of;
use super::theme::Theme;
use crate::state::Tone;
use ratatui::style::{Color, Modifier, Style};

/// Estilo del tema asociado al tono.
pub fn tone_style(tone: Tone, theme: &Theme) -> Style {
    let s = &theme.styles;
    match tone {
        Tone::Neutral => s.muted,
        Tone::Info => s.accent,
        Tone::Success => s.success,
        Tone::Warning => s.warning,
        Tone::Error => s.error,
    }
}

/// Relleno de chip/botón: fondo del color del tono con texto oscuro; sin
/// color, vídeo inverso para que se siga distinguiendo.
pub(crate) fn tone_fill(tone: Tone, theme: &Theme) -> Style {
    match theme.adapt(color_of(tone_style(tone, theme))) {
        Some(bg) => Style::default()
            .bg(bg)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
        None => Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD),
    }
}

/// Icono de una columna para `status`.
pub fn tone_icon(tone: Tone) -> char {
    match tone {
        Tone::Neutral => '•',
        Tone::Info => '●',
        Tone::Success => '✓',
        Tone::Warning => '!',
        Tone::Error => '×',
    }
}

/// Etiqueta corta para el chip de `toast`.
pub fn tone_label(tone: Tone) -> &'static str {
    match tone {
        Tone::Neutral => "NOTE",
        Tone::Info => "HEY!",
        Tone::Success => "OKAY!",
        Tone::Warning => "WARNING",
        Tone::Error => "ERROR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_maps_to_theme_tokens() {
        let th = Theme::test();
        assert_eq!(tone_style(Tone::Error, &th), th.styles.error);
        assert_eq!(tone_style(Tone::Neutral, &th), th.styles.muted);
        assert_eq!(tone_style(Tone::Info, &th), th.styles.accent);
    }

    #[test]
    fn icons_and_labels_are_distinct() {
        let mut icons: Vec<char> = Tone::ALL.iter().map(|t| tone_icon(*t)).collect();
        icons.dedup();
        assert_eq!(icons.len(), 5);
        assert_eq!(tone_label(Tone::Success), "OKAY!");
    }
}
