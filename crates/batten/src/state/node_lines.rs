//! Líneas de un nodo de subagente en el árbol de la conversación. La cabecera
//! lleva el icono de estado A2A y el nombre exacto del estado; desplegado, los
//! hijos cuelgan de ella con guías de árbol. Los prefijos de cada línea los
//! pinta el widget y no forman parte del texto.

use super::{Line, LineKind, SpanSpec, StyleToken, ToolStatus};

/// Hijo de un nodo ya traducido por quien lo construye.
#[derive(Debug, Clone, Copy)]
pub struct NodeChild<'a> {
    /// Icono de estado A2A del hijo (un carácter).
    pub glyph: &'a str,
    pub text: &'a str,
}

/// Datos de un nodo de subagente, ya traducidos por quien lo construye.
#[derive(Debug, Clone, Copy)]
pub struct NodeView<'a> {
    /// Icono de estado A2A (un carácter).
    pub glyph: &'a str,
    /// Tipo del nodo: `"subagent"`.
    pub kind: &'a str,
    pub label: &'a str,
    /// Etiqueta de origen, p. ej. `"[rust]"` o `"[front·project]"`.
    pub tag: Option<&'a str>,
    /// Nombre A2A exacto del estado: `"working"`, `"input-required"`…
    pub state: &'a str,
    /// Estado agrupado, solo para el color.
    pub status: ToolStatus,
    /// Resumen de progreso, p. ej. `"step 2/30 · read_file"`.
    pub summary: Option<&'a str>,
    pub expanded: bool,
    pub children: &'a [NodeChild<'a>],
}

/// Cabecera de una línea y, si el nodo está desplegado, una línea por hijo con
/// sus guías. Plegado, solo la cabecera.
pub fn node_lines(view: &NodeView<'_>) -> Vec<Line> {
    let mark = if view.expanded { "▾" } else { "▸" };
    let mut text = format!("{mark} {} ", view.glyph);
    let kind_start = text.len();
    text.push_str(view.kind);
    let kind_span = SpanSpec {
        start: kind_start,
        len: view.kind.len(),
        token: StyleToken::ToolName,
    };

    let label = view.label.lines().next().unwrap_or("").trim();
    if !label.is_empty() {
        text.push_str("  ");
        text.push_str(label);
    }
    if let Some(tag) = view.tag.map(str::trim).filter(|t| !t.is_empty()) {
        text.push(' ');
        text.push_str(tag);
    }
    text.push_str(" (");
    text.push_str(view.state);
    text.push(')');
    if let Some(summary) = view.summary.map(str::trim).filter(|s| !s.is_empty()) {
        text.push_str(" · ");
        text.push_str(summary);
    }

    let head = Line {
        spans: vec![kind_span],
        ..Line::new(LineKind::NodeHead(view.status), text)
    };
    let mut lines = vec![head];
    if view.expanded {
        let last = view.children.len().saturating_sub(1);
        lines.extend(view.children.iter().enumerate().map(|(i, child)| {
            let guide = if i == last { "└─" } else { "├─" };
            Line::new(
                LineKind::NodeBody,
                format!("{guide} {} {}", child.glyph, child.text),
            )
        }));
    }
    lines
}

#[cfg(test)]
mod tests {
    use crate::state::{LineKind, StyleToken, ToolStatus};

    use super::{node_lines, NodeChild, NodeView};

    fn view<'a>(children: &'a [NodeChild<'a>]) -> NodeView<'a> {
        NodeView {
            glyph: "●",
            kind: "subagent",
            label: "review",
            tag: Some("[rust]"),
            state: "working",
            status: ToolStatus::Running,
            summary: Some("step 2/30 · read_file"),
            expanded: false,
            children,
        }
    }

    #[test]
    fn folded_node_is_one_line_with_glyph_and_exact_state() {
        let children = [NodeChild {
            glyph: "✓",
            text: "read_file",
        }];
        let lines = node_lines(&view(&children));
        assert_eq!(lines.len(), 1, "plegado: solo la cabecera");
        assert_eq!(lines[0].kind, LineKind::NodeHead(ToolStatus::Running));
        assert_eq!(
            lines[0].plain(),
            "▸ ● subagent  review [rust] (working) · step 2/30 · read_file"
        );
        // «▸ ● » ocupa 8 bytes: el tipo empieza en el offset 8.
        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].start, 8);
        assert_eq!(lines[0].spans[0].len, "subagent".len());
        assert_eq!(lines[0].spans[0].token, StyleToken::ToolName);
    }

    #[test]
    fn expanded_node_lists_children_with_tree_guides() {
        let children = [
            NodeChild {
                glyph: "✓",
                text: "read_file src/lib.rs",
            },
            NodeChild {
                glyph: "●",
                text: "thinking",
            },
            NodeChild {
                glyph: "⊘",
                text: "bash",
            },
        ];
        let mut v = view(&children);
        v.expanded = true;
        let lines = node_lines(&v);
        assert_eq!(lines.len(), 1 + 3);
        assert!(lines[0].plain().starts_with("▾ "), "la cabecera lleva ▾");
        assert_eq!(lines[1].kind, LineKind::NodeBody);
        assert_eq!(lines[1].plain(), "├─ ✓ read_file src/lib.rs");
        assert_eq!(lines[2].plain(), "├─ ● thinking");
        assert_eq!(lines[3].kind, LineKind::NodeBody);
        assert_eq!(lines[3].plain(), "└─ ⊘ bash");
    }

    #[test]
    fn node_head_without_tag_or_summary_has_no_stray_spaces() {
        let v = NodeView {
            glyph: "✓",
            kind: "subagent",
            label: "review",
            tag: None,
            state: "completed",
            status: ToolStatus::Success,
            summary: None,
            expanded: false,
            children: &[],
        };
        let lines = node_lines(&v);
        assert_eq!(lines[0].plain(), "▸ ✓ subagent  review (completed)");
        assert_eq!(lines[0].kind, LineKind::NodeHead(ToolStatus::Success));
    }

    #[test]
    fn empty_label_or_blank_tag_leave_no_double_space() {
        let v = NodeView {
            glyph: "✓",
            kind: "subagent",
            label: "  ",
            tag: Some(" "),
            state: "completed",
            status: ToolStatus::Success,
            summary: Some(""),
            expanded: false,
            children: &[],
        };
        let lines = node_lines(&v);
        assert_eq!(lines[0].plain(), "▸ ✓ subagent (completed)");
    }
}
