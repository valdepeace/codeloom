//! Logo paramétrico: fuente de bloques 3x5 y wordmark.

/// Filas de la fuente de bloques.
pub const FONT_ROWS: usize = 5;

use super::gradient::{color_of, fg, lerp_color};
use super::Theme;
use crate::state::StyleToken;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Mindfork brand mark, drawn from its 16×16 pixel grid with half-block cells.
/// Colors and geometry follow `mindfork-rs/src/widgets/logo.rs`.
pub fn mindfork_glyph() -> Vec<Line<'static>> {
    const RECTANGLES: [(u8, u8, u8, u8, Color); 5] = [
        (7, 2, 2, 12, Color::Rgb(0xc2, 0x5a, 0x27)),
        (11, 2, 2, 5, Color::Rgb(0x5c, 0x63, 0x70)),
        (9, 5, 2, 2, Color::Rgb(0x5c, 0x63, 0x70)),
        (3, 7, 2, 5, Color::Rgb(0x5c, 0x63, 0x70)),
        (5, 10, 2, 2, Color::Rgb(0x5c, 0x63, 0x70)),
    ];
    let pixel = |x: u8, y: u8| {
        RECTANGLES
            .iter()
            .find(|(rx, ry, rw, rh, _)| x >= *rx && x < rx + rw && y >= *ry && y < ry + rh)
            .map(|(_, _, _, _, color)| *color)
    };

    (0..6)
        .map(|row| {
            let top = 2 + row * 2;
            let spans: Vec<Span<'static>> = (3..13)
                .map(|x| match (pixel(x, top), pixel(x, top + 1)) {
                    (Some(fg), Some(bg)) if fg == bg => Span::styled("█", Style::default().fg(fg)),
                    (Some(fg), Some(bg)) => Span::styled("▀", Style::default().fg(fg).bg(bg)),
                    (Some(fg), None) => Span::styled("▀", Style::default().fg(fg)),
                    (None, Some(fg)) => Span::styled("▄", Style::default().fg(fg)),
                    (None, None) => Span::raw(" "),
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Wordmark pixelado `mindfork` de Mindfork. `mind` sigue el color de texto;
/// `fork` conserva el naranja de marca del upstream.
pub fn mindfork_wordmark(mind: Color) -> Vec<Line<'static>> {
    const LETTERS: [(&str, &[&str]); 8] = [
        (
            "m",
            &[
                "........", "........", "########", "########", "##.##.##", "##.##.##", "##.##.##",
                "##.##.##",
            ],
        ),
        ("i", &["##", "##", "..", "##", "##", "##", "##", "##"]),
        (
            "n",
            &[
                "......", "......", "######", "######", "##..##", "##..##", "##..##", "##..##",
            ],
        ),
        (
            "d",
            &[
                "....##", "....##", "######", "######", "##..##", "##..##", "######", "######",
            ],
        ),
        (
            "f",
            &[
                "..####", "..##..", "######", "######", "..##..", "..##..", "..##..", "..##..",
            ],
        ),
        (
            "o",
            &[
                "......", "......", "######", "######", "##..##", "##..##", "######", "######",
            ],
        ),
        (
            "r",
            &[
                ".....", ".....", "#####", "#####", "##...", "##...", "##...", "##...",
            ],
        ),
        (
            "k",
            &[
                "##....", "##....", "##.###", "##.###", "####..", "####..", "##.###", "##.###",
            ],
        ),
    ];
    let orange = Color::Rgb(0xc2, 0x5a, 0x27);
    let mut columns: Vec<(Color, [bool; 8])> = Vec::new();
    for (index, (_, rows)) in LETTERS.iter().enumerate() {
        if index > 0 {
            columns.push((mind, [false; 8]));
        }
        for x in 0..rows[0].len() {
            let color = if index < 4 { mind } else { orange };
            columns.push((
                color,
                std::array::from_fn(|y| rows[y].as_bytes()[x] == b'#'),
            ));
        }
    }
    (0..4)
        .map(|row| {
            Line::from(
                columns
                    .iter()
                    .map(|(color, px)| match (px[row * 2], px[row * 2 + 1]) {
                        (true, true) => Span::styled("█", Style::default().fg(*color)),
                        (true, false) => Span::styled("▀", Style::default().fg(*color)),
                        (false, true) => Span::styled("▄", Style::default().fg(*color)),
                        (false, false) => Span::raw(" "),
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Lockup horizontal Mindfork: glyph original a la izquierda y wordmark alineado
/// con la línea de base del SVG de marca.
pub fn mindfork_lockup(mind: Color) -> Vec<Line<'static>> {
    let word = mindfork_wordmark(mind);
    mindfork_glyph()
        .into_iter()
        .enumerate()
        .map(|(row, glyph)| {
            let mut spans = glyph.spans;
            if (1..5).contains(&row) {
                spans.extend([Span::raw("   ")]);
                spans.extend(word[row - 1].spans.iter().cloned());
            }
            Line::from(spans)
        })
        .collect()
}

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
    fg(theme, lerp_color(from, to, t))
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

/// Wordmark de puntos 5×7 de Batten, con el degradado habitual del tema.
pub fn batten_dot_wordmark(theme: &Theme) -> Vec<Line<'static>> {
    const LETTERS: [&[&str; 7]; 6] = [
        &[
            "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
        ],
        &[
            ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
        ],
        &[
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
        &[
            "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
        ],
        &[
            "#####", "#....", "#....", "####.", "#....", "#....", "#####",
        ],
        &[
            "#...#", "##..#", "##..#", "#.#.#", "#..##", "#..##", "#...#",
        ],
    ];
    const WIDTH: usize = 6 * 5 + 5;
    (0..7)
        .map(|row| {
            let mut spans = Vec::with_capacity(WIDTH);
            for (letter, bitmap) in LETTERS.iter().enumerate() {
                for (column, pixel) in bitmap[row].chars().enumerate() {
                    if pixel == '#' {
                        let x = letter * 6 + column;
                        spans.push(Span::styled("●", gradient_style(x, WIDTH, theme)));
                    } else {
                        spans.push(Span::raw(" "));
                    }
                }
                if letter + 1 < LETTERS.len() {
                    spans.push(Span::raw(" "));
                }
            }
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

/// Wordmark terminal de Martty. El arte ASCII deriva del logo MIT de Martty;
/// consulta `docs/references/loom-ui-comparison/third-party-notices.md`.
pub fn martty_wordmark(theme: &Theme, width: u16) -> Vec<Line<'static>> {
    const LARGE: [&str; 6] = [
        "███╗   ███╗ █████╗ ██████╗ ████████╗████████╗██╗   ██╗",
        "████╗ ████║██╔══██╗██╔══██╗╚══██╔══╝╚══██╔══╝╚██╗ ██╔╝",
        "██╔████╔██║███████║██████╔╝   ██║      ██║    ╚████╔╝ ",
        "██║╚██╔╝██║██╔══██║██╔══██╗   ██║      ██║     ╚██╔╝  ",
        "██║ ╚═╝ ██║██║  ██║██║  ██║   ██║      ██║      ██║   ",
        "╚═╝     ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝   ╚═╝      ╚═╝      ╚═╝   ",
    ];
    const SMALL: [&str; 5] = [
        " __  __          _   _        ",
        "|  \\/  |__ _ _ _| |_| |_ _  _ ",
        "| |\\/| / _` | '_|  _|  _| || |",
        "|_|  |_\\__,_|_|  \\__|\\__|\\_, |",
        "                         |__/ ",
    ];
    if width < 8 {
        return Vec::new();
    }
    if width < 34 {
        return vec![Line::from(vec![
            Span::raw(" ".repeat((width as usize).saturating_sub(8) / 2)),
            Span::styled(
                "MAR",
                Style::default()
                    .fg(theme.style(StyleToken::Accent).fg.unwrap_or(Color::Blue))
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
            Span::styled(
                "TTY",
                theme
                    .style(StyleToken::Default)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            ),
        ])];
    }
    let rows: &[&str] = if width < 56 { &SMALL } else { &LARGE };
    let split = if width < 56 { 16 } else { 27 };
    let art_width = if width < 56 { 30 } else { 54 };
    let pad = (width as usize).saturating_sub(art_width) / 2;
    let ocean_top = Color::Rgb(237, 243, 254);
    let ocean_bottom = theme.style(StyleToken::Accent).fg.unwrap_or(Color::Blue);
    rows.iter()
        .enumerate()
        .map(|(row, text)| {
            let mut chars = text.chars();
            let mar = chars.by_ref().take(split).collect::<String>();
            let tty = chars.collect::<String>();
            let t = row as f32 / rows.len().saturating_sub(1).max(1) as f32;
            let ocean = lerp_color(ocean_top, ocean_bottom, t);
            Line::from(vec![
                Span::raw(" ".repeat(pad)),
                Span::styled(mar, fg(theme, ocean)),
                Span::styled(tty, theme.style(StyleToken::Default)),
            ])
        })
        .collect()
}

/// Dibuja el emblema pixelado de ballena de Martty en celdas RGB.
pub fn render_martty_whale(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    pixels: &[Color],
    width: u32,
    height: u32,
) {
    crate::widgets::raster::render_pixels(frame, area, pixels, width, height);
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
