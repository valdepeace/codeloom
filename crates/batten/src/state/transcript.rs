use super::diff::DiffKind;
use super::tool::ToolStatus;

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
    /// Cabecera de una herramienta, con su estado.
    ToolHead(ToolStatus),
    /// Cabecera de un elemento de actividad (job o subagente), con su estado.
    ActivityHead(ToolStatus),
    /// Elemento de la lista de tareas, con su estado (pendiente, en curso o hecha).
    TodoItem(ToolStatus),
    /// Línea de salida de una herramienta.
    ToolBody,
    /// Línea de diff bajo una herramienta `edit`: cabecera de hunk, contexto,
    /// inserción o borrado.
    ToolDiff(DiffKind),
    /// Indicación de salida oculta bajo una herramienta.
    ToolMore,
    /// Cabecera de un nodo de subagente, con su estado. El icono va en el texto.
    NodeHead(ToolStatus),
    /// Hijo de un nodo de subagente desplegado. El guion de árbol va en el texto.
    NodeBody,
    /// Aviso de que un bloque largo tiene líneas plegadas.
    Fold,
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
    /// Ruta o patrón del dato principal de una pieza de herramienta.
    ToolPath,
    /// Comando de una pieza de herramienta (`bash`, `job_start`).
    ToolCommand,
    /// Recuento de una pieza de herramienta («3 matches», «2 options»).
    ToolCount,
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

/// Enlace sobre el texto plano de una línea (offsets en bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSpec {
    pub start: usize,
    pub len: usize,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub text: String,
    pub kind: LineKind,
    pub spans: Vec<SpanSpec>,
    pub links: Vec<LinkSpec>,
}

impl Line {
    pub fn new(kind: LineKind, text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind,
            spans: Vec::new(),
            links: Vec::new(),
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

impl EntryId {
    /// Posición de la entrada en el transcript (orden de inserción).
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone)]
struct Entry {
    kind: LineKind,
    format: Format,
    source: String,
    lines: Vec<Line>,
    rev: u64,
    /// Se deriva de un texto de usuario o de asistente: es la única clase de entrada que se pliega.
    foldable: bool,
    /// Ya no recibe texto. Mientras llega (streaming) se ve entera, aunque sea larga.
    sealed: bool,
    /// La persona usuaria la ha desplegado.
    expanded: bool,
    /// Líneas que tendría desplegada.
    total: usize,
}

impl Entry {
    fn new(kind: LineKind, format: Format, source: String, sealed: bool) -> Self {
        Self {
            kind,
            format,
            source,
            lines: Vec::new(),
            rev: 0,
            foldable: matches!(kind, LineKind::User | LineKind::Assistant),
            sealed,
            expanded: false,
            total: 0,
        }
    }

