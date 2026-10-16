//! Barra de progreso determinada: `━━━━━━━━────────  42%`.

use super::gradient::{color_of, fg, lerp_color};
use super::text::truncate;
use super::theme::Theme;
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
            Span::styled("━", fg(theme, lerp_color(from, to, t)))
        })
        .collect();
    spans.push(Span::styled("─".repeat(bar - filled), s.muted));
    spans.push(Span::styled(format!(" {pct:>3}%"), s.muted));
    Line::from(spans)
}

/// Bloques parciales por octavos (índice = octavos ocupados en la celda).
const EIGHTHS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];

/// Como `progress`, pero con `█` y la última celda partida en octavos:
/// avanza por fracciones de celda. Hueco en blanco; porcentaje en 5 columnas.
pub fn progress_smooth(ratio: f32, width: u16, theme: &Theme) -> Line<'static> {
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
    let eighths = (r * (bar * 8) as f32).round() as usize;
    let (full, part) = (eighths / 8, eighths % 8);
    let from = color_of(s.accent_from);
    let to = color_of(s.accent_to);
    let color = |i: usize| {
        let t = if bar > 1 {
            i as f32 / (bar - 1) as f32
        } else {
            0.0
        };
        fg(theme, lerp_color(from, to, t))
    };
    let mut spans: Vec<Span<'static>> = (0..full).map(|i| Span::styled("█", color(i))).collect();
    let mut used = full;
    if part > 0 {
        spans.push(Span::styled(EIGHTHS[part], color(full)));
        used += 1;
    }
    spans.push(Span::raw(" ".repeat(bar - used)));
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

    #[test]
    fn no_color_depth_drops_the_gradient() {
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::NoColor;
        let l = progress(1.0, 10, &th);
        assert_eq!(l.spans[0].style.fg, None);
    }

    #[cfg(feature = "termprofile")]
    #[test]
    fn ansi256_depth_never_emits_rgb() {
        let mut th = Theme::test();
        th.depth = crate::widgets::ColorDepth::Ansi256;
        for s in progress_smooth(0.7, 20, &th).spans {
            assert!(!matches!(s.style.fg, Some(ratatui::style::Color::Rgb(..))));
        }
    }
}

#[cfg(test)]
mod smooth_tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn half_is_whole_cells() {
        let l = progress_smooth(0.5, 13, &Theme::test());
        assert_eq!(text(&l), "████      50%");
    }

    #[test]
    fn small_ratio_uses_partial_block() {
        // barra de 8 celdas = 64 octavos; 10 % → 6 octavos → "▊"
        let l = progress_smooth(0.1, 13, &Theme::test());
        assert_eq!(text(&l), format!("▊{}  10%", " ".repeat(7)));
    }

    #[test]
    fn full_and_nan() {
        let th = Theme::test();
        assert_eq!(text(&progress_smooth(1.5, 10, &th)), "█████ 100%");
        assert_eq!(
            text(&progress_smooth(f32::NAN, 10, &th)),
            format!("{}   0%", " ".repeat(5))
        );
    }

    #[test]
    fn narrow_shows_only_percent() {
        assert_eq!(text(&progress_smooth(0.5, 4, &Theme::test())), "50%");
        assert_eq!(text(&progress_smooth(0.5, 0, &Theme::test())), "");
    }

    #[test]
    fn never_exceeds_width() {
        for w in 0..30u16 {
            for r in [0.0, 0.013, 0.37, 0.999, 1.0] {
                let l = progress_smooth(r, w, &Theme::test());
                assert!(text(&l).width() <= w as usize, "w={w} r={r}");
            }
        }
    }
}
