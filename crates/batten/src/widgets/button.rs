//! Botones de texto: el enfocado lleva fondo del color del tono.

use super::theme::Theme;
use super::tone::tone_fill;
use crate::state::Tone;
use ratatui::text::{Line, Span};

/// `" label "`; enfocado → fondo del tono, texto negro en negrita; si no, `muted`.
pub fn button(label: &str, focused: bool, tone: Tone, theme: &Theme) -> Span<'static> {
    let text = format!(" {label} ");
    if focused {
        Span::styled(text, tone_fill(tone, theme))
    } else {
        Span::styled(text, theme.styles.muted)
    }
}

/// Fila de botones separados por dos espacios; `selected` es el enfocado.
pub fn button_row(items: &[(&str, Tone)], selected: usize, theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (label, tone)) in items.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(button(label, i == selected, *tone, theme));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn focused_uses_tone_background() {
        let th = Theme::test();
        let b = button("Denegar", true, Tone::Error, &th);
        assert_eq!(b.content, " Denegar ");
        assert_eq!(b.style.bg, Some(Color::Red));
        let b = button("Denegar", false, Tone::Error, &th);
        assert_eq!(b.style, th.styles.muted);
    }

    #[test]
    fn row_marks_only_the_selected() {
        let th = Theme::test();
        let items = [("Sí", Tone::Success), ("No", Tone::Error)];
        let l = button_row(&items, 1, &th);
        assert_eq!(text(&l), " Sí    No ");
        assert_eq!(l.spans[0].style, th.styles.muted);
        assert_eq!(l.spans[2].style.bg, Some(Color::Red));
    }
}
