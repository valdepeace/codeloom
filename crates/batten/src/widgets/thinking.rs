//! Indicador compacto de actividad mientras el asistente está pensando.

use crate::state::StyleToken;
use crate::widgets::{Anim, Theme};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub fn render(frame: &mut Frame, area: Rect, label: &str, anim: &Anim, theme: &Theme) {
    let line = Line::from(vec![
        anim.glyph(super::spinner::BRAILLE, theme),
        Span::styled(format!(" {label}"), theme.style(StyleToken::Muted)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// Bloque plegable de thinking de Mindfork.
pub fn render_mindfork(frame: &mut Frame, area: Rect, content: &[&str], expanded: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let muted = Style::default().fg(Color::Rgb(133, 139, 146));
    let marker = if expanded { "▾" } else { "▸" };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("{marker} thinking"),
            muted.add_modifier(Modifier::ITALIC),
        ),
        Span::styled(format!(" · {} lines", content.len()), muted),
        if expanded {
            Span::raw("")
        } else {
            Span::styled("  Ctrl+T", muted)
        },
    ])];
    if expanded {
        lines.extend(content.iter().map(|line| {
            Line::from(vec![
                Span::styled("│ ", muted),
                Span::styled((*line).to_string(), muted.add_modifier(Modifier::ITALIC)),
            ])
        }));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Resumen de thinking de Martty con duración y número de líneas.
pub fn render_martty(
    frame: &mut Frame,
    area: Rect,
    content: &[&str],
    elapsed: &str,
    expanded: bool,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let muted = theme.style(StyleToken::Muted);
    let mut lines = vec![Line::from(vec![
        Span::styled("✻ thought", muted),
        Span::styled(format!(" · {elapsed} · {} lines", content.len()), muted),
    ])];
    if expanded {
        lines.extend(content.iter().map(|line| {
            Line::from(vec![
                Span::styled("  ", muted),
                Span::styled((*line).to_string(), muted.add_modifier(Modifier::ITALIC)),
            ])
        }));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Caja de thinking de Crush: contenido con énfasis tenue, ventana plegada y
/// duración cuando el razonamiento ha terminado.
pub fn render_crush(
    frame: &mut Frame,
    area: Rect,
    content: &[&str],
    expanded: bool,
    complete: bool,
    duration: &str,
    theme: &Theme,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let muted = theme.style(StyleToken::Muted);
    let mut lines = Vec::new();
    if !expanded && content.len() > 10 {
        lines.push(Line::from(Span::styled(
            format!("… ({} lines hidden) [space to expand]", content.len() - 10),
            muted,
        )));
        lines.push(Line::raw(""));
        lines.extend(
            content
                .iter()
                .rev()
                .take(10)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(|line| {
                    Line::from(vec![
                        Span::styled("│ ", muted),
                        Span::styled((*line).to_string(), muted.add_modifier(Modifier::ITALIC)),
                    ])
                }),
        );
    } else {
        lines.extend(content.iter().map(|line| {
            Line::from(vec![
                Span::styled("│ ", muted),
                Span::styled((*line).to_string(), muted.add_modifier(Modifier::ITALIC)),
            ])
        }));
    }
    let title = if complete { "Thinking" } else { "Thinking…" };
    let block = ratatui::widgets::Block::default()
        .borders(ratatui::widgets::Borders::LEFT)
        .border_style(theme.style(StyleToken::Accent))
        .title(Span::styled(title, muted.add_modifier(Modifier::ITALIC)));
    frame.render_widget(ratatui::widgets::Paragraph::new(lines).block(block), area);
    if complete && !duration.is_empty() && area.height > 1 {
        let footer_y = area.bottom() - 1;
        frame.render_widget(
            ratatui::widgets::Paragraph::new(Line::from(Span::styled(
                format!("Thought for {duration}"),
                muted,
            ))),
            Rect::new(area.x, footer_y, area.width, 1),
        );
    }
}
