//! Piezas atómicas por herramienta: cómo se representa una llamada (`ToolItem`)
//! en el transcript a partir de sus hechos tipados (`ToolFacts`).
//!
//! Este módulo no conoce el JSON de entrada: los hechos los extrae el adaptador
//! que vive fuera de `batten` (`codeloom-tui`), de modo que la pieza solo pinta
//! hechos y estado.

use super::tool_lines::{body_lines, more_line, truncate_graphemes, SUMMARY_GRAPHEMES};
use super::{
    tool_lines, Diff, DiffKind, DiffLine, Line, LineKind, SpanSpec, StyleToken, ToolItem,
    ToolStatus,
};
use unicode_segmentation::UnicodeSegmentation;

/// Hechos tipados de una llamada a herramienta, ya extraídos de su entrada.
///
/// Cada campo es opcional: una pieza muestra lo que el adaptador haya podido
/// extraer y degrada con elegancia cuando falte algo. La interpretación exacta
/// de cada campo depende de la herramienta que lo rellena.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolFacts {
    /// Ruta, comando o patrón principal de la llamada.
    pub primary: Option<String>,
    /// Segundo dato corto (p. ej. el filtro `glob` de `grep`).
    pub secondary: Option<String>,
    /// Líneas de inicio y fin (`view`), ambas incluidas.
    pub range: Option<(usize, usize)>,
    /// Texto previo (`edit`).
    pub before: Option<String>,
    /// Texto nuevo (`edit`).
    pub after: Option<String>,
    /// Recuento (coincidencias, tareas…).
    pub count: Option<usize>,
    /// Resto de pares clave-valor que merece la pena mostrar.
    pub items: Vec<(String, String)>,
}

/// Pieza de render de una herramienta: convierte la llamada y sus hechos en
/// líneas del transcript. Las piezas no parsean JSON ni conocen el protocolo.
pub trait ToolAtom: Sync {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line>;
}

/// Pieza de reserva para las herramientas sin pieza propia (y las MCP). Reproduce
/// `tool_lines` exactamente, ignorando los hechos.
struct GenericAtom;

impl ToolAtom for GenericAtom {
    fn lines(&self, item: &ToolItem, _facts: &ToolFacts) -> Vec<Line> {
        tool_lines(item)
    }
}

static GENERIC: GenericAtom = GenericAtom;

/// Partes de un resumen de cabecera. Cada una lleva su token de estilo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Name,
    Command,
    Path,
    Pattern,
    /// Dato secundario y corto: rango de líneas, filtro `glob`.
    Detail,
    Count,
    /// Líneas añadidas de un `edit` (`+N`).
    Added,
    /// Líneas borradas de un `edit` (`-N`).
    Removed,
}

/// Único punto donde se elige el estilo de cada parte del resumen. La Tarea 5
/// lo sustituye por los tokens propios de pieza (`ToolPath`, `ToolCommand`…).
fn style_of(part: Part) -> StyleToken {
    match part {
        Part::Name => StyleToken::ToolName,
        Part::Command => StyleToken::Default,
        Part::Path | Part::Pattern => StyleToken::Accent,
        Part::Detail => StyleToken::Muted,
        Part::Count => StyleToken::Dim,
        Part::Added => StyleToken::DiffInsert,
        Part::Removed => StyleToken::DiffDelete,
    }
}

/// Longitud máxima en grafemas de la cola de un resumen (`in {glob}`, rango).
const TAIL_GRAPHEMES: usize = 40;

/// Líneas de contexto alrededor de cada cambio del diff de `edit`.
const DIFF_CONTEXT: usize = 2;

/// Líneas de diff visibles mientras la llamada no está expandida.
const DIFF_LINES: usize = 12;

/// Primera línea no vacía de `text`, sin espacios sobrantes.
fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// Parte principal del resumen con su cola opcional. La cola se acota primero y
/// la parte principal se trunca para que el resumen completo quepa en
/// `SUMMARY_GRAPHEMES`, de modo que el rango o el filtro no desaparezcan.
fn with_tail(main: &str, main_part: Part, tail: Option<String>) -> Vec<(Part, String)> {
    let tail = tail.map(|tail| truncate_graphemes(&tail, TAIL_GRAPHEMES));
    // El espacio separador cuenta como un grafema más.
    let reserved = tail.as_deref().map_or(0, |t| t.graphemes(true).count() + 1);
    let mut parts = vec![(
        main_part,
        truncate_graphemes(main, SUMMARY_GRAPHEMES - reserved),
    )];
    if let Some(tail) = tail {
        parts.push((Part::Detail, tail));
    }
    parts
}

