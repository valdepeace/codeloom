//! Pantalla inicial de Pawan, reproducida desde su snapshot Ratatui.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// Renderiza la tarjeta de bienvenida de Pawan en el centro de la terminal.
pub fn render_pawan(
    frame: &mut Frame,
    screen: Rect,
    model: &str,
    path: &str,
    version: &str,
) -> Rect {
    if screen.width < 8 || screen.height < 6 {
        return Rect::default();
    }
    let width = 52u16.min(screen.width.saturating_sub(4));
    let height = 12u16.min(screen.height.saturating_sub(4));
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" पवन — pawan ");
    frame.render_widget(Clear, area);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = vec![
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "  Self-healing CLI coding agent",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  v{version}"),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::raw(""),
        Line::from(vec![
            Span::styled("  Model: ", Style::default().fg(Color::DarkGray)),
            Span::styled(model.to_string(), Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("  Path:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(path.to_string(), Style::default().fg(Color::White)),
        ]),
        Line::raw(""),
        Line::from(Span::styled(
            "  Type a task, or explore:",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(vec![
            Span::styled(
                "  Ctrl+P",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "  fuzzy search (commands)",
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "  F1    ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  keyboard shortcuts", Style::default().fg(Color::DarkGray)),
        ]),
        Line::raw(""),
    ];
    frame.render_widget(Paragraph::new(text), inner);
    area
}
