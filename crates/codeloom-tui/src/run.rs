use crate::app::App;
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

/// Traduce una tecla a una accion sobre la app. Devuelve el prompt a enviar
/// cuando el usuario pulsa Enter con texto.
pub fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) -> Option<String> {
    match code {
        KeyCode::Esc => {
            app.should_quit = true;
            None
        }
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            None
        }
        KeyCode::Char('q') if mods.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
            None
        }
        KeyCode::Enter => app.submit(),
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
) -> std::io::Result<()> {
    let _guard = RawGuard::enter()?;
    let backend = CrosstermBackend::new(std::io::stdout());
    let mut terminal = Terminal::new(backend)?;
    while !app.should_quit {
        while let Ok(ev) = bus_rx.try_recv() {
            app.apply(ev);
        }
        terminal.draw(|f| render(f, &app))?;
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
    fn esc_quits() {
        let mut app = App::new();
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.should_quit);
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
    fn up_moves_the_cursor_when_multiline() {
        let mut app = App::new();
        app.composer.set_text("uno\ndos");
        handle_key(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(app.composer.cursor_row_col().0, 0);
    }
}
