//! Salida de terminal con colores ANSI (SGR) → líneas de ratatui.

use ansi_to_tui::IntoText;
use ratatui::text::Line;

/// Convierte bytes con secuencias ANSI en líneas con estilo. Las secuencias
/// desconocidas se ignoran; si el parser falla, devuelve el texto plano.
pub fn ansi_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    match bytes.into_text() {
        Ok(text) => text.lines,
        // ponytail: fallback con pérdida; las secuencias rotas quedan visibles.
        Err(_) => String::from_utf8_lossy(bytes)
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn text(l: &Line) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn keeps_text_and_colors() {
        let l = ansi_lines(b"\x1b[31mred\x1b[0m plain");
        assert_eq!(l.len(), 1);
        assert_eq!(text(&l[0]), "red plain");
        assert_eq!(l[0].spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn splits_lines() {
        assert_eq!(ansi_lines(b"a\nb").len(), 2);
    }

    #[test]
    fn plain_and_broken_sequences_keep_text() {
        assert_eq!(text(&ansi_lines(b"sin color")[0]), "sin color");
        let l = ansi_lines(b"\x1b[999zok");
        assert!(text(&l[0]).contains("ok"));
    }
}
