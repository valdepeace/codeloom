//! Salida de terminal con colores ANSI (SGR) → líneas de ratatui.

use ansi_to_tui::IntoText;
use ratatui::text::Line;

/// Convierte bytes de un terminal en líneas con estilo: se sanean primero
/// (ver `sanitize`) y se interpretan los colores SGR. Si el parser falla,
/// devuelve el texto sin ninguna secuencia.
pub fn ansi_lines(bytes: &[u8]) -> Vec<Line<'static>> {
    let clean = sanitize(bytes, true);
    match clean.into_text() {
        Ok(text) => text.lines,
        Err(_) => String::from_utf8_lossy(&sanitize(bytes, false))
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect(),
    }
}

/// Deja solo texto, `\n`, `\t` y (si `keep_sgr`) los colores SGR
/// (`ESC [ … m`). Descarta el resto de secuencias CSI (cursor, borrado), las
/// OSC (títulos, enlaces), los escapes de dos bytes y los caracteres de
/// control. Un `\r` suelto sobrescribe la línea en curso (barras de
/// progreso); `\r\n` es un salto de línea. Las secuencias sin terminar al
/// final se descartan.
pub fn sanitize(bytes: &[u8], keep_sgr: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b {
            match bytes.get(i + 1) {
                Some(b'[') => {
                    let mut j = i + 2;
                    while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                        j += 1;
                    }
                    if keep_sgr && j < bytes.len() && bytes[j] == b'm' {
                        out.extend_from_slice(&bytes[i..=j]);
                    }
                    i = j + 1;
                }
                Some(b']') => {
                    let mut j = i + 2;
                    while j < bytes.len()
                        && bytes[j] != 0x07
                        && !(bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\'))
                    {
                        j += 1;
                    }
                    i = if j < bytes.len() && bytes[j] == 0x1b {
                        j + 2
                    } else {
                        j + 1
                    };
                }
                Some(_) => i += 2,
                None => i += 1,
            }
            continue;
        }
        match b {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {}
            b'\r' => {
                let start = out.iter().rposition(|&c| c == b'\n').map_or(0, |p| p + 1);
                out.truncate(start);
            }
            b'\n' | b'\t' => out.push(b),
            0x00..=0x1f | 0x7f => {}
            _ => out.push(b),
        }
        i += 1;
    }
    out
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

    fn clean(b: &[u8]) -> String {
        String::from_utf8(sanitize(b, true)).unwrap()
    }

    #[test]
    fn drops_cursor_and_erase_sequences() {
        assert_eq!(clean(b"a\x1b[2Kb\x1b[1Ac"), "abc");
    }

    #[test]
    fn drops_osc_with_bel_or_st() {
        assert_eq!(clean(b"\x1b]0;titulo\x07hola"), "hola");
        assert_eq!(clean(b"\x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\"), "link");
    }

    #[test]
    fn carriage_return_overwrites_but_crlf_is_a_newline() {
        let l = ansi_lines(b"50%\r100%\nok");
        assert_eq!(l.iter().map(text).collect::<Vec<_>>(), vec!["100%", "ok"]);
        let l = ansi_lines(b"a\r\nb");
        assert_eq!(l.iter().map(text).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn drops_control_chars_and_keeps_tabs() {
        assert_eq!(clean(b"a\x08\x00b\tc"), "ab\tc");
    }

    #[test]
    fn unterminated_sequences_at_the_end_are_dropped() {
        assert_eq!(clean(b"ok\x1b[31"), "ok");
        assert_eq!(clean(b"ok\x1b]0;tit"), "ok");
        assert_eq!(clean(b"ok\x1b"), "ok");
    }

    #[test]
    fn sgr_is_kept_or_stripped_on_demand() {
        assert_eq!(clean(b"\x1b[31mred\x1b[0m"), "\x1b[31mred\x1b[0m");
        assert_eq!(
            String::from_utf8(sanitize(b"\x1b[31mred\x1b[0m", false)).unwrap(),
            "red"
        );
    }
}
