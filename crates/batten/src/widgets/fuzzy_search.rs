//! Popup de búsqueda difusa de comandos de Pawan.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

/// Busca comandos en un popup centrado con cursor y fila seleccionada.
pub fn render_pawan(
    frame: &mut Frame,
    screen: Rect,
    query: &str,
    results: &[String],
    selected: usize,
) -> Rect {
    if screen.width < 34 || screen.height < 7 {
        return Rect::default();
    }
    let width = (screen.width / 2)
        .max(30)
        .min(screen.width.saturating_sub(2));
    let height = (results.len() as u16 + 4)
        .min(24)
        .min(screen.height.saturating_sub(2));
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + screen.height / 4,
        width,
        height,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Fuzzy search (Ctrl+P, Ctrl+F) ");
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    if inner.height == 0 {
        return area;
    }
    let query_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Cyan)),
        Span::styled(query.to_string(), Style::default().fg(Color::White)),
        Span::styled(
            "▌",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(query_line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let list_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    let selected = selected.min(results.len().saturating_sub(1));
    let visible = results
        .iter()
        .take(list_area.height as usize)
        .enumerate()
        .map(|(index, result)| {
            let style = if index == selected {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(result.clone(), style)))
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(visible), list_area);
    area
}
