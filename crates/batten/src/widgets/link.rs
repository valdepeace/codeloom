//! Enlaces clicables (OSC 8) con el estilo del tema. En terminales sin
//! soporte se pintan como texto normal.

use super::theme::Theme;
pub use hyperrat::Link;
use ratatui::style::Modifier;
use std::path::Path;

/// Enlace con estilo `accent` subrayado; si se recorta, termina en `…`.
pub fn link<'a>(label: &'a str, url: &'a str, theme: &Theme) -> Link<'a> {
    Link::new(label, url)
        .style(theme.styles.accent.add_modifier(Modifier::UNDERLINED))
        .fallback_suffix("…")
}

/// URL `file://` de una ruta (absoluta, o relativa al directorio actual),
/// con barras `/` y espacios escapados.
pub fn file_url(path: &Path) -> String {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let s = abs.to_string_lossy().replace('\\', "/").replace(' ', "%20");
    if s.starts_with('/') {
        format!("file://{s}")
    } else {
        format!("file:///{s}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::widgets::Widget;

    #[test]
    fn link_uses_accent_underlined() {
        let th = Theme::test();
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 1));
        let area = buf.area;
        link("main.rs", "file:///x/main.rs", &th)
            .enabled(false)
            .render(area, &mut buf);
        assert_eq!(buf[(0, 0)].symbol(), "m");
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(0, 212, 255));
        assert!(buf[(0, 0)].modifier.contains(Modifier::UNDERLINED));
    }

    #[cfg(windows)]
    #[test]
    fn file_url_windows() {
        assert_eq!(file_url(Path::new(r"E:\a b\c.rs")), "file:///E:/a%20b/c.rs");
    }

    #[cfg(unix)]
    #[test]
    fn file_url_unix() {
        assert_eq!(file_url(Path::new("/a b/c.rs")), "file:///a%20b/c.rs");
    }
}
