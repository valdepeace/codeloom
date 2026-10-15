//! Logo paramétrico: fuente de bloques 3x5 y wordmark.

/// Filas de la fuente de bloques.
pub const FONT_ROWS: usize = 5;

use super::gradient::{color_of, lerp_color};
use super::Theme;
use crate::state::StyleToken;
use ratatui::style::Style;
use ratatui::text::{Line, Span};

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

/// Estilo por columna: degradado de `AccentFrom` (t=0) a `AccentTo` (t=1).
fn gradient_style(col: usize, width: usize, theme: &Theme) -> Style {
    let t = if width <= 1 {
        0.0
    } else {
        col as f32 / (width - 1) as f32
    };
    let from = color_of(theme.style(StyleToken::AccentFrom));
    let to = color_of(theme.style(StyleToken::AccentTo));
    Style::default().fg(lerp_color(from, to, t))
}

/// Wordmark de bloques: `FONT_ROWS` líneas con el degradado por columna. Los
/// huecos van sin estilo.
pub fn render(wordmark: &Wordmark, theme: &Theme) -> Vec<Line<'static>> {
    let width = wordmark.width();
    wordmark
        .rows()
        .iter()
        .map(|row| {
            let spans: Vec<Span> = row
                .chars()
                .enumerate()
                .map(|(col, ch)| {
                    if ch == ' ' {
                        Span::raw(" ")
                    } else {
                        Span::styled(ch.to_string(), gradient_style(col, width, theme))
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Fallback de una línea: nombre (estilo `Heading`) + `╱` (estilo `LogoField`)
/// hasta `width`. Con `width` 0 devuelve solo el nombre.
pub fn compact(name: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let used = name.chars().count();
    let fill = width.saturating_sub(used);
    let mut spans = vec![Span::styled(
        name.to_string(),
        theme.style(StyleToken::Heading),
    )];
    if fill > 0 {
        spans.push(Span::styled(
            theme.glyphs.field.to_string().repeat(fill),
            theme.style(StyleToken::LogoField),
        ));
    }
    vec![Line::from(spans)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Theme;
    use ratatui::text::Line;

    fn text_of(lines: &[Line]) -> String {
        lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect()
    }

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

    #[test]
    fn render_draws_font_rows_of_blocks() {
        let lines = render(&Wordmark::new("A"), &Theme::test());
        assert_eq!(lines.len(), FONT_ROWS);
        assert!(text_of(&lines).contains('█'));
    }

    #[test]
    fn compact_shows_the_name_and_field_fill() {
        let lines = compact("batten", 20, &Theme::test());
        let text = text_of(&lines);
        assert!(text.contains("batten"));
        assert!(text.contains('╱'));
    }

    #[test]
    fn compact_with_zero_width_is_just_the_name() {
        let lines = compact("batten", 0, &Theme::test());
        assert_eq!(text_of(&lines), "batten");
    }
}
