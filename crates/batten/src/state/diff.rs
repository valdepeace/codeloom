//! Diff unificado por líneas entre dos textos que suministra la app
//! (batten no lee ficheros ni ejecuta Git).

use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// Cabecera `@@ -a,b +c,d @@`.
    Hunk,
    Context,
    Insert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// Número de línea en el texto viejo (1-based); `None` en Insert y Hunk.
    pub old_no: Option<usize>,
    /// Número de línea en el texto nuevo (1-based); `None` en Delete y Hunk.
    pub new_no: Option<usize>,
    /// Texto sin salto de línea final.
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    lines: Vec<DiffLine>,
}

impl Diff {
    /// Diff por líneas agrupado en hunks con `context` líneas de contexto.
    pub fn new(before: &str, after: &str, context: usize) -> Self {
        if before == after {
            return Self::default();
        }
        let td = TextDiff::from_lines(before, after);
        let mut lines = Vec::new();
        for group in td.grouped_ops(context) {
            let (Some(first), Some(last)) = (group.first(), group.last()) else {
                continue;
            };
            let (os, oe) = (first.old_range().start, last.old_range().end);
            let (ns, ne) = (first.new_range().start, last.new_range().end);
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                old_no: None,
                new_no: None,
                text: format!("@@ -{},{} +{},{} @@", os + 1, oe - os, ns + 1, ne - ns),
            });
            for op in &group {
                for ch in td.iter_changes(op) {
                    let kind = match ch.tag() {
                        ChangeTag::Equal => DiffKind::Context,
                        ChangeTag::Delete => DiffKind::Delete,
                        ChangeTag::Insert => DiffKind::Insert,
                    };
                    lines.push(DiffLine {
                        kind,
                        old_no: ch.old_index().map(|i| i + 1),
                        new_no: ch.new_index().map(|i| i + 1),
                        text: ch.value().trim_end_matches(['\n', '\r']).to_string(),
                    });
                }
            }
        }
        Self { lines }
    }

    pub fn lines(&self) -> &[DiffLine] {
        &self.lines
    }

    pub fn added(&self) -> usize {
        self.count(DiffKind::Insert)
    }

    pub fn removed(&self) -> usize {
        self.count(DiffKind::Delete)
    }

    /// `true` si no hay cambios.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn count(&self, kind: DiffKind) -> usize {
        self.lines.iter().filter(|l| l.kind == kind).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DiffKind::*;

    fn kinds(d: &Diff) -> Vec<DiffKind> {
        d.lines().iter().map(|l| l.kind).collect()
    }

    #[test]
    fn equal_texts_are_empty() {
        assert!(Diff::new("a\n", "a\n", 3).is_empty());
    }

    #[test]
    fn one_changed_line() {
        let d = Diff::new("a\nb\nc\n", "a\nB\nc\n", 1);
        assert_eq!(kinds(&d), vec![Hunk, Context, Delete, Insert, Context]);
        assert_eq!(d.lines()[0].text, "@@ -1,3 +1,3 @@");
        assert_eq!(
            (d.lines()[1].old_no, d.lines()[1].new_no),
            (Some(1), Some(1))
        );
        assert_eq!((d.lines()[2].old_no, d.lines()[2].new_no), (Some(2), None));
        assert_eq!((d.lines()[3].old_no, d.lines()[3].new_no), (None, Some(2)));
        assert_eq!(d.lines()[3].text, "B");
        assert_eq!((d.added(), d.removed()), (1, 1));
    }

    #[test]
    fn context_is_trimmed() {
        let before: String = (1..=10).map(|i| format!("{i}\n")).collect();
        let after = before.replace("5\n", "cinco\n");
        let d = Diff::new(&before, &after, 1);
        assert_eq!(kinds(&d), vec![Hunk, Context, Delete, Insert, Context]);
        assert_eq!(d.lines()[0].text, "@@ -4,3 +4,3 @@");
        assert_eq!(d.lines()[1].old_no, Some(4));
    }

    #[test]
    fn without_trailing_newline() {
        let d = Diff::new("x", "y", 3);
        assert_eq!(kinds(&d), vec![Hunk, Delete, Insert]);
        assert_eq!(d.lines()[1].text, "x");
        assert_eq!(d.lines()[2].text, "y");
    }
}
