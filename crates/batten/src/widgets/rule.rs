//! Regla horizontal, opcionalmente con título: `Archivos ──────`.

use super::text::truncate;
use super::theme::Theme;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Título (`heading`) + espacio + `─` (`muted`) hasta `width`.
pub fn rule(title: Option<&str>, width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let Some(title) = title else {
        return Line::from(Span::styled("─".repeat(w), s.muted));
    };
    let tw = title.width();
    if tw + 1 >= w {
        return Line::from(Span::styled(truncate(title, w), s.heading));
    }
    Line::from(vec![
        Span::styled(title.to_string(), s.heading),
        Span::raw(" "),
        Span::styled("─".repeat(w - tw - 1), s.muted),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_rule_fills_width() {
        assert_eq!(text(&rule(None, 5, &Theme::test())), "─────");
    }

    #[test]
    fn titled_rule_fills_rest() {
        assert_eq!(
            text(&rule(Some("Archivos"), 12, &Theme::test())),
            "Archivos ───"
        );
    }

    #[test]
    fn wide_chars_count_by_display_width() {
        let l = rule(Some("日本"), 6, &Theme::test());
        assert_eq!(text(&l), "日本 ─");
        assert_eq!(text(&l).width(), 6);
    }

    #[test]
    fn long_title_is_truncated_without_rule() {
        assert_eq!(text(&rule(Some("Archivos"), 4, &Theme::test())), "Arc…");
    }

    #[test]
    fn zero_width_is_empty() {
        assert_eq!(text(&rule(Some("x"), 0, &Theme::test())), "");
    }
}
