use crate::app::App;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(frame.area());

    let header = Line::from(format!(" CodeLoom · {} ", app.status));
    frame.render_widget(
        Paragraph::new(header).style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        chunks[0],
    );

    let body: Vec<Line> = app
        .items
        .iter()
        .flat_map(|item| {
            let prefix = if item.role == "you" {
                "you: "
            } else {
                "CodeLoom: "
            };
            item.text
                .lines()
                .map(move |l| Line::from(format!("{prefix}{l}")))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(body)
            .block(Block::default().borders(Borders::TOP))
            .wrap(Wrap { trim: false }),
        chunks[1],
    );

    let composer = Paragraph::new(app.input.as_str())
        .block(Block::default().borders(Borders::ALL).title("prompt"));
    frame.render_widget(composer, chunks[2]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, TranscriptItem};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn renders_items_and_input() {
        let mut app = App::new();
        app.items.push(TranscriptItem {
            role: "you",
            text: "hello".into(),
        });
        app.push_char('x');
        let backend = TestBackend::new(40, 10);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, &app)).unwrap();
        let content: String = term
            .backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect();
        assert!(content.contains("hello"));
        assert!(content.contains('x'));
        assert!(content.contains("CodeLoom"));
    }
}
