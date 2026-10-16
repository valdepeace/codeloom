use crate::app::App;
use crate::app::Focus;
use crate::render::render;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::time::Duration;

/// Restaura el terminal al salir, pase lo que pase (retorno normal, error o
/// panic): sin esto, un fallo en modo raw dejaba la terminal inutilizable.
struct RawGuard;

impl RawGuard {
    fn enter() -> std::io::Result<Self> {
        enable_raw_mode()?;
        execute!(std::io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = std::io::stdout().execute(LeaveAlternateScreen);
    }
}

/// Teclas con el foco en el transcript (como Crush).
fn transcript_key(app: &mut App, code: KeyCode) {
    let len = app.transcript.len();
    if let Some(mut s) = app.selection {
        match code {
            KeyCode::Up | KeyCode::Char('k') => s.extend(-1, len),
            KeyCode::Down | KeyCode::Char('j') => s.extend(1, len),
            KeyCode::Char('y') | KeyCode::Char('c') => {
                app.status = format!("copiado ({} líneas)", s.range().count());
                app.clipboard = Some(s.text(&app.transcript));
                app.selection = None;
                return;
            }
            KeyCode::Esc | KeyCode::Char('v') => {
                app.selection = None;
                return;
            }
            _ => return,
        }
        app.selection = Some(s);
        return;
    }
    if code == KeyCode::Char('v') && len > 0 {
        app.selection = Some(batten::state::Selection::at(len - 1));
        return;
    }
    let vp = &mut app.viewport;
    match code {
        KeyCode::Up | KeyCode::Char('k') => vp.scroll_up(1),
        KeyCode::Down | KeyCode::Char('j') => vp.scroll_down(1),
        KeyCode::Char('u') => vp.half_page_up(),
        KeyCode::Char('d') => vp.half_page_down(),
        KeyCode::PageUp | KeyCode::Char('b') => vp.page_up(),
        KeyCode::PageDown | KeyCode::Char('f') | KeyCode::Char(' ') => vp.page_down(),
        KeyCode::Home | KeyCode::Char('g') => vp.to_top(),
        KeyCode::End | KeyCode::Char('G') => vp.follow_bottom(),
        KeyCode::Esc => app.focus = Focus::Composer,
        _ => {}
    }
}

/// Traduce una tecla a una accion sobre la app. Devuelve el prompt a enviar
/// cuando el usuario pulsa Enter con texto.
pub fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Option<String> {
    if matches!(code, KeyCode::Char('c' | 'q')) && mods.contains(KeyModifiers::CONTROL) {
        app.should_quit = true;
        return None;
    }
    if let Some(p) = app.permission.as_mut() {
        if let Some(d) = crate::permission_flow::permission_key(&mut p.dialog, code) {
            app.resolve_permission(d);
        }
        return None;
    }
    if code == KeyCode::Tab {
        app.toggle_focus();
        return None;
    }
    if app.focus == Focus::Transcript {
        transcript_key(app, code);
        return None;
    }
    match code {
        KeyCode::Esc => None,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            None
        }
        KeyCode::Char('q') if mods.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            None
        }
        KeyCode::Enter => {
            let sent = app.submit();
            if sent.is_some() {
                app.viewport.follow_bottom();
            }
            sent
        }
        KeyCode::PageUp => {
            app.viewport.scroll_up(10);
            None
        }
        KeyCode::PageDown => {
            app.viewport.scroll_down(10);
            None
        }
        KeyCode::Char('j') if mods.contains(KeyModifiers::CONTROL) => {
            app.newline();
            None
        }
        KeyCode::Delete => {
            app.delete();
            None
        }
        KeyCode::Left => {
            app.move_left();
            None
        }
        KeyCode::Right => {
            app.move_right();
            None
        }
        KeyCode::Home => {
            app.move_home();
            None
        }
        KeyCode::End => {
            app.move_end();
            None
        }
        KeyCode::Up => {
            app.up();
            None
        }
        KeyCode::Down => {
            app.down();
            None
        }
        KeyCode::Backspace => {
            app.backspace();
            None
        }
        KeyCode::Char(c) => {
            app.push_char(c);
            None
        }
        _ => None,
    }
}

