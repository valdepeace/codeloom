//! Selector de modelos compacto basado en el picker de Martty.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

const BG: Color = Color::Rgb(38, 38, 38);
const BORDER: Color = Color::Rgb(0, 148, 255);
const MUTED: Color = Color::Rgb(104, 104, 104);

/// Dibuja el picker de Martty con id técnico y nombre de modelo alineados.
/// Devuelve el área del popup para que el consumidor pueda añadir scroll.
pub fn render_martty(
    frame: &mut Frame,
    screen: Rect,
    title: &str,
    items: &[(&str, &str)],
    selected: usize,
) -> Rect {
    if screen.width < 24 || screen.height < 5 || items.is_empty() {
        return Rect::default();
    }
    let width = screen.width.saturating_sub(4).max(24);
    let height = (items.len() as u16 + 2).min(screen.height.saturating_sub(2));
    if height < 3 {
        return Rect::default();
    }
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(
        Block::default().style(Style::default().bg(BG).fg(MUTED)),
        screen,
    );
    frame.render_widget(Clear, area);

    let label_width = ((area.width.saturating_sub(8)) / 2).max(1) as usize;
    let mut rows = Vec::with_capacity(items.len());
    for (i, (id, label)) in items.iter().enumerate() {
        let style = if i == selected {
            Style::default().fg(BORDER).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(MUTED)
        };
        let id = fit(id, label_width);
        let label = fit(label, label_width);
        rows.push(Line::from(vec![
            Span::styled(if i == selected { "▸ " } else { "  " }, style),
            Span::styled(format!("{id:<label_width$}"), style),
            Span::styled("  ·  ", Style::default().fg(MUTED)),
            Span::styled(label, style),
        ]));
    }
    let block = Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(MUTED),
        ))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    frame.render_widget(Paragraph::new(rows).block(block), area);
    area
}

fn fit(text: &str, width: usize) -> String {
    let mut value: String = text.chars().take(width).collect();
    value.push_str(&" ".repeat(width.saturating_sub(value.chars().count())));
    value
}

/// Entrada del selector de modelos de Pawan.
#[derive(Debug, Clone)]
pub struct PawanModel<'a> {
    pub provider: &'a str,
    pub id: &'a str,
    pub quality_score: u8,
}

/// Selector modal de Pawan con proveedor, id y puntuación de calidad.
pub fn render_pawan(
    frame: &mut Frame,
    screen: Rect,
    models: &[PawanModel<'_>],
    selected: usize,
    query: &str,
    source: &str,
) -> Rect {
    if screen.width < 44 || screen.height < 7 {
        return Rect::default();
    }
    let filtered = models
        .iter()
        .filter(|model| {
            query.is_empty()
                || model.id.to_lowercase().contains(&query.to_lowercase())
                || model
                    .provider
                    .to_lowercase()
                    .contains(&query.to_lowercase())
        })
        .collect::<Vec<_>>();
    let selected = selected.min(filtered.len().saturating_sub(1));
    let width = (screen.width * 50 / 100)
        .max(40)
        .min(screen.width.saturating_sub(4));
    let height = (filtered.len() as u16 + 4)
        .min(18)
        .min(screen.height.saturating_sub(2));
    if width < 40 || height < 4 {
        return Rect::default();
    }
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .title(" Model Picker (/model) ")
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(Clear, area);
    frame.render_widget(block.clone(), area);
    let inner = block.inner(area);
    let search_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Blue)),
        Span::styled(query.to_string(), Style::default().fg(Color::White)),
        Span::styled(
            "▌",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
        Span::styled(
            format!("  {source} / {} shown", filtered.len()),
            Style::default().fg(Color::Gray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(search_line),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );

    let list_area = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1),
    );
    let list_height = list_area.height as usize;
    let offset = selected.saturating_sub(list_height.saturating_sub(1));
    let visible = filtered
        .iter()
        .skip(offset)
        .take(list_height)
        .enumerate()
        .map(|(index, model)| {
            let is_selected = offset + index == selected;
            let provider = truncate_chars(model.provider, 12);
            let row_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let provider_style = if is_selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };
            let score_style = if is_selected {
                Style::default().fg(Color::Black).bg(Color::Blue)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {provider} "), provider_style),
                Span::styled(model.id.to_string(), row_style),
                Span::styled(format!("  q{} ", model.quality_score), score_style),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(visible), list_area);
    area
}

fn truncate_chars(value: &str, width: usize) -> String {
    let chars = value.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        value.to_string()
    } else {
        chars
            .into_iter()
            .take(width.saturating_sub(1))
            .chain(['…'])
            .collect()
    }
}
