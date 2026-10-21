//! Líneas del transcript que representan la lista de tareas (`todo_write`)
//! en el look Crush.

use super::{Line, LineKind, SpanSpec, StyleToken, ToolStatus};

/// Elementos visibles antes de plegar el resto.
const FOLDED_ITEMS: usize = 12;

/// Lista de tareas como líneas del transcript: una cabecera con el recuento
/// de hechas y, debajo, cada tarea con su estado. Más de `FOLDED_ITEMS`
/// tareas se pliegan en una línea de recuento. Los prefijos de cada línea los
/// pinta el widget y no forman parte del texto.
pub fn todo_lines(items: &[(&str, ToolStatus)]) -> Vec<Line> {
    if items.is_empty() {
        return Vec::new();
    }

    let total = items.len();
    let done = items
        .iter()
        .filter(|(_, status)| *status == ToolStatus::Success)
        .count();
    let head_status = if done == total {
        ToolStatus::Success
    } else if items
        .iter()
        .any(|(_, status)| *status == ToolStatus::Running)
    {
        ToolStatus::Running
    } else {
        ToolStatus::Pending
    };

    let text = format!("todos  {done}/{total}");
    let head = Line {
        spans: vec![
            SpanSpec {
                start: 0,
                len: "todos".len(),
                token: StyleToken::ToolName,
            },
            SpanSpec {
                start: "todos".len(),
                len: text.len() - "todos".len(),
                token: StyleToken::Muted,
            },
        ],
        ..Line::new(LineKind::ActivityHead(head_status), text)
    };

    let mut lines = vec![head];
    for (content, status) in items.iter().take(FOLDED_ITEMS) {
        let text = content
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(Line::new(LineKind::TodoItem(item_status(*status)), text));
    }
    if total > FOLDED_ITEMS {
        lines.push(Line::new(
            LineKind::ToolMore,
            format!("… ({} more)", total - FOLDED_ITEMS),
        ));
    }
    lines
}

/// Estado con el que se pinta una tarea: solo importan las tres situaciones
/// de una lista; cualquier otro estado se trata como pendiente.
fn item_status(status: ToolStatus) -> ToolStatus {
    match status {
        ToolStatus::Running => ToolStatus::Running,
        ToolStatus::Success => ToolStatus::Success,
        _ => ToolStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use crate::state::{LineKind, StyleToken, ToolStatus};

    use super::todo_lines;

    #[test]
    fn empty_has_no_lines() {
        assert!(todo_lines(&[]).is_empty());
    }

    #[test]
    fn header_counts_done_over_total() {
        let lines = todo_lines(&[
            ("a", ToolStatus::Success),
            ("b", ToolStatus::Pending),
            ("c", ToolStatus::Pending),
        ]);
        assert_eq!(lines[0].plain(), "todos  1/3");
        assert_eq!(lines[0].kind, LineKind::ActivityHead(ToolStatus::Pending));
        assert_eq!(lines[0].spans[0].token, StyleToken::ToolName);
        assert_eq!(lines[0].spans[1].token, StyleToken::Muted);
    }

    #[test]
    fn header_is_running_when_one_is_in_progress() {
        let lines = todo_lines(&[
            ("a", ToolStatus::Success),
            ("b", ToolStatus::Running),
            ("c", ToolStatus::Pending),
        ]);
        assert_eq!(lines[0].kind, LineKind::ActivityHead(ToolStatus::Running));
    }

    #[test]
    fn header_is_success_when_all_done() {
        let lines = todo_lines(&[("a", ToolStatus::Success), ("b", ToolStatus::Success)]);
        assert_eq!(lines[0].plain(), "todos  2/2");
        assert_eq!(lines[0].kind, LineKind::ActivityHead(ToolStatus::Success));
    }

    #[test]
    fn items_keep_their_status_and_one_line() {
        let lines = todo_lines(&[
            ("a\nb", ToolStatus::Running),
            ("c", ToolStatus::Success),
            ("d", ToolStatus::Error),
        ]);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[1].kind, LineKind::TodoItem(ToolStatus::Running));
        assert_eq!(lines[1].plain(), "a b");
        assert_eq!(lines[2].kind, LineKind::TodoItem(ToolStatus::Success));
        assert_eq!(lines[3].kind, LineKind::TodoItem(ToolStatus::Pending));
    }

    #[test]
    fn long_lists_are_folded_after_twelve() {
        let names: Vec<String> = (1..=15).map(|n| format!("tarea {n}")).collect();
        let items: Vec<(&str, ToolStatus)> = names
            .iter()
            .map(|n| (n.as_str(), ToolStatus::Pending))
            .collect();
        let lines = todo_lines(&items);
        assert_eq!(lines.len(), 1 + 12 + 1);
        assert_eq!(lines[13].kind, LineKind::ToolMore);
        assert_eq!(lines[13].plain(), "… (3 more)");
    }
}
