//! Demo sin back: inyecta eventos falsos y muestra los componentes.
//! Uso: `cargo run -p batten --example demo -- basic`
use batten::state::{Composer, Line, LineKind, ToolItem, Transcript, Viewport};
use batten::widgets::{self, Anim, Theme, FRAME_INTERVAL};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::{Duration, Instant};

/// Aplica una tecla. Solo cuenta la pulsación: en Windows llegan también
/// `Release`/`Repeat`, y actuar sobre ellos duplicaría cada carácter.
/// Devuelve `true` si hay que salir.
fn handle_key(
    kind: KeyEventKind,
    code: KeyCode,
    composer: &mut Composer,
    transcript: &mut Transcript,
) -> bool {
    if kind != KeyEventKind::Press {
        return false;
    }
    match code {
        KeyCode::Esc | KeyCode::Char('q') => return true,
        KeyCode::Enter => {
            if let Some(text) = composer.submit() {
                transcript.push(Line::new(LineKind::User, text));
            }
        }
        KeyCode::Backspace => composer.backspace(),
        KeyCode::Char(c) => composer.push_char(c),
        _ => {}
    }
    false
}

fn main() -> std::io::Result<()> {
    let scenario = std::env::args().nth(1).unwrap_or_else(|| "basic".into());
    let theme = Theme::default_dark();
    let mut terminal = ratatui::init();

    let mut transcript = Transcript::new();
    let mut composer = Composer::new();
    let viewport = Viewport::new();
    let mut anim = Anim::new();
    let mut tool = ToolItem::new("c1", "view");
    let mut last_tick = Instant::now();
    let started = Instant::now();

    let mut side = batten::state::Sidebar::with_logo("batten");
    side.push_section("state", vec!["working".into()]);
    side.push_section("model", vec!["mock".into()]);

    loop {
        // ---- "Eventos" falsos, por tiempo ----
        if scenario == "basic" {
            let secs = started.elapsed().as_secs();
            if transcript.is_empty() {
                transcript.push(Line::new(
                    LineKind::User,
                    "lee hello.txt y dime qué contiene",
                ));
            }
            if secs >= 1 && transcript.len() == 1 {
                transcript.push(Line::new(LineKind::Assistant, "hola mundo"));
            }
        } else {
            if transcript.is_empty() {
                transcript.push(Line::new(LineKind::User, "lee hello.txt"));
                tool.start();
            }
            let secs = started.elapsed().as_secs();
            if secs >= 1 && tool.status() == batten::state::ToolStatus::Running {
                tool.set_result(false);
                transcript.push(Line::new(LineKind::Assistant, "listo"));
            }
        }

        if last_tick.elapsed() >= FRAME_INTERVAL {
            anim.advance();
            last_tick = Instant::now();
        }

        terminal.draw(|f| {
            let area = f.area();
            let rows = ratatui::layout::Layout::vertical([
                ratatui::layout::Constraint::Min(1),
                ratatui::layout::Constraint::Length(3),
            ])
            .split(area);
            let (main, side_area) = if area.width >= 80 {
                let cols = ratatui::layout::Layout::horizontal([
                    ratatui::layout::Constraint::Min(1),
                    ratatui::layout::Constraint::Length(30),
                ])
                .split(rows[0]);
                (cols[0], Some(cols[1]))
            } else {
                (rows[0], None)
            };
            if scenario == "tools" {
                let top = ratatui::layout::Layout::vertical([
                    ratatui::layout::Constraint::Length(2),
                    ratatui::layout::Constraint::Min(1),
                ])
                .split(main);
                widgets::tool_item::render(f, top[0], &tool, &anim, &theme);
                widgets::transcript::render(f, top[1], &transcript, &viewport, &theme);
            } else {
                widgets::transcript::render(f, main, &transcript, &viewport, &theme);
            }
            if let Some(sa) = side_area {
                batten::widgets::sidebar::render(f, sa, &side, &theme);
            }
            widgets::composer::render(f, rows[1], &composer, &theme);
        })?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(k) = event::read()? {
                if handle_key(k.kind, k.code, &mut composer, &mut transcript) {
                    break;
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyEventKind;

    #[test]
    fn release_events_do_not_type() {
        let mut c = Composer::new();
        let mut t = Transcript::new();
        let quit = handle_key(KeyEventKind::Release, KeyCode::Char('a'), &mut c, &mut t);
        assert!(!quit);
        assert!(c.is_empty());
        assert!(t.is_empty());
    }

    #[test]
    fn press_events_type_and_submit() {
        let mut c = Composer::new();
        let mut t = Transcript::new();
        handle_key(KeyEventKind::Press, KeyCode::Char('h'), &mut c, &mut t);
        handle_key(KeyEventKind::Press, KeyCode::Char('i'), &mut c, &mut t);
        handle_key(KeyEventKind::Press, KeyCode::Enter, &mut c, &mut t);
        assert_eq!(t.len(), 1);
        assert_eq!(t.line(0).unwrap().plain(), "hi");
    }

    #[test]
    fn esc_quits() {
        let mut c = Composer::new();
        let mut t = Transcript::new();
        assert!(handle_key(
            KeyEventKind::Press,
            KeyCode::Esc,
            &mut c,
            &mut t
        ));
    }
}
