//! Render del sidebar: logo (o compacto) + secciones.

use crate::state::{Sidebar, StyleToken};
use crate::widgets::logo::{self, Wordmark, FONT_ROWS};
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Altura mínima para el wordmark de bloques (wordmark + una línea en blanco).
pub const LOGO_MIN_HEIGHT: usize = FONT_ROWS + 2;

pub fn render(frame: &mut Frame, area: Rect, sidebar: &Sidebar, theme: &Theme) {
    let block = Block::default().borders(Borders::LEFT);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let height = inner.height as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    if let Some(log) = &sidebar.logo {
        let wm = Wordmark::new(&log.name);
        if height >= LOGO_MIN_HEIGHT && wm.width() <= width {
            lines.extend(logo::render(&wm, theme));
        } else {
            lines.extend(logo::compact(&log.name, width, theme));
        }
        lines.push(Line::from(""));
    }

    for section in &sidebar.sections {
        lines.push(Line::from(Span::styled(
            section.title.clone(),
            theme.style(StyleToken::Heading),
        )));
        for l in &section.lines {
            lines.push(Line::from(Span::styled(
                truncate(l, width),
                theme.style(StyleToken::Default),
            )));
        }
        lines.push(Line::from(""));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// Trunca por anchura de display reservando una columna para `…`.
fn truncate(s: &str, max: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Sidebar;
    use crate::widgets::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(sb: &Sidebar, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), sb, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_block_logo_and_sections() {
        let mut sb = Sidebar::with_logo("batten");
        sb.push_section("estado", vec!["working".into()]);
        let out = buffer(&sb, 32, 40);
        assert!(out.contains('█'));
        assert!(out.contains("estado"));
        assert!(out.contains("working"));
    }

    #[test]
    fn compact_logo_when_the_wordmark_does_not_fit() {
        let sb = Sidebar::with_logo("batten");
        let out = buffer(&sb, 32, LOGO_MIN_HEIGHT as u16 - 1);
        assert!(out.contains("batten"));
    }

    #[test]
    fn compact_logo_when_too_narrow_for_the_wordmark() {
        let sb = Sidebar::with_logo("batten");
        let out = buffer(&sb, 8, 40);
        assert!(out.contains("batten"));
    }

    #[test]
    fn truncates_long_lines_with_an_ellipsis() {
        let mut sb = Sidebar::new();
        sb.push_section("s", vec!["x".repeat(100)]);
        let out = buffer(&sb, 20, 10);
        assert!(out.contains('…'));
    }

    #[test]
    fn truncates_on_display_width_not_bytes() {
        // CJK es 2 columnas por carácter: truncar por bytes desbordaría.
        let mut sb = Sidebar::new();
        sb.push_section("s", vec!["星".repeat(50)]);
        let _ = buffer(&sb, 20, 10);
    }

    #[test]
    fn zero_and_tiny_sizes_do_not_panic() {
        let sb = Sidebar::with_logo("batten");
        let _ = buffer(&sb, 0, 0);
        let _ = buffer(&sb, 1, 1);
        let _ = buffer(&sb, 1, 40);
    }
}
