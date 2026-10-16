//! Vista del transcript: ventana visible + scroll + wraps CJK-aware.

use crate::state::{Line, LineKind, StyleToken, Transcript};
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line as TLine, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

fn prefix(kind: LineKind) -> &'static str {
    match kind {
        LineKind::User => "you: ",
        LineKind::Assistant => "loom: ",
        LineKind::System => "sys: ",
        LineKind::Thinking => "… ",
        LineKind::ToolCall | LineKind::ToolResult => "⚙ ",
        LineKind::Todo => "☐ ",
        LineKind::Plan => "plan: ",
        LineKind::Banner => "",
        LineKind::Error => "! ",
    }
}

fn kind_token(kind: LineKind) -> StyleToken {
    match kind {
        LineKind::User => StyleToken::User,
        LineKind::Assistant => StyleToken::Assistant,
        LineKind::System => StyleToken::System,
        LineKind::Thinking => StyleToken::Muted,
        LineKind::ToolCall | LineKind::ToolResult => StyleToken::ToolName,
        LineKind::Todo => StyleToken::Heading,
        LineKind::Plan => StyleToken::Heading,
        LineKind::Banner => StyleToken::Accent,
        LineKind::Error => StyleToken::Error,
    }
}

/// Convierte una `Line` de estado en lineas ratatui, aplicando los `span` por
/// offsets sobre el texto plano (los huecos usan el token del kind).
fn render_line(line: &Line, theme: &Theme) -> Vec<TLine<'static>> {
    let base = theme.style(kind_token(line.kind));
    let prefix_text = prefix(line.kind);
    let text = line.plain();
    let mut spans: Vec<Span> = vec![Span::styled(prefix_text.to_string(), base)];

    if line.spans.is_empty() {
        spans.push(Span::styled(text.to_string(), base));
        return vec![TLine::from(spans)];
    }

    let mut spans_sorted = line.spans.clone();
    spans_sorted.sort_by_key(|s| s.start);
    let mut cursor = 0usize;
    for span in spans_sorted {
        let start = span.start.min(text.len());
        let end = (span.start + span.len).min(text.len());
        if start < cursor {
            continue;
        }
        if cursor < start {
            spans.push(Span::styled(text[cursor..start].to_string(), base));
        }
        spans.push(Span::styled(
            text[start..end].to_string(),
            theme.style(span.token),
        ));
        cursor = end;
    }
    if cursor < text.len() {
        spans.push(Span::styled(text[cursor..].to_string(), base));
    }
    vec![TLine::from(spans)]
}

/// Pinta el transcript sin selección.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut crate::state::Viewport,
    theme: &Theme,
) {
    render_with_selection(frame, area, transcript, viewport, None, theme);
}