/// Añade `text` al final de `line` con el token de `part`.
fn push_text(line: &mut Line, text: &str, part: Part) {
    let start = line.text.len();
    line.text.push_str(text);
    line.spans.push(SpanSpec {
        start,
        len: text.len(),
        token: style_of(part),
    });
}

/// Cabecera con el nombre de la herramienta, su estado y las partes del resumen.
/// Mismo formato que la cabecera genérica: nombre, dos espacios y resumen.
fn head_line(item: &ToolItem, parts: &[(Part, String)]) -> Line {
    let mut head = Line::new(LineKind::ToolHead(item.status()), item.name.as_str());
    if !item.name.is_empty() {
        head.spans.push(SpanSpec {
            start: 0,
            len: item.name.len(),
            token: style_of(Part::Name),
        });
    }
    for (index, (part, text)) in parts.iter().enumerate() {
        head.text.push_str(if index == 0 { "  " } else { " " });
        push_text(&mut head, text, *part);
    }
    head
}

/// Cabecera con `parts` seguida del cuerpo común de la herramienta.
fn with_body(item: &ToolItem, parts: &[(Part, String)]) -> Vec<Line> {
    let mut lines = vec![head_line(item, parts)];
    lines.extend(body_lines(item));
    lines
}

/// Número de líneas no vacías de una salida.
fn count_nonempty(output: &str) -> usize {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// Línea final de recuento, «N matches» o «N files», con el recuento en `Count`.
fn count_line(count: usize, label: &str) -> Line {
    let text = format!("{count} {label}");
    let len = text.len();
    let mut line = Line::new(LineKind::ToolBody, text);
    line.spans.push(SpanSpec {
        start: 0,
        len,
        token: style_of(Part::Count),
    });
    line
}

/// Pieza de `bash`: el comando como resumen y la salida plegada como cuerpo.
struct ShellAtom;

impl ToolAtom for ShellAtom {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line> {
        let Some(command) = facts.primary.as_deref().and_then(first_line) else {
            return tool_lines(item);
        };
        with_body(item, &with_tail(command, Part::Command, None))
    }
}

/// Pieza de `view`: la ruta y, si lo hay, el rango de líneas.
struct FileAtom;

impl ToolAtom for FileAtom {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line> {
        let Some(path) = facts.primary.as_deref().and_then(first_line) else {
            return tool_lines(item);
        };
        let range = facts.range.map(|(start, end)| format!("L{start}-{end}"));
        with_body(item, &with_tail(path, Part::Path, range))
    }
}

/// Pieza de búsquedas (`grep`, `glob`). `label` nombra lo contado: «matches»
/// para coincidencias o «files» para archivos.
struct SearchAtom {
    label: &'static str,
}

impl ToolAtom for SearchAtom {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line> {
        let Some(pattern) = facts.primary.as_deref().and_then(first_line) else {
            return tool_lines(item);
        };
        let filter = facts
            .secondary
            .as_deref()
            .and_then(first_line)
            .map(|glob| format!("in {glob}"));
        let mut lines = with_body(item, &with_tail(pattern, Part::Pattern, filter));
        // Sin recuento explícito, se cuentan las líneas de la salida. Sin salida
        // aún (en curso) no hay recuento que mostrar.
        let count = facts
            .count
            .or_else(|| item.output.as_deref().map(count_nonempty));
        if let Some(count) = count {
            lines.push(count_line(count, self.label));
        }
        lines
    }
}

/// Cabecera de `edit`: la ruta y, tras dos espacios, las líneas añadidas y
/// borradas. La ruta se trunca dejando sitio a ese recuento.
fn edit_head(item: &ToolItem, path: &str, diff: &Diff) -> Line {
    let (added, removed) = (diff.added(), diff.removed());
    let stats = format!("+{added} -{removed}");
    // Se reservan los dos espacios separadores y el recuento.
    let reserved = stats.graphemes(true).count() + 2;
    let path = truncate_graphemes(path, SUMMARY_GRAPHEMES - reserved);
    let mut head = head_line(item, &[(Part::Path, path)]);
    head.text.push_str("  ");
    push_text(&mut head, &format!("+{added}"), Part::Added);
    head.text.push(' ');
    push_text(&mut head, &format!("-{removed}"), Part::Removed);
    head
}

/// Token de una línea de diff según su tipo.
fn diff_token(kind: DiffKind) -> StyleToken {
    match kind {
        DiffKind::Insert => StyleToken::DiffInsert,
        DiffKind::Delete => StyleToken::DiffDelete,
        DiffKind::Hunk => StyleToken::DiffHunk,
        DiffKind::Context => StyleToken::Muted,
    }
}

/// Línea de diff con su marca delante del texto: `+` o `-` en los cambios y un
/// espacio en el contexto. La cabecera de hunk va sin marca.
fn diff_line(line: &DiffLine) -> Line {
    let text = match line.kind {
        DiffKind::Hunk => line.text.clone(),
        DiffKind::Context => format!("  {}", line.text),
        DiffKind::Insert => format!("+ {}", line.text),
        DiffKind::Delete => format!("- {}", line.text),
    };
    let len = text.len();
    let mut out = Line::new(LineKind::ToolDiff(line.kind), text);
    out.spans.push(SpanSpec {
        start: 0,
        len,
        token: diff_token(line.kind),
    });
    out
}

/// Líneas del diff, acotadas a `DIFF_LINES` salvo que la llamada esté expandida.
fn diff_body(item: &ToolItem, diff: &Diff) -> Vec<Line> {
    let all = diff.lines();
    let shown = if item.expanded {
        all.len()
    } else {
        all.len().min(DIFF_LINES)
    };
    let mut lines: Vec<Line> = all[..shown].iter().map(diff_line).collect();
    if shown < all.len() {
        lines.push(more_line(all.len() - shown));
    }
    lines
}

/// Pieza de `edit`: la ruta con las líneas añadidas y borradas, y el diff como
/// cuerpo. Con error, la salida del error va tras el diff. Sin texto previo y
/// nuevo, o sin cambios, degrada a la genérica.
struct EditAtom;

impl ToolAtom for EditAtom {
    fn lines(&self, item: &ToolItem, facts: &ToolFacts) -> Vec<Line> {
        let (Some(path), Some(before), Some(after)) = (
            facts.primary.as_deref().and_then(first_line),
            facts.before.as_deref(),
            facts.after.as_deref(),
        ) else {
            return tool_lines(item);
        };
        let diff = Diff::new(before, after, DIFF_CONTEXT);
        if diff.is_empty() {
            return tool_lines(item);
        }
        let mut lines = vec![edit_head(item, path, &diff)];
        lines.extend(diff_body(item, &diff));
        if item.status() == ToolStatus::Error {
            lines.extend(body_lines(item));
        }
        lines
    }
}

static SHELL: ShellAtom = ShellAtom;
static FILE: FileAtom = FileAtom;
static EDIT: EditAtom = EditAtom;
static GREP: SearchAtom = SearchAtom { label: "matches" };
static GLOB: SearchAtom = SearchAtom { label: "files" };

/// Pieza registrada para `name`. Las herramientas sin pieza propia (y las MCP)
/// usan la genérica.
pub fn atom_for(name: &str) -> &'static dyn ToolAtom {
    match name {
        "bash" => &SHELL,
        "view" => &FILE,
        "edit" => &EDIT,
        "grep" => &GREP,
        "glob" => &GLOB,
        _ => &GENERIC,
    }
}