/// Bucle principal: aplica eventos del bus y lee teclado.
pub async fn run(
    mut app: App,
    prompt_tx: tokio::sync::mpsc::Sender<String>,
    mut bus_rx: tokio::sync::broadcast::Receiver<codeloom_core::event::LiveEvent>,
    mut perm_rx: tokio::sync::mpsc::Receiver<(
        codeloom_core::permission::PermissionRequest,
        crate::permission_flow::PermissionReply,
    )>,
) -> std::io::Result<()> {
    let _guard = RawGuard::enter()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    let mut fx: Option<batten::widgets::fx::Effect> = None;
    while !app.should_quit {
        while let Ok(ev) = bus_rx.try_recv() {
            app.apply(ev);
        }
        while let Ok((req, reply)) = perm_rx.try_recv() {
            app.enqueue_permission(req, reply);
        }
        if std::mem::take(&mut app.fx_pending) {
            fx = Some(batten::widgets::fx::dialog_open(&app.theme));
        }
        if let Some(text) = app.clipboard.take() {
            use std::io::Write;
            let mut out = std::io::stdout();
            let _ = out.write_all(crate::clipboard::osc52(&text).as_bytes());
            let _ = out.flush();
        }
        terminal.draw(|f| {
            render(f, &mut app);
            if let (Some(e), Some(_)) = (fx.as_mut(), app.permission.as_ref()) {
                let area = batten::widgets::permission::area(f.area());
                e.process(
                    batten::widgets::fx::FxDuration::from_millis(50),
                    f.buffer_mut(),
                    area,
                );
            }
        })?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if let Some(text) = handle_key(&mut app, key.code, key.modifiers) {
                        let _ = prompt_tx.try_send(text);
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_dialog(
        app: &mut App,
    ) -> tokio::sync::oneshot::Receiver<codeloom_core::permission::Decision> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.enqueue_permission(
            codeloom_core::permission::PermissionRequest {
                tool: "bash".into(),
                action: "run".into(),
                detail: r#"{"command":"ls"}"#.into(),
            },
            tx,
        );
        rx
    }

    #[test]
    fn dialog_captures_keys_and_answers() {
        use codeloom_core::permission::Decision;
        let mut app = App::new();
        let mut rx = open_dialog(&mut app);
        assert!(app.fx_pending);
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "");
        assert!(app.permission.is_none());
        assert_eq!(rx.try_recv().unwrap(), Decision::Allow);
    }

    #[test]
    fn esc_denies_without_quitting_and_ctrl_c_still_quits() {
        use codeloom_core::permission::Decision;
        let mut app = App::new();
        let mut rx = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(rx.try_recv().unwrap(), Decision::Deny);
        let _rx2 = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn queued_requests_are_answered_in_order() {
        use codeloom_core::permission::Decision;
        let mut app = App::new();
        let mut first = open_dialog(&mut app);
        let mut second = open_dialog(&mut app);
        handle_key(&mut app, KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(app.permission.is_some());
        handle_key(&mut app, KeyCode::Char('s'), KeyModifiers::NONE);
        assert_eq!(first.try_recv().unwrap(), Decision::Deny);
        assert_eq!(second.try_recv().unwrap(), Decision::AllowSession);
    }

    #[test]
    fn ctrl_c_quits() {
        let mut app = App::new();
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL),
            None
        );
        assert!(app.should_quit);
    }

    #[test]
    fn esc_does_not_quit_but_ctrl_q_does() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert!(app.should_quit);
    }

    #[test]
    fn tab_moves_focus_and_transcript_keys_scroll() {
        let mut app = App::new();
        app.viewport.set_geometry(100, 10);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "");
        assert!(!app.viewport.is_following());
        handle_key(&mut app, KeyCode::Char('G'), KeyModifiers::NONE);
        assert!(app.viewport.is_following());
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
        assert!(!app.should_quit);
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert!(
            app.should_quit,
            "Ctrl+Q sale también con foco en el transcript"
        );
    }

    #[test]
    fn plain_c_types_and_enter_submits() {
        let mut app = App::new();
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('h'), KeyModifiers::NONE),
            None
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Char('i'), KeyModifiers::NONE),
            None
        );
        assert_eq!(
            handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE),
            Some("hi".to_string())
        );
        assert!(!app.should_quit);
    }

    #[test]
    fn ctrl_j_adds_a_line_and_enter_sends() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Char('a'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('j'), KeyModifiers::CONTROL);
        handle_key(&mut app, KeyCode::Char('b'), KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "a\nb");
        let sent = handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(sent, Some("a\nb".to_string()));
    }

    #[test]
    fn up_recalls_history_when_single_line() {
        let mut app = App::new();
        for c in "hola".chars() {
            handle_key(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
        }
        handle_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.text(), "hola");
    }

    #[test]
    fn select_extend_copy_and_escape() {
        let mut app = App::new();
        for s in ["uno", "dos", "tres"] {
            app.push_user(s);
        }
        handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Char('y'), KeyModifiers::NONE);
        assert_eq!(app.clipboard.as_deref(), Some("dos\ntres"));
        assert!(app.selection.is_none());
        handle_key(&mut app, KeyCode::Char('v'), KeyModifiers::NONE);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.selection.is_none());
        assert_eq!(app.focus, Focus::Transcript);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Composer);
    }

    #[test]
    fn up_moves_the_cursor_when_multiline() {
        let mut app = App::new();
        app.composer.set_text("uno\ndos");
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.cursor_row_col().0, 0);
    }
}