/// Pinta el transcript. Una sola geometría (filas tras el wrap) alimenta el
/// pintado, el scroll y la scrollbar; la fija en `viewport`. Las líneas de
/// `selection` se resaltan y su extremo se mantiene visible.
pub fn render_with_selection(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &mut crate::state::Viewport,
    selection: Option<&crate::state::Selection>,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = area.height as usize;
    let sel_style = theme.styles.selection;
    // limitación: se compone todo el historial en cada frame; cachear por línea
    // si se nota en conversaciones largas.
    let lines: Vec<TLine> = (0..transcript.len())
        .filter_map(|i| transcript.line(i).map(|l| (i, l)))
        .flat_map(|(i, l)| {
            let mut out = render_line(l, theme);
            if selection.is_some_and(|s| s.contains(i)) {
                for tl in &mut out {
                    tl.style = tl.style.patch(sel_style);
                }
            }
            out
        })
        .collect();
    let wrap = Wrap { trim: false };
    let para = Paragraph::new(lines.clone()).wrap(wrap);
    let full = para.line_count(area.width);
    let overflow = full > rows && area.width >= 2;
    let text_area = if overflow {
        Rect {
            width: area.width - 1,
            ..area
        }
    } else {
        area
    };
    let total = if overflow {
        para.line_count(text_area.width)
    } else {
        full
    };
    viewport.set_geometry(total, rows);
    if let (Some(s), false) = (selection, lines.is_empty()) {
        let head = s.head().min(lines.len() - 1);
        let w = text_area.width;
        let start = Paragraph::new(lines[..head].to_vec())
            .wrap(wrap)
            .line_count(w);
        let height = Paragraph::new(vec![lines[head].clone()])
            .wrap(wrap)
            .line_count(w);
        viewport.ensure_visible(start, start + height);
    }
    // limitación: `Paragraph::scroll` usa u16; más de 65535 filas se quedan en el tope.
    let top = u16::try_from(viewport.top()).unwrap_or(u16::MAX);
    frame.render_widget(para.scroll((top, 0)), text_area);
    if overflow {
        let bar = Rect {
            x: area.x + area.width - 1,
            width: 1,
            ..area
        };
        crate::widgets::scrollbar::render(frame, bar, viewport.scroll(), theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Line, LineKind, Transcript, Viewport};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn draw(transcript: &Transcript, vp: &mut Viewport, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), transcript, vp, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn buffer(transcript: &Transcript, w: u16, h: u16) -> String {
        draw(transcript, &mut Viewport::new(), w, h)
    }

    #[test]
    fn shows_user_and_assistant_prefixes() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::User, "hola"));
        t.push(Line::new(LineKind::Assistant, "buenas"));
        let out = buffer(&t, 40, 10);
        assert!(out.contains("you:"));
        assert!(out.contains("hola"));
        assert!(out.contains("buenas"));
    }

    #[test]
    fn empty_transcript_does_not_panic() {
        let _ = buffer(&Transcript::new(), 40, 10);
    }

    #[test]
    fn zero_and_tiny_sizes_do_not_panic() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "x"));
        let _ = buffer(&t, 0, 0);
        let _ = buffer(&t, 1, 1);
    }

    #[test]
    fn wraps_a_long_unbroken_token_without_panicking() {
        let mut t = Transcript::new();
        let url = "https://example.com/".to_string() + &"a".repeat(300);
        t.push(Line::new(LineKind::Assistant, url));
        let out = buffer(&t, 20, 5);
        // No se desborda: no hay panico y el buffer tiene la altura pedida.
        assert_eq!(out.chars().count(), 20 * 5);
    }

    #[test]
    fn truncates_on_display_width_not_bytes() {
        // CJK ocupa 2 columnas cada uno; ASCII 1.
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "日本語 テキスト 🚀 hola"));
        let _ = buffer(&t, 12, 4); // no debe entrar en panico por anchura
    }

    #[test]
    fn shows_only_the_viewport_window() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i}")));
        }
        let out = buffer(&t, 40, 5); // 5 filas -> ventana 45..50
        assert!(out.contains("line 49"));
        assert!(!out.contains("line 0"));
    }

    #[test]
    fn shows_a_scrollbar_when_content_overflows() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i}")));
        }
        let out = buffer(&t, 20, 5);
        assert!(
            out.contains('\u{2551}') || out.contains('\u{2588}'),
            "esperaba una barra de scroll"
        );
    }

    #[test]
    fn no_scrollbar_when_content_fits() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "hi"));
        let out = buffer(&t, 20, 5);
        assert!(!out.contains('\u{2551}') && !out.contains('\u{2588}'));
    }

    #[test]
    fn wrapped_lines_count_as_rows_when_following() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::User, "inicio"));
        t.push(Line::new(LineKind::Assistant, "palabra ".repeat(20)));
        t.push(Line::new(LineKind::Assistant, "fin"));
        let out = buffer(&t, 20, 3);
        assert!(out.contains("fin"), "la última línea debe verse");
        assert!(!out.contains("inicio"));
    }

    #[test]
    fn reading_up_keeps_the_view_while_content_grows() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut vp = Viewport::new();
        draw(&t, &mut vp, 30, 5);
        vp.scroll_up(10);
        let before = draw(&t, &mut vp, 30, 5);
        for i in 50..70 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let after = draw(&t, &mut vp, 30, 5);
        assert!(before.contains("line 35"));
        assert!(after.contains("line 35"));
        assert!(!after.contains("line 69"));
    }

    #[test]
    fn selected_lines_are_highlighted() {
        use crate::state::Selection;
        let mut t = Transcript::new();
        for s in ["uno", "dos", "tres"] {
            t.push(Line::new(LineKind::Assistant, s));
        }
        let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
        let mut vp = Viewport::new();
        let sel = Selection::at(1);
        term.draw(|f| render_with_selection(f, f.area(), &t, &mut vp, Some(&sel), &Theme::test()))
            .unwrap();
        let buf = term.backend().buffer();
        assert_eq!(buf[(0, 1)].bg, ratatui::style::Color::Indexed(238));
        assert_ne!(buf[(0, 0)].bg, ratatui::style::Color::Indexed(238));
    }

    #[test]
    fn the_selection_head_is_scrolled_into_view() {
        use crate::state::Selection;
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut term = Terminal::new(TestBackend::new(30, 5)).unwrap();
        let mut vp = Viewport::new();
        let sel = Selection::at(0);
        term.draw(|f| render_with_selection(f, f.area(), &t, &mut vp, Some(&sel), &Theme::test()))
            .unwrap();
        let out: String = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(out.contains("line 00"));
        assert!(!vp.is_following());
    }

    #[test]
    fn following_shows_new_content() {
        let mut t = Transcript::new();
        for i in 0..50 {
            t.push(Line::new(LineKind::Assistant, format!("line {i:02}")));
        }
        let mut vp = Viewport::new();
        draw(&t, &mut vp, 30, 5);
        t.push(Line::new(LineKind::Assistant, "nueva"));
        assert!(draw(&t, &mut vp, 30, 5).contains("nueva"));
    }
}
