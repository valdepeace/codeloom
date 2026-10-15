//! Demo sin back: inyecta eventos falsos y muestra los componentes.
//! Uso: `cargo run -p batten --example demo -- basic`
use batten::state::{Composer, Line, LineKind, ToolItem, Transcript, Viewport};
use batten::widgets::{self, Anim, Theme, FRAME_INTERVAL};
use ratatui::crossterm::event::{self, Event, KeyCode};
use std::time::{Duration, Instant};

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
            if scenario == "tools" {
                let top = ratatui::layout::Layout::vertical([
                    ratatui::layout::Constraint::Length(2),
                    ratatui::layout::Constraint::Min(1),
                ])
                .split(rows[0]);
                widgets::tool_item::render(f, top[0], &tool, &anim, &theme);
                widgets::transcript::render(f, top[1], &transcript, &viewport, &theme);
            } else {
                widgets::transcript::render(f, rows[0], &transcript, &viewport, &theme);
            }
            widgets::composer::render(f, rows[1], &composer, &theme);
        })?;

        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(k) = event::read()? {
                match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => break,
                    KeyCode::Enter => {
                        if let Some(text) = composer.submit() {
                            transcript.push(Line::new(LineKind::User, text));
                        }
                    }
                    KeyCode::Backspace => composer.backspace(),
                    KeyCode::Char(c) => composer.push_char(c),
                    _ => {}
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}
