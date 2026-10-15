#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    User,
    System,
    Assistant,
    Thinking,
    ToolCall,
    ToolResult,
    Todo,
    Plan,
    Banner,
    Error,
}

/// Token semantico de estilo. La capa `state` no conoce colores: los widgets
/// resuelven cada token a un `Style` con el tema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleToken {
    Default,
    Muted,
    Dim,
    Heading,
    User,
    Assistant,
    System,
    Error,
    Warning,
    Success,
    ToolName,
    ToolPending,
    ToolRunning,
    ToolSuccess,
    ToolError,
    ToolCanceled,
    TodoPending,
    TodoInProgress,
    TodoCompleted,
    Accent,
    AccentFrom,
    AccentTo,
    LogoField,
    Scrollbar,
    Border,
    DiffInsert,
    DiffDelete,
    DiffHunk,
    Selection,
}

/// Rango con estilo, como offsets sobre el texto plano de una linea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpanSpec {
    pub start: usize,
    pub len: usize,
    pub token: StyleToken,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub kind: LineKind,
    pub spans: Vec<SpanSpec>,
}

impl Line {
    pub fn new(kind: LineKind, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind,
            spans: Vec::new(),
        }
    }

    pub fn plain(&self) -> &str {
        &self.text
    }
}

#[derive(Debug, Default, Clone)]
pub struct Transcript {
    lines: Vec<Line>,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn push(&mut self, line: Line) {
        self.lines.push(line);
    }
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
    pub fn line(&self, i: usize) -> Option<&Line> {
        self.lines.get(i)
    }
    pub fn last_mut(&mut self) -> Option<&mut Line> {
        self.lines.last_mut()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_iterate() {
        let mut t = Transcript::new();
        assert!(t.is_empty());
        t.push(Line::new(LineKind::User, "hi"));
        t.push(Line::new(LineKind::Assistant, "yo"));
        assert_eq!(t.len(), 2);
        assert_eq!(t.line(0).unwrap().plain(), "hi");
        assert_eq!(t.line(1).unwrap().kind, LineKind::Assistant);
        assert_eq!(t.iter().count(), 2);
    }

    #[test]
    fn last_mut_edits_in_place() {
        let mut t = Transcript::new();
        t.push(Line::new(LineKind::Assistant, "a"));
        t.last_mut().unwrap().text.push('b');
        assert_eq!(t.line(0).unwrap().plain(), "ab");
    }

    #[test]
    fn line_with_no_spans_uses_plain_text() {
        let l = Line::new(LineKind::System, "sys");
        assert!(l.spans.is_empty());
        assert_eq!(l.plain(), "sys");
    }
}
