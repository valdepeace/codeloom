//! Layout de filas visuales del composer (wrap por grafemas). Puro.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Fila visual del composer: un rango de bytes del texto, sin el `\n` final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisualRow {
    /// Byte inicial de la fila dentro del texto.
    pub start: usize,
    /// Byte final (exclusivo) de la fila dentro del texto.
    pub end: usize,
    /// Índice de la línea lógica a la que pertenece la fila.
    pub line: usize,
    /// Indica si es la primera fila de su línea lógica.
    pub first: bool,
}

/// Resultado del layout: filas visuales y posición del cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerLayout {
    pub rows: Vec<VisualRow>,
    /// `(fila visual, columna en celdas)`.
    pub cursor: (usize, usize),
}

/// Parte `text` en filas visuales ajustadas por grafemas y localiza el cursor.
///
/// `cursor_graphemes` cuenta grafemas como `Composer::cursor()`, de modo que el
/// salto de línea cuenta como uno. `first_width` se aplica a la línea lógica 0 y
/// `rest_width` al resto; ambos se fuerzan a un mínimo de 1 celda.
pub fn layout(
    text: &str,
    cursor_graphemes: usize,
    first_width: usize,
    rest_width: usize,
) -> ComposerLayout {
    let mut rows: Vec<VisualRow> = Vec::new();
    let mut cursor: Option<(usize, usize)> = None;
    let mut base = 0usize;
    let mut seen = 0usize;

    for (idx, line) in text.split('\n').enumerate() {
        let line_width = if idx == 0 { first_width } else { rest_width };
        let w = line_width.max(1);
        let count = line.graphemes(true).count();
        let mut row_start = 0usize;
        let mut row_width = 0usize;
        let mut first = true;

        for (i, (off, g)) in line.grapheme_indices(true).enumerate() {
            let gw = UnicodeWidthStr::width(g);
            // Un grafema que no cabe en la fila abre otra; si la fila está vacía
            // se acepta igualmente para garantizar progreso.
            if row_width + gw > w && row_width > 0 {
                rows.push(VisualRow {
                    start: base + row_start,
                    end: base + off,
                    line: idx,
                    first,
                });
                row_start = off;
                row_width = 0;
                first = false;
            }
            // El cursor precede al grafema i; tras un posible salto cae en la fila nueva.
            if cursor.is_none() && seen + i == cursor_graphemes {
                cursor = Some((rows.len(), row_width));
            }
            row_width += gw;
        }

        if cursor.is_none() && seen + count == cursor_graphemes {
            if row_width >= w {
                // El cursor al final de una fila llena baja a una fila propia.
                rows.push(VisualRow {
                    start: base + row_start,
                    end: base + line.len(),
                    line: idx,
                    first,
                });
                let end_row = base + line.len();
                rows.push(VisualRow {
                    start: end_row,
                    end: end_row,
                    line: idx,
                    first: false,
                });
                cursor = Some((rows.len() - 1, 0));
            } else {
                cursor = Some((rows.len(), row_width));
                rows.push(VisualRow {
                    start: base + row_start,
                    end: base + line.len(),
                    line: idx,
                    first,
                });
            }
        } else {
            rows.push(VisualRow {
                start: base + row_start,
                end: base + line.len(),
                line: idx,
                first,
            });
        }

        base += line.len() + 1;
        seen += count + 1;
    }

    let cursor = cursor.unwrap_or_else(|| {
        // Cursor fuera del texto: se clampa al final de la última fila.
        let last = rows
            .last()
            .expect("split siempre produce al menos una línea");
        (
            rows.len() - 1,
            UnicodeWidthStr::width(&text[last.start..last.end]),
        )
    });

    ComposerLayout { rows, cursor }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(t: &'a str, l: &ComposerLayout) -> Vec<&'a str> {
        l.rows.iter().map(|r| &t[r.start..r.end]).collect()
    }

    #[test]
    fn empty_text_has_one_row_and_cursor_at_origin() {
        let l = layout("", 0, 10, 10);
        assert_eq!(l.rows.len(), 1);
        assert_eq!(l.cursor, (0, 0));
    }

    #[test]
    fn logical_lines_become_rows() {
        let t = "ab\ncd";
        let l = layout(t, 4, 10, 10); // tras la 'c'
        assert_eq!(texts(t, &l), vec!["ab", "cd"]);
        assert_eq!(l.rows[1].line, 1);
        assert!(l.rows[1].first);
        assert_eq!(l.cursor, (1, 1));
    }

    #[test]
    fn soft_wrap_splits_long_lines() {
        let t = "abcdefgh";
        let l = layout(t, 8, 3, 3);
        assert_eq!(texts(t, &l), vec!["abc", "def", "gh"]);
        assert!(l.rows[0].first && !l.rows[1].first && !l.rows[2].first);
        assert!(l.rows.iter().all(|r| r.line == 0));
        assert_eq!(l.cursor, (2, 2));
    }

    #[test]
    fn cursor_on_a_wrap_boundary_belongs_to_the_next_row() {
        let t = "abcdef";
        let l = layout(t, 3, 3, 3); // entre 'c' y 'd'
        assert_eq!(l.cursor, (1, 0));
    }

    #[test]
    fn cursor_at_end_of_full_row_gets_its_own_row() {
        let t = "abc";
        let l = layout(t, 3, 3, 3);
        assert_eq!(texts(t, &l), vec!["abc", ""]);
        assert_eq!(l.cursor, (1, 0));
    }

    #[test]
    fn first_and_rest_widths_apply_per_logical_line() {
        let t = "abcd\nabcd";
        let l = layout(t, 0, 2, 4);
        assert_eq!(texts(t, &l), vec!["ab", "cd", "abcd"]);
    }

    #[test]
    fn wide_graphemes_wrap_without_splitting() {
        let t = "日本語"; // 3 grafemas de 2 celdas
        let l = layout(t, 3, 5, 5);
        assert_eq!(texts(t, &l), vec!["日本", "語"]);
        assert_eq!(l.cursor, (1, 2));
    }

    #[test]
    fn combining_marks_stay_with_their_base() {
        let t = "e\u{301}e\u{301}e\u{301}"; // 3 grafemas de 1 celda
        let l = layout(t, 2, 2, 2);
        assert_eq!(texts(t, &l), vec!["e\u{301}e\u{301}", "e\u{301}"]);
        assert_eq!(l.cursor, (1, 0));
    }

    #[test]
    fn tiny_widths_do_not_panic_or_loop() {
        for w in [0usize, 1] {
            let t = "日本a\nb";
            let l = layout(t, 4, w, w);
            assert!(!l.rows.is_empty());
            assert!(l.cursor.0 < l.rows.len());
        }
    }

    #[test]
    fn cursor_past_the_end_is_clamped_to_the_last_row() {
        let t = "ab";
        let l = layout(t, 99, 10, 10);
        assert_eq!(l.cursor, (0, 2));
    }
}
