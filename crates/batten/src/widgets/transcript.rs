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

pub fn render(
    frame: &mut Frame,
    area: Rect,
    transcript: &Transcript,
    viewport: &crate::state::Viewport,
    theme: &Theme,
) {
    let rows = area.height as usize;
    let range = viewport.window(transcript.len(), rows);
    let mut lines: Vec<TLine> = Vec::new();
    for i in range {
        if let Some(line) = transcript.line(i) {
            lines.extend(render_line(line, theme));
        }
    }

    // Barra a la derecha cuando hay más líneas que filas visibles.
    let overflow = transcript.len() > rows && area.width >= 2;
    let text_area = if overflow {
        Rect {
            width: area.width - 1,
            ..area
        }
    } else {
        area
    };
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);
    if overflow {
        let bar = Rect {
            x: area.x + area.width - 1,
            width: 1,
            ..area
        };
        crate::widgets::scrollbar::render(
            frame,
            bar,
            viewport.scroll(transcript.len(), rows),
            theme,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Line, LineKind, Transcript, Viewport};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(transcript: &Transcript, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        let vp = Viewport::new();
        term.draw(|f| render(f, f.area(), transcript, &vp, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
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
}
