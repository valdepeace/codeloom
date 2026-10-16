use crate::app::App;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

/// Por debajo de este ancho la barra lateral se oculta (modo compacto).
const SIDEBAR_MIN_WIDTH: u16 = 90;
const SIDEBAR_WIDTH: u16 = 30;

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let header = Line::from(format!(" CodeLoom · {} ", app.status));
    frame.render_widget(
        Paragraph::new(header).style(Style::default().fg(Color::Black).bg(Color::Cyan)),
        area,
    );
}

fn draw_transcript(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default().borders(Borders::TOP);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let t = app.transcript();
    batten::widgets::transcript::render_with_selection(
        frame,
        inner,
        &t,
        &mut app.viewport,
        app.selection.as_ref(),
        &app.theme,
    );
}

fn draw_composer(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == crate::app::Focus::Composer;
    batten::widgets::composer::render_focused(frame, area, &app.composer, focused, &app.theme);
}

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(3),
    ])
    .split(area);

    draw_header(frame, rows[0], app);

    if area.width >= SIDEBAR_MIN_WIDTH {
        let cols = Layout::horizontal([Constraint::Min(1), Constraint::Length(SIDEBAR_WIDTH)])
            .split(rows[1]);
        draw_transcript(frame, cols[0], app);
        let right = cols[1];
        if app.changes.is_empty() {
            batten::widgets::sidebar::render(frame, right, &app.sidebar(), &app.theme);
        } else {
            let need = (app.changes.len() as u16).saturating_add(2);
            let h = need.min(right.height.saturating_sub(1));
            let parts = Layout::vertical([Constraint::Min(1), Constraint::Length(h)]).split(right);
            batten::widgets::sidebar::render(frame, parts[0], &app.sidebar(), &app.theme);
            batten::widgets::file_changes::render(
                frame,
                parts[1],
                "ficheros modificados",
                &app.changes,
                &app.theme,
            );
        }
    } else {
        draw_transcript(frame, rows[1], app);
    }

    draw_composer(frame, rows[2], app);

    if let Some(p) = &app.permission {
        batten::widgets::permission::render(frame, area, &p.dialog, &app.theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, TranscriptItem};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn buffer_of(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut term = Terminal::new(backend).unwrap();
        term.draw(|f| render(f, app)).unwrap();
        term.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    #[test]
    fn renders_items_and_input() {
        let mut app = App::new();
        app.items.push(TranscriptItem {
            role: "you",
            text: "hello".into(),
        });
        app.push_char('x');
        let content = buffer_of(&mut app, 40, 10);
        assert!(content.contains("hello"));
        assert!(content.contains('x'));
        assert!(content.contains("CodeLoom"));
    }

    #[test]
    fn assistant_markdown_is_rendered() {
        let mut app = App::new();
        app.items.push(TranscriptItem {
            role: "CodeLoom",
            text: "## Hecho\n\n- **uno**\n- `dos`".into(),
        });
        let content = buffer_of(&mut app, 60, 20);
        assert!(content.contains("Hecho"));
        assert!(!content.contains("##"));
        assert!(content.contains("• uno"));
        assert!(!content.contains("**"));
    }

    #[test]
    fn hides_sidebar_when_narrow() {
        let mut app = App::with_context("/proj", "m");
        let content = buffer_of(&mut app, 60, 20);
        assert!(!content.contains("subagentes"));
    }

    #[test]
    fn shows_sidebar_when_wide() {
        let mut app = App::with_context("/proj", "mistral-nemo");
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("mistral-nemo"));
        assert!(content.contains("subagentes"));
        assert!(content.contains('█'));
    }

    #[test]
    fn shows_modified_files_when_wide() {
        let mut app = App::with_context("/proj", "m");
        app.apply(codeloom_core::event::LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("ficheros modificados"));
        assert!(content.contains("src/main.rs"));
    }

    #[test]
    fn shows_permission_dialog_on_top() {
        let mut app = App::new();
        let (tx, _rx) = tokio::sync::oneshot::channel();
        app.enqueue_permission(
            codeloom_core::permission::PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: r#"{"command":"cargo test"}"#.into(),
            },
            tx,
        );
        let content = buffer_of(&mut app, 120, 40);
        assert!(content.contains("Permiso"));
        assert!(content.contains("cargo test"));
        assert!(content.contains("Denegar"));
    }

    fn chatty_app() -> App {
        let mut app = App::new();
        for i in 0..60 {
            app.items.push(TranscriptItem {
                role: "you",
                text: format!("msg-{i:02}"),
            });
        }
        app
    }

    #[test]
    fn newest_messages_are_visible_when_overflowing() {
        let mut app = chatty_app();
        let content = buffer_of(&mut app, 60, 20);
        assert!(content.contains("msg-59"));
        assert!(!content.contains("msg-00"));
    }

    #[test]
    fn page_up_scrolls_back_and_stops_following() {
        let mut app = chatty_app();
        buffer_of(&mut app, 60, 20);
        crate::run::handle_key(
            &mut app,
            crossterm::event::KeyCode::PageUp,
            crossterm::event::KeyModifiers::NONE,
        );
        let content = buffer_of(&mut app, 60, 20);
        assert!(!app.viewport.is_following());
        assert!(!content.contains("msg-59"));
    }
}
