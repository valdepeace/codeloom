//! Widget para el composer de entrada.

use crate::state::{Composer, StyleToken};
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Composer con foco (borde de acento y cursor).
pub fn render(frame: &mut Frame, area: Rect, composer: &Composer, theme: &Theme) {
    render_focused(frame, area, composer, true, theme);
}

/// Composer; sin foco, borde atenuado y sin cursor.
pub fn render_focused(
    frame: &mut Frame,
    area: Rect,
    composer: &Composer,
    focused: bool,
    theme: &Theme,
) {
    let border = if focused {
        StyleToken::Accent
    } else {
        StyleToken::Dim
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title("prompt")
        .border_style(theme.style(border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = Paragraph::new(composer.text().to_string()).style(theme.style(StyleToken::Default));
    frame.render_widget(text, inner);

    if !focused {
        return;
    }
    let (row, col) = composer.cursor_row_col();
    if (row as u16) < inner.height && (col as u16) < inner.width {
        frame.set_cursor_position((inner.x + col as u16, inner.y + row as u16));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Composer;
    use ratatui::backend::{Backend, TestBackend};
    use ratatui::Terminal;

    fn buffer(c: &Composer, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), c, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn shows_title_and_text() {
        let mut c = Composer::new();
        c.push_char('h');
        c.push_char('i');
        let out = buffer(&c, 40, 3);
        assert!(out.contains("prompt"));
        assert!(out.contains("hi"));
    }

    #[test]
    fn empty_composer_does_not_panic() {
        let _ = buffer(&Composer::new(), 20, 3);
        let _ = buffer(&Composer::new(), 0, 0);
    }

    #[test]
    fn places_the_cursor_at_the_end() {
        let mut c = Composer::new();
        c.insert_str("hi");
        let backend = TestBackend::new(20, 3);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!((pos.x, pos.y), (3, 1));
    }

    #[test]
    fn places_the_cursor_on_the_second_line() {
        let mut c = Composer::new();
        c.insert_str("a\nb");
        let backend = TestBackend::new(20, 4);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), &c, &Theme::test()))
            .unwrap();
        let pos = term.backend_mut().get_cursor_position().unwrap();
        assert_eq!((pos.x, pos.y), (2, 2));
    }

    #[test]
    fn border_shows_focus() {
        use ratatui::style::Color;
        let c = Composer::new();
        for (focused, color) in [
            (true, Color::Rgb(0, 212, 255)),
            (false, Color::Indexed(240)),
        ] {
            let mut term = Terminal::new(TestBackend::new(20, 3)).unwrap();
            term.draw(|f| render_focused(f, f.area(), &c, focused, &Theme::test()))
                .unwrap();
            assert_eq!(term.backend().buffer()[(0, 0)].fg, color);
        }
    }

    #[test]
    fn tiny_area_does_not_panic() {
        let c = Composer::new();
        let _ = buffer(&c, 0, 0);
        let _ = buffer(&c, 1, 1);
    }
}
