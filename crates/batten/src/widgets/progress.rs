//! Barra de progreso determinada: `━━━━━━━━────────  42%`.

use super::gradient::{color_of, lerp_color};
use super::text::truncate;
use super::theme::Theme;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

/// Relleno `━` con degradado `accent_from → accent_to`, resto `─` y porcentaje
/// en 5 columnas. `ratio` se acota a `[0, 1]`; NaN cuenta como 0.
pub fn progress(ratio: f32, width: u16, theme: &Theme) -> Line<'static> {
    let w = width as usize;
    if w == 0 {
        return Line::default();
    }
    let s = &theme.styles;
    let r = if ratio.is_nan() {
        0.0
    } else {
        ratio.clamp(0.0, 1.0)
    };
    let pct = (r * 100.0).round() as u32;
    if w < 6 {
        return Line::from(Span::styled(truncate(&format!("{pct}%"), w), s.muted));
    }
    let bar = w - 5;
    let filled = (r * bar as f32).round() as usize;
    let from = color_of(s.accent_from);
    let to = color_of(s.accent_to);
    let mut spans: Vec<Span<'static>> = (0..filled)
        .map(|i| {
            let t = if bar > 1 {
                i as f32 / (bar - 1) as f32
            } else {
                0.0
            };
            Span::styled("━", Style::default().fg(lerp_color(from, to, t)))
        })
        .collect();
    spans.push(Span::styled("─".repeat(bar - filled), s.muted));
    spans.push(Span::styled(format!(" {pct:>3}%"), s.muted));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn partial_bar() {
        let l = progress(0.42, 25, &Theme::test());
        assert_eq!(
            text(&l),
            format!("{}{}  42%", "━".repeat(8), "─".repeat(12))
        );
        assert_eq!(text(&l).width(), 25);
    }

    #[test]
    fn first_cell_uses_accent_from() {
        let th = Theme::test();
        let l = progress(1.0, 10, &th);
        assert_eq!(l.spans[0].style.fg, Some(color_of(th.styles.accent_from)));
    }

    #[test]
    fn clamps_and_handles_nan() {
        let th = Theme::test();
        assert_eq!(text(&progress(1.5, 10, &th)), "━━━━━ 100%");
        assert_eq!(text(&progress(f32::NAN, 10, &th)), "─────   0%");
        assert_eq!(text(&progress(-3.0, 10, &th)), "─────   0%");
    }

    #[test]
    fn narrow_shows_only_percent() {
        assert_eq!(text(&progress(0.5, 4, &Theme::test())), "50%");
        assert_eq!(text(&progress(0.5, 0, &Theme::test())), "");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..30u16 {
            assert!(text(&progress(0.37, w, &Theme::test())).width() <= w as usize);
        }
    }
}
