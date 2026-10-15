//! Vista del aviso visible: `[ OKAY! ] Copiado al portapapeles`.

use super::chip::chip;
use super::text::truncate;
use super::theme::Theme;
use super::tone::tone_label;
use crate::state::Toasts;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Chip con la etiqueta del tono + texto, truncado a `width`. `None` si no
/// hay aviso.
pub fn render(toasts: &Toasts, width: u16, theme: &Theme) -> Option<Line<'static>> {
    let t = toasts.current()?;
    let w = width as usize;
    if w == 0 {
        return Some(Line::default());
    }
    let s = &theme.styles;
    let c = chip(tone_label(t.tone), t.tone, theme);
    let cw = c.content.width();
    if w <= cw + 1 {
        return Some(Line::from(Span::styled(truncate(&t.text, w), s.default)));
    }
    Some(Line::from(vec![
        c,
        Span::raw(" "),
        Span::styled(truncate(&t.text, w - cw - 1), s.default),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Tone;
    use ratatui::style::Color;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn shows_chip_and_text() {
        let mut t = Toasts::new();
        t.push(Tone::Success, "Copiado", 10);
        let l = render(&t, 40, &Theme::test()).unwrap();
        assert_eq!(text(&l), " OKAY!  Copiado");
        assert_eq!(l.spans[0].style.bg, Some(Color::Green));
    }

    #[test]
    fn truncates_text_to_width() {
        let mut t = Toasts::new();
        t.push(Tone::Success, "Copiado", 10);
        assert_eq!(text(&render(&t, 10, &Theme::test()).unwrap()), " OKAY!  C…");
    }

    #[test]
    fn none_without_toast() {
        assert!(render(&Toasts::new(), 40, &Theme::test()).is_none());
    }

    #[test]
    fn never_exceeds_width() {
        let mut t = Toasts::new();
        t.push(Tone::Warning, "Algo largo que no cabe", 10);
        for w in 0..30u16 {
            let l = render(&t, w, &Theme::test()).unwrap();
            assert!(text(&l).width() <= w as usize);
        }
    }
}