    /// Líneas visibles a partir del texto. Con `limit`, una entrada sellada y sin desplegar que
    /// pase de `limit` líneas muestra las primeras y un aviso con las que oculta. No se pliega
    /// si solo sobraría una línea: el aviso ocuparía lo mismo que ella.
    fn derive(&mut self, limit: Option<usize>) {
        let mut lines = match self.format {
            Format::Plain => self
                .source
                .lines()
                .map(|l| Line::new(self.kind, l))
                .collect(),
            Format::Markdown => super::markdown::parse(&self.source, self.kind),
        };
        self.total = lines.len();
        if let Some(limit) = limit {
            if self.foldable && self.sealed && !self.expanded && lines.len() > limit + 1 {
                let hidden = lines.len() - limit;
                lines.truncate(limit);
                lines.push(Line::new(
                    LineKind::Fold,
                    format!("… ({hidden} lines hidden) [click or space to expand]"),
                ));
            }
        }
        self.lines = lines;
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
    /// Líneas a partir de las cuales se pliega un bloque largo. `None`: nunca.
    fold_limit: Option<usize>,
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

    /// Nueva entrada a partir de su texto, ya completa (puede plegarse).
    pub fn push_entry(
        &mut self,
        kind: LineKind,
        format: Format,
        text: impl Into<String>,
    ) -> EntryId {
        let mut entry = Entry::new(kind, format, text.into(), true);
        entry.derive(self.fold_limit);
        self.push_built(entry)
    }

    /// Nueva entrada que seguirá recibiendo texto con `append`: no se pliega hasta `settle`.
    pub fn push_streaming(
        &mut self,
        kind: LineKind,
        format: Format,
        text: impl Into<String>,
    ) -> EntryId {
        let mut entry = Entry::new(kind, format, text.into(), false);
        entry.derive(self.fold_limit);
        self.push_built(entry)
    }

    /// Fija a partir de cuántas líneas se pliega un bloque largo (`None`: nunca) y recalcula.
    pub fn set_fold_limit(&mut self, limit: Option<usize>) {
        self.fold_limit = limit;
        self.rederive_all();
    }

    /// Da por terminado el texto de todas las entradas: las largas se pliegan.
    pub fn settle(&mut self) {
        for entry in &mut self.entries {
            entry.sealed = true;
        }
        self.rederive_all();
    }

    /// Re-deriva las entradas cuyo aspecto cambia y reindexa. Las que no cambian conservan su
    /// revisión, para no invalidar su medida.
    fn rederive_all(&mut self) {
        let limit = self.fold_limit;
        let mut first_changed = None;
        for i in 0..self.entries.len() {
            let mut probe = self.entries[i].clone();
            if probe.foldable {
                probe.derive(limit);
            }
            if probe.lines != self.entries[i].lines {
                probe.rev = self.bump();
                self.entries[i] = probe;
                first_changed.get_or_insert(i);
            } else {
                self.entries[i].total = probe.total;
            }
        }
        if let Some(i) = first_changed {
            self.reindex_from(i);
        }
    }

    /// `true` si la entrada `i` tiene más líneas que el límite de pliegue (esté plegada o no).
    pub fn is_foldable(&self, i: usize) -> bool {
        let Some(limit) = self.fold_limit else {
            return false;
        };
        self.entries
            .get(i)
            .is_some_and(|e| e.foldable && e.sealed && e.total > limit + 1)
    }

    /// Pliega o despliega la entrada `i`. `false` si no hay nada que plegar en ella.
    pub fn toggle_fold(&mut self, i: usize) -> bool {
        if !self.is_foldable(i) {
            return false;
        }
        let limit = self.fold_limit;
        let rev = self.bump();
        let entry = &mut self.entries[i];
        entry.expanded = !entry.expanded;
        entry.derive(limit);
        entry.rev = rev;
        self.reindex_from(i);
        true
    }

    /// Entrada con líneas ya construidas. Admite una lista vacía. Estas
    /// entradas no admiten `append`: sus líneas no se derivan de un texto.
    pub fn push_lines(&mut self, lines: Vec<Line>) -> EntryId {
        let entry = Self::built_entry(lines);
        self.push_built(entry)
    }

    /// Sustituye las líneas de una entrada existente y sube su revisión.
    /// Un `id` inválido no hace nada.
    pub fn replace(&mut self, id: EntryId, lines: Vec<Line>) {
        let rev = self.bump();
        let Some(entry) = self.entries.get_mut(id.0) else {
            return;
        };
        let built = Self::built_entry(lines);
        entry.kind = built.kind;
        entry.source = built.source;
        entry.lines = built.lines;
        entry.rev = rev;
        self.reindex_from(id.0);
    }

    /// Entrada de líneas propias. Su tipo es el de la primera línea (o
    /// `System` si no hay ninguna) y su texto es la unión de las líneas.
    fn built_entry(lines: Vec<Line>) -> Entry {
        let kind = lines.first().map_or(LineKind::System, |l| l.kind);
        let source = lines
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut entry = Entry::new(kind, Format::Plain, source, true);
        entry.foldable = false;
        entry.lines = lines;
        entry
    }

    /// Una línea ya construida como entrada propia (no admite `append`).
    pub fn push(&mut self, line: Line) {
        let mut entry = Entry::new(line.kind, Format::Plain, line.text.clone(), true);
        entry.foldable = false;
        entry.lines = vec![line];
        self.push_built(entry);
    }

    /// Añade texto a una entrada y re-deriva solo sus líneas.
    pub fn append(&mut self, id: EntryId, delta: &str) {
        let limit = self.fold_limit;
        let rev = self.bump();
        let Some(entry) = self.entries.get_mut(id.0) else {
            return;
        };
        entry.source.push_str(delta);
        // Vuelve a llegar texto: se ve entera hasta el próximo `settle`.
        entry.sealed = false;
        entry.derive(limit);
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

    /// Texto completo de la entrada `i`, también el que un pliegue deja fuera de la vista. Es lo
    /// que debe copiarse: copiar las líneas visibles se llevaría el aviso en lugar del texto.
    pub fn entry_text(&self, i: usize) -> Option<String> {
        let entry = self.entries.get(i)?;
        let mut completa;
        let lines = if entry.lines.last().is_some_and(|l| l.kind == LineKind::Fold) {
            completa = entry.clone();
            completa.expanded = true;
            completa.derive(self.fold_limit);
            &completa.lines
        } else {
            &entry.lines
        };
        Some(
            lines
                .iter()
                .map(|l| l.plain())
                .collect::<Vec<_>>()
                .join("\n"),
        )
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

    #[test]
    fn push_lines_creates_a_multi_line_entry() {
        let mut t = Transcript::new();
        let id = t.push_lines(vec![
            Line::new(LineKind::Banner, "a"),
            Line::new(LineKind::Banner, "b"),
        ]);
        t.push_entry(LineKind::User, Format::Plain, "z");
        assert_eq!(t.len(), 3);
        assert_eq!(t.entry_count(), 2);
        assert_eq!(t.entry_lines(id.index()).len(), 2);
        assert_eq!(t.line(2).unwrap().plain(), "z");
    }

    #[test]
    fn replace_rewrites_one_entry_and_reindexes_the_rest() {
        let mut t = Transcript::new();
        t.push_entry(LineKind::User, Format::Plain, "antes");
        let id = t.push_lines(vec![Line::new(LineKind::Banner, "x")]);
        t.push_entry(LineKind::Assistant, Format::Plain, "despues");
        let (r0, r1) = (t.entry_rev(0), t.entry_rev(1));
        t.replace(
            id,
            vec![
                Line::new(LineKind::Banner, "x1"),
                Line::new(LineKind::Banner, "x2"),
                Line::new(LineKind::Banner, "x3"),
            ],
        );
        assert_eq!(t.entry_rev(0), r0);
        assert!(t.entry_rev(1) > r1);
        assert_eq!(t.len(), 5);
        assert_eq!(t.entry_start(2), 4);
        assert_eq!(t.line(4).unwrap().plain(), "despues");
        t.replace(id, Vec::new());
        assert_eq!(t.len(), 2);
        assert_eq!(t.line(1).unwrap().plain(), "despues");
    }

    fn lines_of(n: usize) -> String {
        (1..=n)
            .map(|i| format!("l{i}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_long_sealed_block_folds_to_the_limit_with_a_hint() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        t.push_entry(LineKind::Assistant, Format::Plain, lines_of(20));
        t.push_entry(LineKind::User, Format::Plain, "despues");

        assert_eq!(t.entry_lines(0).len(), 11);
        assert_eq!(t.entry_lines(0)[9].plain(), "l10");
        let aviso = t.entry_lines(0).last().unwrap();
        assert_eq!(aviso.kind, LineKind::Fold);
        assert_eq!(
            aviso.plain(),
            "… (10 lines hidden) [click or space to expand]"
        );
        // Los índices globales cuentan las líneas visibles.
        assert_eq!(t.entry_start(1), 11);
        assert_eq!(t.len(), 12);
    }

    #[test]
    fn a_block_that_would_hide_a_single_line_is_not_folded() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        t.push_entry(LineKind::Assistant, Format::Plain, lines_of(11));
        t.push_entry(LineKind::Assistant, Format::Plain, lines_of(12));

        assert_eq!(t.entry_lines(0).len(), 11, "11 líneas caben");
        assert!(t.entry_lines(0).iter().all(|l| l.kind != LineKind::Fold));
        assert_eq!(t.entry_lines(1).len(), 11, "12 líneas sí se pliegan");
        assert_eq!(t.entry_lines(1).last().unwrap().kind, LineKind::Fold);
    }

    #[test]
    fn a_streaming_block_is_whole_until_it_settles() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        let id = t.push_streaming(LineKind::Assistant, Format::Plain, lines_of(5));
        t.append(id, "\nl6\nl7\nl8\nl9\nl10\nl11\nl12\nl13\nl14\nl15\nl16");
        assert_eq!(t.entry_lines(0).len(), 16, "se ve entera mientras llega");

        t.settle();
        assert_eq!(t.entry_lines(0).len(), 11);

        // Si vuelve a llegar texto, se despliega otra vez hasta el siguiente `settle`.
        t.append(id, "\nmas");
        assert_eq!(t.entry_lines(0).len(), 17);
    }

    #[test]
    fn toggling_expands_and_collapses_and_reindexes_the_rest() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        t.push_entry(LineKind::Assistant, Format::Plain, lines_of(20));
        t.push_entry(LineKind::User, Format::Plain, "despues");
        let rev = t.entry_rev(0);

        assert!(t.is_foldable(0));
        assert!(t.toggle_fold(0));
        assert_eq!(t.entry_lines(0).len(), 20);
        assert!(t.entry_lines(0).iter().all(|l| l.kind != LineKind::Fold));
        assert_eq!(t.entry_start(1), 20);
        assert!(t.entry_rev(0) > rev);
        assert!(t.is_foldable(0), "desplegada sigue siendo plegable");

        assert!(t.toggle_fold(0));
        assert_eq!(t.entry_lines(0).len(), 11);
        assert_eq!(t.entry_start(1), 11);
    }

    #[test]
    fn only_user_and_assistant_text_folds_and_no_limit_means_never() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        t.push_entry(LineKind::System, Format::Plain, lines_of(30));
        t.push_lines(
            (0..30)
                .map(|i| Line::new(LineKind::Assistant, format!("x{i}")))
                .collect(),
        );
        assert_eq!(t.entry_lines(0).len(), 30);
        assert_eq!(t.entry_lines(1).len(), 30);
        assert!(!t.toggle_fold(0));
        assert!(!t.toggle_fold(1));

        let mut libre = Transcript::new();
        libre.push_entry(LineKind::Assistant, Format::Plain, lines_of(50));
        assert_eq!(libre.entry_lines(0).len(), 50);
        libre.set_fold_limit(Some(10));
        assert_eq!(libre.entry_lines(0).len(), 11);
        libre.set_fold_limit(None);
        assert_eq!(libre.entry_lines(0).len(), 50);
    }

    #[test]
    fn settling_leaves_untouched_entries_with_their_revision() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        t.push_entry(LineKind::User, Format::Plain, "corto");
        let rev = t.entry_rev(0);
        t.settle();
        assert_eq!(t.entry_rev(0), rev, "no se re-mide lo que no cambia");
    }

    #[test]
    fn entry_text_returns_the_hidden_lines_too() {
        let mut t = Transcript::new();
        t.set_fold_limit(Some(10));
        t.push_entry(LineKind::Assistant, Format::Plain, lines_of(20));
        t.push_entry(LineKind::User, Format::Plain, "corto");

        assert_eq!(t.entry_lines(0).len(), 11);
        assert_eq!(t.entry_text(0).unwrap(), lines_of(20));
        assert_eq!(t.entry_text(1).unwrap(), "corto");
        assert_eq!(t.entry_text(2), None);
        // Consultarlo no despliega nada.
        assert_eq!(t.entry_lines(0).len(), 11);
    }
}