/// Líneas de la llamada según su pieza y sus hechos.
pub fn tool_lines_with(item: &ToolItem, facts: &ToolFacts) -> Vec<Line> {
    atom_for(&item.name).lines(item, facts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Diff, DiffKind, LineKind, StyleToken, ToolStatus};
    use unicode_segmentation::UnicodeSegmentation;

    /// Llamadas representativas de cada estado que `tool_lines` distingue.
    fn sample_items() -> Vec<ToolItem> {
        let mut pending = ToolItem::new("c0", "bash");
        pending.set_summary("ls");

        let mut running = ToolItem::new("c1", "bash");
        running.set_summary("cargo test");
        running.start();

        let mut short = ToolItem::new("c2", "bash");
        short.set_summary("echo hi");
        short.set_output("a\nb");

        let mut folded = ToolItem::new("c3", "bash");
        folded.set_result(false);
        folded.set_output("1\n2\n3\n4\n5");

        let mut expanded = ToolItem::new("c4", "bash");
        expanded.set_result(true);
        expanded.set_output("1\n2\n3\n4");
        expanded.toggle_expanded();

        let mut failed = ToolItem::new("c5", "edit");
        failed.set_summary("src/lib.rs");
        failed.set_result(true);
        failed.set_output("old text not found");

        let mut waiting = ToolItem::new("c6", "bash");
        waiting.set_summary("rm -rf build");
        waiting.start();
        waiting.await_permission();

        let mut canceled = ToolItem::new("c7", "view");
        canceled.set_summary("src/main.rs");
        canceled.start();
        canceled.cancel();

        vec![
            pending, running, short, folded, expanded, failed, waiting, canceled,
        ]
    }

    #[test]
    fn generic_atom_matches_the_current_tool_lines() {
        let facts = ToolFacts::default();
        for item in sample_items() {
            assert_eq!(
                tool_lines_with(&item, &facts),
                super::super::tool_lines(&item),
                "distinto para la llamada {} ({})",
                item.call_id,
                item.name
            );
        }
    }

    #[test]
    fn generic_atom_ignores_facts() {
        // Con hechos presentes la pieza genérica sigue pintando lo mismo: el
        // aspecto de las herramientas sin pieza propia no cambia.
        // Se llama a la genérica directamente: los ítems de ejemplo se llaman
        // `bash`, que ya tiene pieza propia, y lo que se prueba es la genérica.
        let facts = ToolFacts {
            primary: Some("ls -la".to_string()),
            count: Some(3),
            ..ToolFacts::default()
        };
        for item in sample_items() {
            assert_eq!(
                GENERIC.lines(&item, &facts),
                super::super::tool_lines(&item)
            );
        }
    }

    #[test]
    fn unknown_tool_falls_back_to_generic() {
        let mut item = ToolItem::new("c1", "mcp__github__create_issue");
        item.set_summary("{\"title\":\"x\"}");
        item.set_result(false);
        item.set_output("created");
        let lines = atom_for(&item.name).lines(&item, &ToolFacts::default());
        assert_eq!(lines, super::super::tool_lines(&item));
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Success));
    }

    #[test]
    fn facts_default_is_empty() {
        let facts = ToolFacts::default();
        assert_eq!(facts.primary, None);
        assert_eq!(facts.secondary, None);
        assert_eq!(facts.range, None);
        assert_eq!(facts.before, None);
        assert_eq!(facts.after, None);
        assert_eq!(facts.count, None);
        assert!(facts.items.is_empty());
    }

    /// Hechos con solo el `primary` dado.
    fn primary(text: &str) -> ToolFacts {
        ToolFacts {
            primary: Some(text.to_string()),
            ..ToolFacts::default()
        }
    }

    /// Resumen de la cabecera tras el nombre de la herramienta `name`.
    fn summary_of(lines: &[Line], name: &str) -> String {
        let head = lines[0].plain();
        head.strip_prefix(name)
            .and_then(|rest| rest.strip_prefix("  "))
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn bash_summary_is_the_command_truncated() {
        let item = ToolItem::new("c1", "bash");
        let lines = tool_lines_with(&item, &primary("cargo test -p batten"));
        assert_eq!(lines[0].plain(), "bash  cargo test -p batten");

        let lines = tool_lines_with(&item, &primary(&"x".repeat(200)));
        let summary = summary_of(&lines, "bash");
        assert_eq!(summary.chars().count(), 80);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn view_summary_includes_range() {
        let item = ToolItem::new("c1", "view");
        let facts = ToolFacts {
            range: Some((10, 29)),
            ..primary("src/lib.rs")
        };
        let lines = tool_lines_with(&item, &facts);
        assert_eq!(lines[0].plain(), "view  src/lib.rs L10-29");

        let lines = tool_lines_with(&item, &primary("src/lib.rs"));
        assert_eq!(lines[0].plain(), "view  src/lib.rs");
    }

    #[test]
    fn grep_shows_match_count_from_output() {
        let mut item = ToolItem::new("c1", "grep");
        item.set_result(false);
        item.set_output("a\nb\n\nc");
        let lines = tool_lines_with(&item, &primary("TODO"));
        let last = lines.last().unwrap();
        assert_eq!(last.plain(), "3 matches");
        assert_eq!(last.kind, LineKind::ToolBody);

        // Un recuento explícito de la pieza prevalece sobre las líneas de salida.
        let facts = ToolFacts {
            count: Some(7),
            ..primary("TODO")
        };
        let lines = tool_lines_with(&item, &facts);
        assert_eq!(lines.last().unwrap().plain(), "7 matches");
    }

    #[test]
    fn grep_summary_includes_glob_filter() {
        let item = ToolItem::new("c1", "grep");
        let facts = ToolFacts {
            secondary: Some("*.rs".to_string()),
            ..primary("TODO")
        };
        assert_eq!(
            tool_lines_with(&item, &facts)[0].plain(),
            "grep  TODO in *.rs"
        );
    }

    #[test]
    fn glob_shows_file_count() {
        let mut item = ToolItem::new("c1", "glob");
        item.set_result(false);
        item.set_output("a.rs\nb.rs\nc.rs");
        let lines = tool_lines_with(&item, &primary("crates/**/*.rs"));
        assert_eq!(lines[0].plain(), "glob  crates/**/*.rs");
        assert_eq!(lines.last().unwrap().plain(), "3 files");
    }

    #[test]
    fn running_tool_without_output_has_no_count() {
        for name in ["grep", "glob"] {
            let mut item = ToolItem::new("c1", name);
            item.start();
            let lines = tool_lines_with(&item, &primary("TODO"));
            assert_eq!(lines.len(), 1, "{name} en curso no debe mostrar recuento");
            assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Running));
        }
    }

    #[test]
    fn missing_primary_falls_back_to_generic() {
        let mut item = ToolItem::new("c1", "bash");
        item.set_summary("{\"command\":1}");
        item.set_output("a\nb\nc");
        let blank = ToolFacts {
            primary: Some("  \n ".to_string()),
            ..ToolFacts::default()
        };
        for facts in [ToolFacts::default(), blank] {
            assert_eq!(
                tool_lines_with(&item, &facts),
                super::super::tool_lines(&item)
            );
        }
    }

    #[test]
    fn long_command_with_newlines_and_wide_chars_is_truncated_by_graphemes() {
        let item = ToolItem::new("c1", "bash");
        // Primera línea de caracteres anchos (dos columnas cada uno); el resto se oculta.
        let wide = format!("{}\nsegunda línea", "日".repeat(120));
        let lines = tool_lines_with(&item, &primary(&wide));
        let summary = summary_of(&lines, "bash");
        assert_eq!(summary.graphemes(true).count(), 80);
        assert!(summary.ends_with('…'));
        assert!(!summary.contains("segunda"));

        // Un grafema compuesto (e con acento combinado) cuenta como uno solo.
        let combining = "e\u{301}".repeat(100);
        let lines = tool_lines_with(&item, &primary(&combining));
        assert_eq!(
            summary_of(&lines, "bash"),
            format!("{}…", "e\u{301}".repeat(79))
        );
    }

    #[test]
    fn error_status_keeps_the_output_body() {
        let mut item = ToolItem::new("c1", "bash");
        item.set_result(true);
        item.set_output("x\ny\nz");
        let lines = tool_lines_with(&item, &primary("false"));
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Error));
        let body = lines
            .iter()
            .filter(|l| l.kind == LineKind::ToolBody)
            .count();
        assert_eq!(body, 2);
        assert_eq!(lines.last().unwrap().kind, LineKind::ToolMore);
    }

    /// Hechos de una edición de `path` de `before` a `after`.
    fn edit_facts(path: &str, before: &str, after: &str) -> ToolFacts {
        ToolFacts {
            primary: Some(path.to_string()),
            before: Some(before.to_string()),
            after: Some(after.to_string()),
            ..ToolFacts::default()
        }
    }

    /// Llamada `edit` terminada con éxito.
    fn edit_item() -> ToolItem {
        let mut item = ToolItem::new("c1", "edit");
        item.set_result(false);
        item.set_output("edit applied");
        item
    }

    fn diff_count(lines: &[Line]) -> usize {
        lines
            .iter()
            .filter(|l| matches!(l.kind, LineKind::ToolDiff(_)))
            .count()
    }

    #[test]
    fn edit_summary_shows_path_and_counts() {
        let item = edit_item();
        let facts = edit_facts("src/lib.rs", "a\nb\nc\n", "a\nB\nc\nd\n");
        let lines = tool_lines_with(&item, &facts);
        assert_eq!(lines[0].plain(), "edit  src/lib.rs  +2 -1");
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Success));
    }

    #[test]
    fn edit_body_has_diff_lines_with_kinds() {
        let item = edit_item();
        let facts = edit_facts("a.txt", "a\nb\n", "a\nB\n");
        let lines = tool_lines_with(&item, &facts);
        let body: Vec<(LineKind, &str)> = lines[1..].iter().map(|l| (l.kind, l.plain())).collect();
        assert_eq!(
            body,
            vec![
                (LineKind::ToolDiff(DiffKind::Hunk), "@@ -1,2 +1,2 @@"),
                (LineKind::ToolDiff(DiffKind::Context), "  a"),
                (LineKind::ToolDiff(DiffKind::Delete), "- b"),
                (LineKind::ToolDiff(DiffKind::Insert), "+ B"),
            ]
        );
        assert_eq!(lines[3].spans[0].token, StyleToken::DiffDelete);
        assert_eq!(lines[4].spans[0].token, StyleToken::DiffInsert);
    }

    #[test]
    fn edit_diff_is_capped_and_expandable() {
        let before: String = (0..30).map(|i| format!("viejo {i}\n")).collect();
        let after: String = (0..30).map(|i| format!("nuevo {i}\n")).collect();
        let total = Diff::new(&before, &after, DIFF_CONTEXT).lines().len();
        assert!(
            total > DIFF_LINES + 1,
            "precondición: el diff supera el tope"
        );
        let facts = edit_facts("a.txt", &before, &after);

        let mut item = edit_item();
        let lines = tool_lines_with(&item, &facts);
        assert_eq!(diff_count(&lines), DIFF_LINES);
        let more = lines.last().unwrap();
        assert_eq!(more.kind, LineKind::ToolMore);
        assert_eq!(
            more.plain(),
            format!("… ({} lines hidden) [Ctrl+O to expand]", total - DIFF_LINES)
        );

        item.toggle_expanded();
        let lines = tool_lines_with(&item, &facts);
        assert_eq!(diff_count(&lines), total);
        assert!(lines.iter().all(|l| l.kind != LineKind::ToolMore));
    }

    #[test]
    fn edit_without_before_after_falls_back() {
        let mut item = edit_item();
        item.set_summary("src/lib.rs");
        let only_path = primary("src/lib.rs");
        let without_after = ToolFacts {
            before: Some("x".to_string()),
            ..primary("src/lib.rs")
        };
        for facts in [only_path, without_after] {
            assert_eq!(tool_lines_with(&item, &facts), tool_lines(&item));
        }
    }

    #[test]
    fn empty_diff_falls_back() {
        let item = edit_item();
        let facts = edit_facts("a.txt", "igual\n", "igual\n");
        assert_eq!(tool_lines_with(&item, &facts), tool_lines(&item));
    }

    #[test]
    fn edit_error_keeps_error_output_after_the_diff() {
        let mut item = ToolItem::new("c1", "edit");
        item.set_result(true);
        item.set_output("old text not found");
        let lines = tool_lines_with(&item, &edit_facts("a.txt", "a\n", "b\n"));
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Error));
        let last = lines.last().unwrap();
        assert_eq!(last.kind, LineKind::ToolBody);
        assert_eq!(last.plain(), "old text not found");
        assert_eq!(diff_count(&lines[..lines.len() - 1]), 3);
    }

    #[test]
    fn edit_running_shows_diff_from_input() {
        let mut item = ToolItem::new("c1", "edit");
        item.start();
        let lines = tool_lines_with(&item, &edit_facts("a.txt", "a\n", "b\n"));
        assert_eq!(lines[0].kind, LineKind::ToolHead(ToolStatus::Running));
        assert!(lines
            .iter()
            .any(|l| l.kind == LineKind::ToolDiff(DiffKind::Insert)));
    }
}
