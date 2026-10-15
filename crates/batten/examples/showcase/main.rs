//! Showcase navegable de los widgets de batten.
//! Uso: `cargo run -p batten --example showcase`

mod app;
mod demo;
mod demos;

use app::{route, Action, Focus, Showcase};
use batten::widgets::{Anim, Theme, FRAME_INTERVAL};
use ratatui::crossterm::event::{self, Event};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use std::time::{Duration, Instant};

const INDEX_WIDTH: u16 = 26;
const WIDE_MIN: u16 = 70;

fn main() -> std::io::Result<()> {
    let theme = Theme::default_dark();
    let mut show = Showcase::new();
    let mut anim = Anim::new();
    let mut terminal = ratatui::init();
    let mut last_tick = Instant::now();

    while !show.should_quit() {
        if last_tick.elapsed() >= FRAME_INTERVAL {
            anim.advance();
            show.tick(&anim);
            last_tick = Instant::now();
        }
        terminal.draw(|f| draw(f, &mut show, &anim, &theme))?;
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(k) = event::read()? {
                if route(&mut show, k.kind, k.code, k.modifiers) == Action::Quit {
                    break;
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}

fn draw(f: &mut Frame, show: &mut Showcase, _anim: &Anim, theme: &Theme) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let (body, footer) = (rows[0], rows[1]);

    let wide = area.width >= WIDE_MIN;
    let (index, panel) = if wide {
        let cols =
            Layout::horizontal([Constraint::Length(INDEX_WIDTH), Constraint::Min(1)]).split(body);
        (Some(cols[0]), cols[1])
    } else {
        (None, body)
    };

    let header_h = if wide { 2 } else { 3 };
    let parts = Layout::vertical([Constraint::Length(header_h), Constraint::Min(1)]).split(panel);
    let (header, content) = (parts[0], parts[1]);

    if let Some(ia) = index {
        draw_index(f, ia, show, theme);
        draw_header(f, header, show, theme);
    } else {
        draw_collapsed(f, header, show, theme);
    }

    if let Some(demo) = show.current() {
        demo.render(f, content, theme);
    }
    draw_footer(f, footer);
}

fn draw_index(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let mut lines: Vec<Line> = Vec::new();
    for i in 0..show.len() {
        let title = show.title_at(i).unwrap_or("");
        let selected = i == show.selected();
        let focused = selected && show.focus() == Focus::Index;
        let marker = if focused {
            "▸ "
        } else if selected {
            "• "
        } else {
            "  "
        };
        let mut style = theme.style(batten::state::StyleToken::Default);
        if focused {
            style = style.add_modifier(Modifier::BOLD);
        }
        lines.push(Line::styled(format!("{marker}{title}"), style));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_header(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let title = show.title_at(show.selected()).unwrap_or("");
    let hint = match show.focus() {
        Focus::Index => "Tab: foco · ↓/↑: widget · q: salir".to_string(),
        Focus::Demo => format!("{} · Tab/Esc: índice", show.current_hint()),
    };
    let lines = vec![
        Line::styled(
            title.to_string(),
            theme
                .style(batten::state::StyleToken::Heading)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            hint.to_string(),
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_collapsed(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let title = show.title_at(show.selected()).unwrap_or("");
    let lines = vec![
        Line::styled(
            format!("‹ {title} ›  (Tab cambia de widget)"),
            theme.style(batten::state::StyleToken::Muted),
        ),
        Line::styled(
            title.to_string(),
            theme.style(batten::state::StyleToken::Heading),
        ),
        Line::styled(
            "↓/↑: widget · Tab: foco · q: salir",
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(f: &mut Frame, area: Rect) {
    f.render_widget(
        Paragraph::new("batten showcase").style(Style::default().add_modifier(Modifier::DIM)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn draw_does_not_panic_at_any_size() {
        let theme = Theme::default_dark();
        let anim = Anim::new();
        let mut show = Showcase::new();
        for (w, h) in [(0u16, 0u16), (1, 1), (60, 20), (120, 40)] {
            let backend = TestBackend::new(w, h);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| draw(f, &mut show, &anim, &theme)).unwrap();
        }
    }
}
