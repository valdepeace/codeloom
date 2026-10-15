use crate::app::App;
use crate::render::render;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::Stdout;
use std::time::Duration;

pub fn setup() -> std::io::Result<Stdout> {
    enable_raw_mode()?;
    let mut out = std::io::stdout();
    execute!(out, EnterAlternateScreen)?;
    Ok(out)
}

pub fn teardown(mut out: Stdout) -> std::io::Result<()> {
    disable_raw_mode()?;
    out.execute(LeaveAlternateScreen)?;
    Ok(())
}

/// Bucle principal: aplica eventos del bus y lee teclado.
pub async fn run(
    mut app: App,
    prompt_tx: tokio::sync::mpsc::Sender<String>,
    mut bus_rx: tokio::sync::broadcast::Receiver<codeloom_core::event::LiveEvent>,
) -> std::io::Result<()> {
    let out = setup()?;
    let backend = CrosstermBackend::new(out);
    let mut terminal = Terminal::new(backend)?;
    while !app.should_quit {
        while let Ok(ev) = bus_rx.try_recv() {
            app.apply(ev);
        }
        terminal.draw(|f| render(f, &app))?;
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q')
                            if key
                                .modifiers
                                .contains(crossterm::event::KeyModifiers::CONTROL) =>
                        {
                            app.should_quit = true;
                        }
                        KeyCode::Enter => {
                            if let Some(text) = app.submit() {
                                let _ = prompt_tx.try_send(text);
                            }
                        }
                        KeyCode::Backspace => app.backspace(),
                        KeyCode::Char(c) => app.push_char(c),
                        _ => {}
                    }
                }
            }
        }
    }
    let _ = teardown(std::io::stdout());
    Ok(())
}
