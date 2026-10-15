//! Fila de ayuda de teclas: `enter send · ↑↓ history · ctrl+c quit`.

use super::theme::Theme;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Pinta `key desc` separados por ` · `. Si no cabe, quita items del final
/// y añade ` …`; si ni el primero cabe, solo `…`.
pub fn key_help(items: &[(&str, &str)], width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if items.is_empty() || w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let total = |n: usize| -> usize {
        let body: usize = items[..n]
            .iter()
            .map(|(k, d)| k.width() + 1 + d.width())
            .sum();
        body + 3 * n.saturating_sub(1)
    };
    let fits = |n: usize| total(n) + if n < items.len() { 2 } else { 0 } <= w;
    let Some(n) = (1..=items.len()).rev().find(|&n| fits(n)) else {
        return Line::from(Span::styled("…", s.dim));
    };
    let mut spans = Vec::new();
    for (i, (k, d)) in items[..n].iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", s.dim));
        }
        spans.push(Span::styled(k.to_string(), s.heading));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(d.to_string(), s.muted));
    }
    if n < items.len() {
        spans.push(Span::styled(" …", s.dim));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: [(&str, &str); 3] = [("enter", "send"), ("↑↓", "history"), ("ctrl+c", "quit")];

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn everything_fits() {
        assert_eq!(
            text(&key_help(&ITEMS, 40, &Theme::test())),
            "enter send · ↑↓ history · ctrl+c quit"
        );
    }

    #[test]
    fn drops_items_from_the_end() {
        assert_eq!(
            text(&key_help(&ITEMS, 25, &Theme::test())),
            "enter send · ↑↓ history …"
        );
        assert_eq!(text(&key_help(&ITEMS, 24, &Theme::test())), "enter send …");
    }

    #[test]
    fn nothing_fits_gives_ellipsis() {
        assert_eq!(text(&key_help(&ITEMS, 5, &Theme::test())), "…");
    }

    #[test]
    fn empty_or_zero_width_is_empty() {
        assert_eq!(text(&key_help(&[], 40, &Theme::test())), "");
        assert_eq!(text(&key_help(&ITEMS, 0, &Theme::test())), "");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..45u16 {
            assert!(text(&key_help(&ITEMS, w, &Theme::test())).width() <= w as usize);
        }
    }
}
