//! Logo paramétrico: fuente de bloques 3x5 y wordmark.

/// Filas de la fuente de bloques.
pub const FONT_ROWS: usize = 5;

/// Arte de bloque para un carácter (3 columnas x 5 filas). Un carácter fuera de
/// la tabla devuelve el glifo de espacio.
fn glyph(c: char) -> [&'static str; FONT_ROWS] {
    match c {
        'A' => [" █ ", "█ █", "███", "█ █", "█ █"],
        'B' => ["██ ", "█ █", "██ ", "█ █", "██ "],
        'C' => [" ██", "█  ", "█  ", "█  ", " ██"],
        'D' => ["██ ", "█ █", "█ █", "█ █", "██ "],
        'E' => ["███", "█  ", "██ ", "█  ", "███"],
        'F' => ["███", "█  ", "██ ", "█  ", "█  "],
        'G' => [" ██", "█  ", "█ █", "█ █", " ██"],
        'H' => ["█ █", "█ █", "███", "█ █", "█ █"],
        'I' => ["███", " █ ", " █ ", " █ ", "███"],
        'J' => ["  █", "  █", "  █", "█ █", " █ "],
        'K' => ["█ █", "█ █", "██ ", "█ █", "█ █"],
        'L' => ["█  ", "█  ", "█  ", "█  ", "███"],
        'M' => ["█ █", "███", "█ █", "█ █", "█ █"],
        'N' => ["█ █", "███", "███", "█ █", "█ █"],
        'O' => [" █ ", "█ █", "█ █", "█ █", " █ "],
        'P' => ["██ ", "█ █", "██ ", "█  ", "█  "],
        'Q' => [" █ ", "█ █", "█ █", "███", " ██"],
        'R' => ["██ ", "█ █", "██ ", "█ █", "█ █"],
        'S' => [" ██", "█  ", " █ ", "  █", "██ "],
        'T' => ["███", " █ ", " █ ", " █ ", " █ "],
        'U' => ["█ █", "█ █", "█ █", "█ █", " █ "],
        'V' => ["█ █", "█ █", "█ █", " █ ", " █ "],
        'W' => ["█ █", "█ █", "███", "███", "█ █"],
        'X' => ["█ █", "█ █", " █ ", "█ █", "█ █"],
        'Y' => ["█ █", "█ █", " █ ", " █ ", " █ "],
        'Z' => ["███", "  █", " █ ", "█  ", "███"],
        '0' => [" █ ", "█ █", "█ █", "█ █", " █ "],
        '1' => [" █ ", "██ ", " █ ", " █ ", "███"],
        '2' => ["██ ", "  █", " █ ", "█  ", "███"],
        '3' => ["██ ", "  █", " █ ", "  █", "██ "],
        '4' => ["█ █", "█ █", "███", "  █", "  █"],
        '5' => ["███", "█  ", "██ ", "  █", "██ "],
        '6' => [" ██", "█  ", "██ ", "█ █", " █ "],
        '7' => ["███", "  █", " █ ", " █ ", " █ "],
        '8' => [" █ ", "█ █", " █ ", "█ █", " █ "],
        '9' => [" █ ", "█ █", " ██", "  █", "██ "],
        ' ' => ["   ", "   ", "   ", "   ", "   "],
        '.' => ["   ", "   ", "   ", "   ", " █ "],
        '-' => ["   ", "   ", "███", "   ", "   "],
        '_' => ["   ", "   ", "   ", "   ", "███"],
        '/' => ["  █", "  █", " █ ", "█  ", "█  "],
        ':' => ["   ", " █ ", "   ", " █ ", "   "],
        _ => glyph(' '),
    }
}

/// Wordmark de bloques construido a partir de un nombre (denso, sin separación
/// entre glifos; cada glifo ocupa 3 columnas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wordmark {
    rows: Vec<String>,
}

impl Wordmark {
    pub fn new(name: &str) -> Self {
        let mut rows: Vec<String> = vec![String::new(); FONT_ROWS];
        for c in name.chars().flat_map(char::to_uppercase) {
            let g = glyph(c);
            for (r, row) in rows.iter_mut().enumerate() {
                row.push_str(g[r]);
            }
        }
        Self { rows }
    }

    pub fn rows(&self) -> &[String] {
        &self.rows
    }

    pub fn width(&self) -> usize {
        self.rows.first().map_or(0, |r| r.chars().count())
    }

    pub fn height(&self) -> usize {
        FONT_ROWS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_char_has_font_rows_of_three_columns() {
        for c in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 .-/_".chars() {
            let g = glyph(c);
            assert_eq!(g.len(), FONT_ROWS, "glyph {c:?} rows");
            for row in g {
                assert_eq!(row.chars().count(), 3, "glyph {c:?} row width");
            }
        }
    }

    #[test]
    fn unknown_char_falls_back_to_space() {
        assert_eq!(glyph('?'), glyph(' '));
        assert_eq!(glyph('é'), glyph(' '));
        assert_eq!(glyph('🚀'), glyph(' '));
    }

    #[test]
    fn wordmark_of_a_known_name() {
        let wm = Wordmark::new("AB");
        assert_eq!(wm.height(), FONT_ROWS);
        assert_eq!(wm.width(), 6);
        assert_eq!(
            wm.rows(),
            [" █ ██ ", "█ ██ █", "█████ ", "█ ██ █", "█ ███ ",]
        );
    }

    #[test]
    fn lowercase_is_upcased() {
        assert_eq!(Wordmark::new("ab").rows(), Wordmark::new("AB").rows());
    }

    #[test]
    fn empty_name_has_no_width() {
        let wm = Wordmark::new("");
        assert_eq!(wm.width(), 0);
        assert_eq!(wm.height(), FONT_ROWS);
    }
}
