//! Showcase navegable de los widgets de batten.
//! Uso: `cargo run -p batten --example showcase`

mod app;
mod demo;
mod demos;
mod matrix;
mod prompt_keys;

use app::{route, Action, Focus, Showcase};
use batten::widgets::{Anim, Theme, ThemePreset, FRAME_INTERVAL};
use ratatui::crossterm::event::{self, Event, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::time::{Duration, Instant};

const INDEX_WIDTH: u16 = 34;
const WIDE_MIN: u16 = 70;

fn main() -> std::io::Result<()> {
    let mut show = Showcase::new();
    let mut active_preset = show.theme_preset();
    let mut theme = Theme::preset(active_preset);
    let mut anim = Anim::new();
    let mut terminal = ratatui::init();
    ratatui::crossterm::execute!(std::io::stdout(), event::EnableMouseCapture)?;
    let mut last_tick = Instant::now();

    while !show.should_quit() {
        if last_tick.elapsed() >= FRAME_INTERVAL {
            anim.advance();
            show.tick(&anim);
            last_tick = Instant::now();
        }
        terminal.draw(|f| draw(f, &mut show, &anim, &theme))?;
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(k) => {
                    if route(&mut show, k.kind, k.code, k.modifiers) == Action::Quit {
                        break;
                    }
                    let preset = show.theme_preset();
                    if preset != active_preset {
                        active_preset = preset;
                        theme = Theme::preset(preset);
                    }
                }
                Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => {
                    handle_mouse(&mut show, mouse.column, mouse.row, terminal.size()?.into());
                }
                _ => {}
            }
        }
    }

    ratatui::crossterm::execute!(std::io::stdout(), event::DisableMouseCapture)?;
    ratatui::restore();
    Ok(())
}

fn draw(f: &mut Frame, show: &mut Showcase, _anim: &Anim, theme: &Theme) {
    let area = f.area();
    if area.width == 0 || area.height == 0 {
        return;
    }
    f.render_widget(
        Block::default().style(theme.style(batten::state::StyleToken::Default)),
        area,
    );
    let [filters, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_filters(f, filters, show, theme);

    let wide = area.width >= WIDE_MIN;
    let (index, panel) = if wide {
        let cols =
            Layout::horizontal([Constraint::Length(INDEX_WIDTH), Constraint::Min(1)]).split(body);
        (Some(cols[0]), cols[1])
    } else {
        (None, body)
    };

    let header_h = 3;
    let parts = Layout::vertical([Constraint::Length(header_h), Constraint::Min(1)]).split(panel);
    let (header, content) = (parts[0], parts[1]);

    if let Some(ia) = index {
        draw_index(f, ia, show, theme);
        if !show.current().is_some_and(|demo| demo.fullscreen()) {
            draw_header(f, header, show, theme);
        }
    } else {
        if !show.current().is_some_and(|demo| demo.fullscreen()) {
            draw_collapsed(f, header, show, theme);
        }
    }

    if let Some(demo) = show.current() {
        demo.render(f, if demo.fullscreen() { panel } else { content }, theme);
    }
    draw_footer(f, footer, show.theme_preset(), theme);
}

fn draw_filters(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let [provider_area, query_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
    let muted = theme.style(batten::state::StyleToken::Muted);
    let active = theme
        .style(batten::state::StyleToken::Accent)
        .add_modifier(Modifier::BOLD);
    let mut provider_spans = vec![Span::styled("Provider: ", muted)];
    let selected = show.provider_filter();
    let all_style = if selected.is_none() { active } else { muted };
    provider_spans.push(Span::styled(" All ", all_style));
    for provider in show.providers() {
        let style = if selected == Some(provider) {
            active
        } else {
            muted
        };
        provider_spans.push(Span::styled(format!(" {provider} "), style));
    }
    f.render_widget(Paragraph::new(Line::from(provider_spans)), provider_area);

    let cursor = if show.focus() == Focus::TextFilter {
        "▏"
    } else {
        ""
    };
    let query = if show.query().is_empty() && show.focus() != Focus::TextFilter {
        "provider · component · variant".to_string()
    } else {
        show.query().to_string()
    };
    let query_style = if show.focus() == Focus::TextFilter {
        theme.style(batten::state::StyleToken::Heading)
    } else {
        muted
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Filter: ", muted),
            Span::styled(query, query_style),
            Span::styled(cursor, theme.style(batten::state::StyleToken::Accent)),
        ])),
        query_area,
    );
}

