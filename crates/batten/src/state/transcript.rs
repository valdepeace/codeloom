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
    Code,
    Strong,
    Emphasis,
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

/// Cómo se derivan las líneas de una entrada a partir de su texto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Una línea por línea de texto.
    Plain,
    /// `markdown::parse`.
    Markdown,
}

/// Identificador estable de una entrada (las entradas no se borran).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryId(usize);

#[derive(Debug, Clone)]
struct Entry {
    kind: LineKind,
    format: Format,
    source: String,
    lines: Vec<Line>,
    rev: u64,
}

impl Entry {
    fn derive(&mut self) {
        self.lines = match self.format {
            Format::Plain => self
                .source
                .lines()
                .map(|l| Line::new(self.kind, l))
                .collect(),
            Format::Markdown => super::markdown::parse(&self.source, self.kind),
        };
    }
}

/// Transcript como lista de entradas (mensajes) con revisión. Cada entrada
/// guarda su texto y las líneas derivadas; `append` re-deriva solo la suya.
/// También se ve como una lista de líneas globales (`len`, `line`, `iter`).
#[derive(Debug, Default, Clone)]
pub struct Transcript {
    entries: Vec<Entry>,
    /// Línea global donde empieza cada entrada.
    starts: Vec<usize>,
    total_lines: usize,
    next_rev: u64,
}

impl Transcript {
    pub fn new() -> Self {
        Self::default()
    }

    fn bump(&mut self) -> u64 {
        self.next_rev += 1;
        self.next_rev
    }

    fn push_built(&mut self, mut entry: Entry) -> EntryId {
        entry.rev = self.bump();
        self.starts.push(self.total_lines);
        self.total_lines += entry.lines.len();
        self.entries.push(entry);
        EntryId(self.entries.len() - 1)
    }

    /// Nueva entrada a partir de su texto.
    pub fn push_entry(
        &mut self,
        kind: LineKind,
        format: Format,
        text: impl Into<String>,
    ) -> EntryId {
        let mut entry = Entry {
            kind,
            format,
            source: text.into(),
            lines: Vec::new(),
            rev: 0,
        };
        entry.derive();
        self.push_built(entry)
    }

    /// Una línea ya construida como entrada propia (no admite `append`).
    pub fn push(&mut self, line: Line) {
        let entry = Entry {
            kind: line.kind,
            format: Format::Plain,
            source: line.text.clone(),
            lines: vec![line],
            rev: 0,
        };
        self.push_built(entry);
    }

    /// Añade texto a una entrada y re-deriva solo sus líneas.
    pub fn append(&mut self, id: EntryId, delta: &str) {
        let rev = self.bump();
        let Some(entry) = self.entries.get_mut(id.0) else {
            return;
        };
        entry.source.push_str(delta);
        entry.derive();
        entry.rev = rev;
        self.reindex_from(id.0);
    }

    fn reindex_from(&mut self, from: usize) {
        let mut acc = self.starts.get(from).copied().unwrap_or(0);
        for i in from..self.entries.len() {
            self.starts[i] = acc;
            acc += self.entries[i].lines.len();
        }
        self.total_lines = acc;
    }

    /// Número de líneas globales.
    pub fn len(&self) -> usize {
        self.total_lines
    }

    pub fn is_empty(&self) -> bool {
        self.total_lines == 0
    }

    /// Línea global `i`.
    pub fn line(&self, i: usize) -> Option<&Line> {
        let (entry, offset) = self.locate(i)?;
        self.entries[entry].lines.get(offset)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Line> {
        self.entries.iter().flat_map(|e| e.lines.iter())
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Revisión de la entrada `i` (sube cada vez que cambia).
    pub fn entry_rev(&self, i: usize) -> u64 {
        self.entries.get(i).map(|e| e.rev).unwrap_or(0)
    }

    pub fn entry_lines(&self, i: usize) -> &[Line] {
        self.entries
            .get(i)
            .map(|e| e.lines.as_slice())
            .unwrap_or(&[])
    }

    /// Línea global donde empieza la entrada `i`.
    pub fn entry_start(&self, i: usize) -> usize {
        self.starts.get(i).copied().unwrap_or(self.total_lines)
    }

    /// `(entrada, línea dentro de la entrada)` de una línea global.
    pub fn locate(&self, line: usize) -> Option<(usize, usize)> {
        if line >= self.total_lines {
            return None;
        }
        let entry = self.starts.partition_point(|&s| s <= line) - 1;
        Some((entry, line - self.starts[entry]))
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
    fn entries_index_global_lines() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::User, Format::Plain, "uno\ndos");
        t.push_entry(LineKind::Assistant, Format::Plain, "tres");
        assert_eq!(t.len(), 3);
        assert_eq!(t.entry_count(), 2);
        assert_eq!(t.entry_start(1), 2);
        assert_eq!(t.line(2).unwrap().plain(), "tres");
        assert_eq!(t.locate(1), Some((0, 1)));
        assert_eq!(t.locate(2), Some((1, 0)));
        assert_eq!(t.locate(3), None);
    }

    #[test]
    fn append_rederives_only_its_entry() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::User, Format::Plain, "hola");
        let id = t.push_entry(LineKind::Assistant, Format::Markdown, "# Tí");
        let (r0, r1) = (t.entry_rev(0), t.entry_rev(1));
        t.append(id, "tulo\n\n- a\n- b");
        assert_eq!(t.entry_rev(0), r0);
        assert!(t.entry_rev(1) > r1);
        let texts: Vec<&str> = t.iter().map(|l| l.plain()).collect();
        assert_eq!(texts, vec!["hola", "Título", "", "• a", "• b"]);
        assert_eq!(t.len(), 5);
    }

    #[test]
    fn append_in_the_middle_reindexes_the_rest() {
        let mut t = Transcript::new();
        let a = t.push_entry(LineKind::Assistant, Format::Plain, "a");
        t.push_entry(LineKind::User, Format::Plain, "z");
        t.append(a, "\nb");
        assert_eq!(t.entry_start(1), 2);
        assert_eq!(t.line(2).unwrap().plain(), "z");
    }

    #[test]
    fn empty_entries_do_not_break_locate() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::Assistant, Format::Markdown, "");
        t.push_entry(LineKind::User, Format::Plain, "x");
        assert_eq!(t.len(), 1);
        assert_eq!(t.locate(0), Some((1, 0)));
        assert_eq!(t.line(0).unwrap().plain(), "x");
    }

    #[test]
    fn line_with_no_spans_uses_plain_text() {
        let l = Line::new(LineKind::System, "sys");
        assert!(l.spans.is_empty());
        assert_eq!(l.plain(), "sys");
    }
}
