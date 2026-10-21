//! Selección de texto por caracteres en el transcript. Las posiciones están en
//! coordenadas de documento `(fila, columna)` sobre el diseño de filas del
//! `Layout`, así que no dependen del scroll. Estado puro: ni ratatui ni crossterm.

/// Posición en el documento: fila del `Layout` y columna dentro del texto. El
/// orden derivado (fila primero, luego columna) es el orden de lectura.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellPos {
    pub row: usize,
    pub col: u16,
}

/// Selección de texto con ancla y cabeza. El ancla es donde empezó el arrastre
/// y la cabeza la posición actual; el orden de lectura se obtiene al consultar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextSelection {
    anchor: CellPos,
    head: CellPos,
}

impl TextSelection {
    /// Selección vacía en `at`: el ancla y la cabeza coinciden.
    pub fn new(at: CellPos) -> Self {
        Self {
            anchor: at,
            head: at,
        }
    }

    /// Mueve la cabeza; el ancla no cambia.
    pub fn set_head(&mut self, head: CellPos) {
        self.head = head;
    }

    /// Verdadero si ancla y cabeza coinciden: no hay nada que copiar.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Extremos `(inicio, fin)` en orden de lectura, sin importar hacia dónde
    /// se arrastró.
    pub fn ordered(&self) -> (CellPos, CellPos) {
        if self.head < self.anchor {
            (self.head, self.anchor)
        } else {
            (self.anchor, self.head)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(row: usize, col: u16) -> CellPos {
        CellPos { row, col }
    }

    #[test]
    fn new_selection_is_empty() {
        let sel = TextSelection::new(pos(4, 2));
        assert!(sel.is_empty());
        assert_eq!(sel.ordered(), (pos(4, 2), pos(4, 2)));
    }

    #[test]
    fn ordered_swaps_when_head_precedes_anchor() {
        let mut sel = TextSelection::new(pos(5, 7));
        sel.set_head(pos(2, 3));
        assert!(!sel.is_empty());
        assert_eq!(sel.ordered(), (pos(2, 3), pos(5, 7)));
        // En la misma fila decide la columna.
        let mut same = TextSelection::new(pos(1, 9));
        same.set_head(pos(1, 4));
        assert_eq!(same.ordered(), (pos(1, 4), pos(1, 9)));
    }
}
