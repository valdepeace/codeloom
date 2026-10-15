//! Selección de líneas del transcript, en coordenadas del documento (índices
//! de línea), independiente del scroll. El widget resalta; la app copia.

use super::transcript::Transcript;
use std::ops::RangeInclusive;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    anchor: usize,
    head: usize,
}

impl Selection {
    /// Selección de una sola línea.
    pub fn at(line: usize) -> Self {
        Self {
            anchor: line,
            head: line,
        }
    }

    /// Extremo móvil (el que siguen las teclas).
    pub fn head(&self) -> usize {
        self.head
    }

    /// Líneas seleccionadas, de menor a mayor.
    pub fn range(&self) -> RangeInclusive<usize> {
        self.anchor.min(self.head)..=self.anchor.max(self.head)
    }

    pub fn contains(&self, line: usize) -> bool {
        self.range().contains(&line)
    }

    /// Mueve el extremo `delta` líneas, acotado a `[0, len)`.
    pub fn extend(&mut self, delta: isize, len: usize) {
        let max = len.saturating_sub(1);
        self.head = self.head.saturating_add_signed(delta).min(max);
    }

    /// Texto plano de las líneas seleccionadas, separadas por `\n`.
    pub fn text(&self, transcript: &Transcript) -> String {
        self.range()
            .filter_map(|i| transcript.line(i))
            .map(|l| l.plain())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Line, LineKind};

    fn three() -> Transcript {
        let mut t = Transcript::new();
        for s in ["uno", "dos", "tres"] {
            t.push(Line::new(LineKind::Assistant, s));
        }
        t
    }

    #[test]
    fn range_is_ordered_and_text_joined() {
        let mut s = Selection::at(2);
        s.extend(-1, 3);
        assert_eq!(s.range(), 1..=2);
        assert!(s.contains(1) && !s.contains(0));
        assert_eq!(s.text(&three()), "dos\ntres");
    }

    #[test]
    fn extend_is_clamped() {
        let mut s = Selection::at(1);
        s.extend(-10, 3);
        assert_eq!(s.head(), 0);
        s.extend(10, 3);
        assert_eq!(s.head(), 2);
        let mut e = Selection::at(0);
        e.extend(5, 0);
        assert_eq!(e.head(), 0);
    }
}