fn handle_mouse(show: &mut Showcase, column: u16, row: u16, screen: Rect) {
    if screen.width == 0 || screen.height < 3 {
        return;
    }
    let filter_end = screen.y.saturating_add(2);
    if row == screen.y {
        let mut x = screen.x.saturating_add(10);
        if column >= x && usize::from(column - x) < 5 {
            show.select_provider(None);
            show.set_focus(Focus::ProviderFilter);
            return;
        }
        x = x.saturating_add(5);
        for provider in show.providers() {
            let width = provider.chars().count() as u16 + 2;
            if column >= x && column < x.saturating_add(width) {
                show.select_provider(Some(provider));
                show.set_focus(Focus::ProviderFilter);
                return;
            }
            x = x.saturating_add(width);
        }
        show.set_focus(Focus::ProviderFilter);
        return;
    }
    if row == screen.y.saturating_add(1) {
        show.set_focus(Focus::TextFilter);
        return;
    }
    if row < filter_end || row >= screen.y.saturating_add(screen.height).saturating_sub(1) {
        return;
    }

    let body_y = filter_end;
    let body_height = screen.height.saturating_sub(3);
    if screen.width >= WIDE_MIN && column < screen.x.saturating_add(INDEX_WIDTH) {
        let relative_row = row.saturating_sub(body_y);
        let (start, visible) = show.visible_index_rows(body_height);
        if start > 0 && relative_row == 0 {
            return;
        }
        let all_rows = show.index_rows();
        if start + visible.len() < all_rows.len() && usize::from(relative_row) + 1 == visible.len()
        {
            return;
        }
        if let Some((Some(index), _)) = visible.get(usize::from(relative_row)) {
            show.select(*index, true);
        }
    } else if screen.width < WIDE_MIN || column >= screen.x.saturating_add(INDEX_WIDTH) {
        show.set_focus(Focus::Demo);
    }
}

fn draw_index(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let (start, rows) = show.visible_index_rows(area.height);
    let all_rows = show.index_rows();
    let mut lines = Vec::with_capacity(rows.len());
    for (offset, (index, label)) in rows.iter().enumerate() {
        if offset == 0 && start > 0 {
            lines.push(Line::styled(
                "  ↑ más resultados",
                theme.style(batten::state::StyleToken::Muted),
            ));
            continue;
        }
        if start + offset + 1 == all_rows.len() && start + rows.len() < all_rows.len() {
            lines.push(Line::styled(
                "  ↓ más resultados",
                theme.style(batten::state::StyleToken::Muted),
            ));
            continue;
        }
        let Some(i) = index else {
            lines.push(Line::styled(
                format!("  {label}"),
                theme.style(batten::state::StyleToken::Muted),
            ));
            continue;
        };
        let selected = *i == show.selected();
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
        lines.push(Line::styled(format!("  {marker}{label}"), style));
    }
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_header(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let component = show.component_at(show.selected()).unwrap_or("");
    let variant = show.variant_at(show.selected()).unwrap_or("");
    let hint = show.current_hint();
    let lines = vec![
        Line::styled(
            component.to_string(),
            theme
                .style(batten::state::StyleToken::Heading)
                .add_modifier(Modifier::BOLD),
        ),
        Line::styled(
            format!("{variant} · {hint}"),
            theme.style(batten::state::StyleToken::Muted),
        ),
        Line::styled(
            match show.focus() {
                Focus::Index => "↑/↓: variante · Tab: abrir · F7: paleta · q: salir",
                Focus::Demo => "F7: paleta · Tab/Esc: volver al índice",
                Focus::ProviderFilter => "←/→: proveedor · Tab/Enter: buscar · Esc: índice",
                Focus::TextFilter => "Escribe para filtrar · Enter: índice · Esc: limpiar",
            },
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_collapsed(f: &mut Frame, area: Rect, show: &Showcase, theme: &Theme) {
    let component = show.component_at(show.selected()).unwrap_or("");
    let variant = show.variant_at(show.selected()).unwrap_or("");
    let lines = vec![
        Line::styled(
            component.to_string(),
            theme.style(batten::state::StyleToken::Heading),
        ),
        Line::styled(
            format!("{variant} · {}", show.current_hint()),
            theme.style(batten::state::StyleToken::Muted),
        ),
        Line::styled(
            "↑/↓: variante · Tab: abrir · F7: paleta · q: salir",
            theme.style(batten::state::StyleToken::Muted),
        ),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(f: &mut Frame, area: Rect, preset: ThemePreset, theme: &Theme) {
    f.render_widget(
        Paragraph::new(format!(
            "batten showcase · F7/Shift+F7 tema: {}",
            preset.name()
        ))
        .style(theme.style(batten::state::StyleToken::Muted)),
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
        let anim = Anim::new();
        let mut show = Showcase::new();
        let theme = Theme::preset(show.theme_preset());
        for (w, h) in [(0u16, 0u16), (1, 1), (60, 20), (120, 40)] {
            let backend = TestBackend::new(w, h);
            let mut term = Terminal::new(backend).unwrap();
            term.draw(|f| draw(f, &mut show, &anim, &theme)).unwrap();
        }
    }
}
