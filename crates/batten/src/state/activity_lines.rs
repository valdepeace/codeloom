//! Líneas del transcript que representan un elemento de actividad (un job o
//! un subagente) en el look Crush.

use super::{Line, LineKind, SpanSpec, StyleToken, ToolStatus};

/// Líneas de resultado visibles antes de plegar el resto.
const FOLDED_LINES: usize = 6;

/// Datos de un elemento de actividad, ya traducidos por quien lo construye.
#[derive(Debug, Clone, Copy)]
pub struct ActivityView<'a> {
    /// Tipo del elemento: `"job"` o `"subagent"`.
    pub kind: &'a str,
    /// Nombre o etiqueta del elemento.
    pub label: &'a str,
    pub status: ToolStatus,
    /// Fracción de progreso y su etiqueta. Solo se pinta mientras corre.
    pub progress: Option<(f64, &'a str)>,
    /// Resultado o detalle final.
    pub body: Option<&'a str>,
}

/// Cabecera (tipo y etiqueta), progreso si está en marcha y, si procede, el
/// resultado plegado a `FOLDED_LINES` líneas más un recuento de las ocultas.
/// Los prefijos de cada línea los pinta el widget y no forman parte del texto.
pub fn activity_lines(view: &ActivityView<'_>) -> Vec<Line> {
    let label = view.label.lines().next().unwrap_or("").trim();
    let mut text = view.kind.to_string();
    let mut spans = vec![SpanSpec {
        start: 0,
        len: view.kind.len(),
        token: StyleToken::ToolName,
    }];
    if !label.is_empty() {
        let start = text.len() + "  ".len();
        text.push_str("  ");
        text.push_str(label);
        spans.push(SpanSpec {
            start,
            len: label.len(),
            token: StyleToken::Muted,
        });
    }
    let head = Line {
        spans,
        ..Line::new(LineKind::ActivityHead(view.status), text)
    };

    let mut lines = vec![head];
    if view.status == ToolStatus::Running {
        if let Some((fraction, progress)) = view.progress {
            let pct = percent(fraction);
            let progress = progress.trim();
            let text = if progress.is_empty() {
                format!("{pct}%")
            } else {
                format!("{pct}% {progress}")
            };
            lines.push(Line::new(LineKind::ToolBody, text));
        }
    }
    if let Some(body) = view.body.map(str::trim).filter(|b| !b.is_empty()) {
        let rows: Vec<&str> = body.lines().collect();
        let shown = rows.len().min(FOLDED_LINES);
        lines.extend(
            rows[..shown]
                .iter()
                .map(|l| Line::new(LineKind::ToolBody, *l)),
        );
        if shown < rows.len() {
            let hidden = rows.len() - shown;
            lines.push(Line::new(
                LineKind::ToolMore,
                format!("… ({hidden} more lines)"),
            ));
        }
    }
    lines
}

/// Porcentaje entero de una fracción, acotada a `[0, 1]`. NaN cuenta como 0.
fn percent(fraction: f64) -> u32 {
    let fraction = if fraction.is_nan() {
        0.0
    } else {
        fraction.clamp(0.0, 1.0)
    };
    (fraction * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use crate::state::{LineKind, StyleToken, ToolStatus};

    use super::{activity_lines, ActivityView};

    fn view<'a>(status: ToolStatus) -> ActivityView<'a> {
        ActivityView {
            kind: "job",
            label: "build",
            status,
            progress: None,
            body: None,
        }
    }

    #[test]
    fn header_carries_kind_label_status_and_spans() {
        let lines = activity_lines(&view(ToolStatus::Running));
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, LineKind::ActivityHead(ToolStatus::Running));
        assert_eq!(lines[0].plain(), "job  build");
        assert_eq!((lines[0].spans[0].start, lines[0].spans[0].len), (0, 3));
        assert_eq!(lines[0].spans[0].token, StyleToken::ToolName);
        assert_eq!(lines[0].spans[1].token, StyleToken::Muted);
    }

    #[test]
    fn running_job_shows_progress_percentage() {
        let mut v = view(ToolStatus::Running);
        v.progress = Some((0.5, "compiling"));
        let lines = activity_lines(&v);
        assert_eq!(lines[1].kind, LineKind::ToolBody);
        assert_eq!(lines[1].plain(), "50% compiling");
        v.progress = Some((0.25, ""));
        assert_eq!(activity_lines(&v)[1].plain(), "25%");
    }

    #[test]
    fn progress_is_clamped_to_a_percentage() {
        let mut v = view(ToolStatus::Running);
        v.progress = Some((7.0, "x"));
        assert_eq!(activity_lines(&v)[1].plain(), "100% x");
        v.progress = Some((-1.0, "x"));
        assert_eq!(activity_lines(&v)[1].plain(), "0% x");
        v.progress = Some((f64::NAN, "x"));
        assert_eq!(activity_lines(&v)[1].plain(), "0% x");
    }

    #[test]
    fn progress_is_hidden_once_settled() {
        let mut v = view(ToolStatus::Success);
        v.progress = Some((0.5, "x"));
        assert_eq!(activity_lines(&v).len(), 1);
    }

    #[test]
    fn long_result_is_folded_to_six_lines_plus_a_count() {
        let body = (1..=9)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut v = view(ToolStatus::Success);
        v.body = Some(&body);
        let lines = activity_lines(&v);
        assert_eq!(lines.len(), 1 + 6 + 1);
        assert_eq!(lines[7].kind, LineKind::ToolMore);
        assert_eq!(lines[7].plain(), "… (3 more lines)");
    }

    #[test]
    fn blank_body_adds_nothing() {
        let mut v = view(ToolStatus::Error);
        v.body = Some("  \n ");
        assert_eq!(activity_lines(&v).len(), 1);
    }
}
