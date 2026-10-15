//! Helpers de texto compartidos por los widgets.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Trunca a `max` columnas de display, con `…` final si recorta.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}
