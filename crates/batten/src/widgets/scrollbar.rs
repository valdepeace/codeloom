//! Barra de scroll vertical para una vista con `Viewport`.

use crate::state::{Scroll, StyleToken};
use crate::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

/// Pinta una barra vertical en `area` según la posición de `scroll`.
pub fn render(frame: &mut Frame, area: Rect, scroll: Scroll, theme: &Theme) {
    let mut state = ScrollbarState::new(scroll.content)
        .position(scroll.offset)
        .viewport_content_length(scroll.viewport);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .style(theme.style(StyleToken::Scrollbar));
    frame.render_stateful_widget(bar, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Scroll;
    use crate::widgets::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer(w: u16, h: u16, scroll: Scroll) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, f.area(), scroll, &Theme::test()))
            .unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn draws_something_in_a_tall_area() {
        let out = buffer(
            1,
            10,
            Scroll {
                offset: 0,
                content: 100,
                viewport: 10,
            },
        );
        assert!(
            out.chars().any(|c| c != ' '),
            "la barra debe pintar alguna celda"
        );
    }

    #[test]
    fn zero_and_tiny_sizes_do_not_panic() {
        let s = Scroll {
            offset: 0,
            content: 0,
            viewport: 0,
        };
        let _ = buffer(0, 0, s);
        let _ = buffer(1, 1, s);
        let _ = buffer(1, 10, s);
    }
}
