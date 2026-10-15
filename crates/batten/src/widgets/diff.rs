//! Vista unified de un `Diff`: números viejo/nuevo, símbolo y texto coloreado.

use super::text::truncate;
use super::theme::Theme;
use crate::state::{Diff, DiffKind, DiffLine};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Pinta las líneas del diff desde `scroll` (acotado a la última línea).
pub fn render(frame: &mut Frame, area: Rect, diff: &Diff, scroll: usize, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let lines = diff.lines();
    let max_no = lines
        .iter()
        .flat_map(|l| [l.old_no, l.new_no])
        .flatten()
        .max()
        .unwrap_or(0);
    let nw = max_no.to_string().len();
    let start = scroll.min(lines.len().saturating_sub(1));
    let w = area.width as usize;
    let rows: Vec<Line<'static>> = lines
        .iter()
        .skip(start)
        .take(area.height as usize)
        .map(|l| row(l, nw, w, theme))
        .collect();
    frame.render_widget(Paragraph::new(rows), area);
}

fn row(l: &DiffLine, nw: usize, w: usize, theme: &Theme) -> Line<'static> {
    let s = &theme.styles;
    let text = l.text.replace('\t', "    ");
    if l.kind == DiffKind::Hunk {
        return Line::from(Span::styled(truncate(&text, w), s.diff_hunk));
    }
    let num = |n: Option<usize>| match n {
        Some(n) => format!("{n:>nw$}"),
        None => " ".repeat(nw),
    };
    let gutter = format!("{} {} ", num(l.old_no), num(l.new_no));
    let (sym, style) = match l.kind {
        DiffKind::Insert => ("+", s.diff_insert),
        DiffKind::Delete => ("-", s.diff_delete),
        _ => (" ", s.default),
    };
    let gw = gutter.width() + 2;
    if gw >= w {
        return Line::from(Span::styled(truncate(&gutter, w), s.dim));
    }
    Line::from(vec![
        Span::styled(gutter, s.dim),
        Span::styled(format!("{sym} "), style),
        Span::styled(truncate(&text, w - gw), style),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn draw(diff: &Diff, w: u16, h: u16, scroll: usize) -> Terminal<TestBackend> {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, f.area(), diff, scroll, &Theme::test()))
            .unwrap();
        term
    }

    fn row_text(term: &Terminal<TestBackend>, y: u16) -> String {
        let buf = term.backend().buffer();
        let s: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        s.trim_end().to_string()
    }

    fn sample() -> Diff {
        Diff::new("a\nb\nc\n", "a\nB\nc\n", 1)
    }

    #[test]
    fn gutter_symbols_and_colors() {
        let term = draw(&sample(), 20, 5, 0);
        assert_eq!(row_text(&term, 0), "@@ -1,3 +1,3 @@");
        assert_eq!(row_text(&term, 1), "1 1   a");
        assert_eq!(row_text(&term, 2), "2   - b");
        assert_eq!(row_text(&term, 3), "  2 + B");
        assert_eq!(row_text(&term, 4), "3 3   c");
        let buf = term.backend().buffer();
        assert_eq!(buf[(4, 2)].fg, Color::Red);
        assert_eq!(buf[(4, 3)].fg, Color::Green);
    }

    #[test]
    fn scroll_is_clamped_to_last_line() {
        let term = draw(&sample(), 20, 3, 99);
        assert_eq!(row_text(&term, 0), "3 3   c");
    }

    #[test]
    fn long_lines_are_truncated() {
        let d = Diff::new("abcdefghij\n", "x\n", 3);
        let term = draw(&d, 8, 3, 0);
        assert_eq!(row_text(&term, 1), "1   - a…");
    }

    #[test]
    fn tiny_and_empty_do_not_panic() {
        draw(&sample(), 0, 0, 0);
        draw(&sample(), 3, 1, 0);
        draw(&Diff::default(), 10, 3, 5);
    }
}
