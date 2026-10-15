//! Mensaje de estado de una línea: `✓ Guardado  3 archivos`.

use super::text::truncate;
use super::theme::Theme;
use super::tone::{tone_icon, tone_style};
use crate::state::Tone;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Icono del tono + título + descripción atenuada. Se recorta primero la
/// descripción; si no cabe, se omite; luego se trunca el título.
pub fn status(tone: Tone, title: &str, desc: &str, width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let icon = Span::styled(tone_icon(tone).to_string(), tone_style(tone, theme));
    if w <= 2 {
        return Line::from(icon);
    }
    let avail = w - 2;
    let mut spans = vec![icon, Span::raw(" ")];
    let tw = title.width();
    if tw >= avail || desc.is_empty() {
        spans.push(Span::styled(truncate(title, avail), s.default));
        return Line::from(spans);
    }
    spans.push(Span::styled(title.to_string(), s.default));
    let rest = avail - tw;
    if rest >= 3 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(truncate(desc, rest - 2), s.muted));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn full_line() {
        let th = Theme::test();
        let l = status(Tone::Success, "Guardado", "3 archivos", 40, &th);
        assert_eq!(text(&l), "✓ Guardado  3 archivos");
        assert_eq!(l.spans[0].style, th.styles.success);
    }

    #[test]
    fn truncates_desc_first_then_drops_it() {
        let th = Theme::test();
        assert_eq!(
            text(&status(Tone::Success, "Guardado", "3 archivos", 15, &th)),
            "✓ Guardado  3 …"
        );
        assert_eq!(
            text(&status(Tone::Success, "Guardado", "3 archivos", 11, &th)),
            "✓ Guardado"
        );
    }

    #[test]
    fn truncates_title_last() {
        assert_eq!(
            text(&status(Tone::Error, "Guardado", "x", 6, &Theme::test())),
            "× Gua…"
        );
    }

    #[test]
    fn empty_desc_has_no_separator() {
        assert_eq!(
            text(&status(Tone::Info, "Hola", "", 40, &Theme::test())),
            "● Hola"
        );
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..30u16 {
            let l = status(Tone::Warning, "Guardado", "3 archivos", w, &Theme::test());
            assert!(text(&l).width() <= w as usize);
        }
    }
}
