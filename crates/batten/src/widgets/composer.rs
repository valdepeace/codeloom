//! Widget para el composer de entrada.

use crate::state::{Composer, StyleToken};
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, composer: &Composer, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("prompt")
        .border_style(theme.style(StyleToken::Muted));
    let text = Paragraph::new(composer.text().to_string())
        .style(theme.style(StyleToken::Default))
        .block(block);
    frame.render_widget(text, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Composer;
    use ratatui::backend::TestBackend;
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
}
