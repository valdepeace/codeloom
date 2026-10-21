//! Líneas del transcript que representan una herramienta (`ToolItem`).

use super::{Line, LineKind, SpanSpec, StyleToken, ToolItem};
use unicode_segmentation::UnicodeSegmentation;

/// Líneas de salida visibles mientras la herramienta no está expandida.
const FOLDED_LINES: usize = 2;

/// Longitud máxima del resumen en grafemas, incluido el `…` final.
pub(crate) const SUMMARY_GRAPHEMES: usize = 80;

/// Cabecera (nombre y resumen), cuerpo con la salida y, si procede, la
/// indicación de líneas ocultas. Los prefijos de cada línea los pinta el
/// widget y no forman parte de su texto.
pub fn tool_lines(item: &ToolItem) -> Vec<Line> {
    let mut head = Line::new(LineKind::ToolHead(item.status()), item.name.as_str());
    let name_len = item.name.len();
    if name_len > 0 {
        head.spans.push(SpanSpec {
            start: 0,
            len: name_len,
            token: StyleToken::ToolName,
        });
    }
    let summary = item
        .summary
        .as_deref()
        .and_then(|s| s.lines().next())
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if let Some(summary) = summary {
        let summary = truncate_graphemes(summary, SUMMARY_GRAPHEMES);
        let start = head.text.len() + "  ".len();
        head.text.push_str("  ");
        head.text.push_str(&summary);
        head.spans.push(SpanSpec {
            start,
            len: summary.len(),
            token: StyleToken::Muted,
        });
    }

    let mut lines = vec![head];
    lines.extend(body_lines(item));
    lines
}

/// Cuerpo de una herramienta: su salida, plegada a `FOLDED_LINES` salvo que esté
/// expandida, con la indicación de líneas ocultas si procede. Lo comparten las
/// piezas propias (`tool_atom`) para que el cuerpo sea el mismo en todas.
pub(crate) fn body_lines(item: &ToolItem) -> Vec<Line> {
    let mut lines = Vec::new();
    if let Some(output) = item.output.as_deref() {
        let body: Vec<&str> = output.lines().collect();
        let shown = if item.expanded || body.len() <= FOLDED_LINES {
            body.len()
        } else {
            FOLDED_LINES
        };
        lines.extend(
            body[..shown]
                .iter()
                .map(|l| Line::new(LineKind::ToolBody, *l)),
        );
        if shown < body.len() {
            lines.push(more_line(body.len() - shown));
        }
    }
    lines
}

/// Indicación de `hidden` líneas ocultas bajo una herramienta. La comparten el
/// cuerpo plegado y el diff de `edit` para que el aviso sea el mismo.
pub(crate) fn more_line(hidden: usize) -> Line {
    Line::new(
        LineKind::ToolMore,
        format!("… ({hidden} lines hidden) [Ctrl+O to expand]"),
    )
}

/// Recorta `text` a `max` grafemas, terminando en `…` si se corta.
pub(crate) fn truncate_graphemes(text: &str, max: usize) -> String {
    if text.graphemes(true).count() <= max {
        return text.to_string();
    }
    let mut out: String = text.graphemes(true).take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LineKind, StyleToken, ToolItem, ToolStatus};

    #[test]
    fn header_only_while_there_is_no_output() {
        let mut t = ToolItem::new("c1", "View");
        t.set_summary("src/lib.rs");
        t.start();
        let lines = tool_lines(&t);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Running));
        assert_eq!(lines[0].plain(), "View  src/lib.rs");
        assert_eq!(lines[0].spans[0].token, StyleToken::ToolName);
        assert_eq!((lines[0].spans[0].start, lines[0].spans[0].len), (0, 4));
        assert_eq!(lines[0].spans[1].token, StyleToken::Muted);
    }

    #[test]
    fn long_output_is_folded_to_two_lines_plus_a_hint() {
        let mut t = ToolItem::new("c1", "Bash");
        t.set_result(false);
        t.set_output("1\n2\n3\n4\n5");
        let lines = tool_lines(&t);
        let kinds: Vec<_> = lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::ToolHead(ToolStatus::Success),
                LineKind::ToolBody,
                LineKind::ToolBody,
                LineKind::ToolMore
            ]
        );
        assert_eq!(lines[3].plain(), "… (3 lines hidden) [Ctrl+O to expand]");
    }

    #[test]
    fn expanded_shows_everything() {
        let mut t = ToolItem::new("c1", "Bash");
        t.set_result(true);
        t.set_output("1\n2\n3\n4");
        t.toggle_expanded();
        let lines = tool_lines(&t);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Error));
        assert!(lines.iter().all(|l| l.kind != LineKind::ToolMore));
    }

    #[test]
    fn short_output_has_no_hint_and_empty_output_has_no_body() {
        let mut t = ToolItem::new("c1", "Bash");
        t.set_output("a\nb");
        assert_eq!(tool_lines(&t).len(), 3);
        t.set_output("");
        assert_eq!(tool_lines(&t).len(), 1);
    }

    #[test]
    fn summary_uses_its_first_line_and_is_truncated() {
        let mut t = ToolItem::new("c1", "Edit");
        t.set_summary(format!("{}\nsegunda", "x".repeat(200)));
        let head = &tool_lines(&t)[0];
        assert!(!head.plain().contains("segunda"));
        assert!(head.plain().ends_with('…'));
        assert!(head.plain().chars().count() <= "Edit  ".len() + 80);
    }
}
