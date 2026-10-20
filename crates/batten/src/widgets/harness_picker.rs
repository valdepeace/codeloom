//! Selector de harnesses basado en el menú de cambio de Martty.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

const BLUE: Color = Color::Rgb(79, 125, 255);
const BG: Color = Color::Rgb(28, 28, 28);
const MUTED: Color = Color::Rgb(128, 128, 128);

/// Dibuja el selector centrado con la configuración detallada debajo de cada opción.
pub fn render_martty(
    frame: &mut Frame,
    screen: Rect,
    items: &[(&str, &str)],
    selected: usize,
) -> Rect {
    if screen.width < 42 || screen.height < 8 || items.is_empty() {
        return Rect::default();
    }
    let height = (items.len() as u16 * 2 + 5).min(screen.height.saturating_sub(2));
    let width = screen.width.saturating_sub(8).max(34);
    let area = Rect::new(
        screen.x + screen.width.saturating_sub(width) / 2,
        screen.y + screen.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(14, 14, 18))),
        screen,
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Switch Harness · starts a new session ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BLUE))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::with_capacity(items.len() * 2 + 2);
    for (index, (name, details)) in items.iter().enumerate() {
        let current = index == 0;
        let active = index == selected;
        let color = if active { BLUE } else { Color::Gray };
        let suffix = if current { " (current)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled(
                if active { "› " } else { "  " },
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                if current { "● " } else { "○ " },
                Style::default().fg(color),
            ),
            Span::styled(
                format!("{name}{suffix}"),
                Style::default().fg(if active { Color::White } else { Color::Gray }),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!("    {details}"),
            Style::default().fg(MUTED),
        )));
    }
    lines.push(Line::from(Span::styled(
        "  + Add Harness…",
        Style::default().fg(Color::Gray),
    )));
    lines.push(Line::from(Span::styled(
        "  ↑/↓ select   enter apply   delete remove   esc cancel",
        Style::default().fg(MUTED),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
    area
}

/// Selector del catálogo de harnesses, agrupado por disponibilidad local.
pub fn render_martty_add(
    frame: &mut Frame,
    screen: Rect,
    installed: &[(&str, &str)],
    available: &[(&str, &str)],
    query: &str,
    selected: usize,
) -> Rect {
    if screen.width < 42 || screen.height < 10 {
        return Rect::default();
    }
    let width = screen.width.saturating_sub(8).max(34);
    let height = screen.height.saturating_sub(4);
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(14, 14, 18))),
        screen,
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title(" Add Harness ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BLUE))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let query = query.to_lowercase();
    let mut lines = vec![Line::from(Span::styled(
        format!("Search: {}▌", query),
        Style::default().fg(Color::White),
    ))];
    let mut index = 0;
    for (heading, entries, installed_group) in [
        ("Installed / configured", installed, true),
        ("Not downloaded", available, false),
    ] {
        lines.push(Line::from(Span::styled(
            format!("─ {heading} ─"),
            Style::default().fg(MUTED),
        )));
        for (name, detail) in entries {
            if !query.is_empty()
                && !name.to_lowercase().contains(&query)
                && !detail.to_lowercase().contains(&query)
            {
                continue;
            }
            let active = index == selected;
            let color = if active { BLUE } else { Color::Gray };
            lines.push(Line::from(vec![
                Span::styled(
                    if active { "› " } else { "  " },
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    if installed_group && index == 0 {
                        "● "
                    } else {
                        "○ "
                    },
                    Style::default().fg(color),
                ),
                Span::styled(
                    *name,
                    Style::default().fg(if active { Color::White } else { Color::Gray }),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!("    {detail}"),
                Style::default().fg(MUTED),
            )));
            index += 1;
        }
    }
    lines.push(Line::from(Span::styled(
        "↑/↓ select   enter apply   esc cancel",
        Style::default().fg(MUTED),
    )));
    let scroll = selected
        .saturating_mul(2)
        .min(lines.len().saturating_sub(inner.height as usize));
    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);
    area
}

/// Paso visual del flujo de eliminación de una configuración de harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalStep {
    Harness,
    Mode,
    Confirm,
}

/// Renderiza selección, modo de limpieza o confirmación del flujo documentado de Martty.
pub fn render_martty_removal(
    frame: &mut Frame,
    screen: Rect,
    step: RemovalStep,
    selected: usize,
) -> Rect {
    if screen.width < 48 || screen.height < 12 {
        return Rect::default();
    }
    let width = screen.width.saturating_sub(16).max(38);
    let height = screen.height.saturating_sub(8).max(6);
    let area = Rect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + (screen.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(14, 14, 18))),
        screen,
    );
    frame.render_widget(Clear, area);
    let title = match step {
        RemovalStep::Harness => " Remove Harness ",
        RemovalStep::Mode => " Remove Cortex Code · esc back ",
        RemovalStep::Confirm => " Remove Cortex Code? · enter remove · esc back ",
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BLUE))
        .style(Style::default().bg(BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = match step {
        RemovalStep::Harness => vec![
            Line::from(Span::styled(
                "Select a saved Harness configuration",
                Style::default().fg(MUTED),
            )),
            Line::from(vec![
                Span::styled(
                    if selected == 0 { "› " } else { "  " },
                    Style::default().fg(BLUE),
                ),
                Span::styled("Cortex Code", Style::default().fg(Color::White)),
                Span::styled(
                    "  configured · ~/.oma/bin/cortex-code/1.0.73/coco",
                    Style::default().fg(MUTED),
                ),
            ]),
        ],
        RemovalStep::Mode => vec![
            Line::from(Span::styled(
                "Choose what to remove",
                Style::default().fg(MUTED),
            )),
            Line::from(vec![
                Span::styled(
                    if selected == 0 { "› " } else { "  " },
                    Style::default().fg(BLUE),
                ),
                Span::styled(
                    "Remove configuration only",
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    "  Keep installed files, history and credentials",
                    Style::default().fg(MUTED),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    if selected == 1 { "› " } else { "  " },
                    Style::default().fg(BLUE),
                ),
                Span::styled(
                    "Remove configuration and private installation",
                    Style::default().fg(Color::White),
                ),
            ]),
            Line::from(Span::styled(
                "  ~/.oma/bin/cortex-code/1.0.73/coco",
                Style::default().fg(MUTED),
            )),
        ],
        RemovalStep::Confirm => vec![
            Line::from(Span::styled(
                "! Remove Cortex Code (cortex-code) and clear its saved default reference.",
                Style::default().fg(Color::Yellow),
            )),
            Line::from(Span::styled(
                "Configuration file",
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  ~/.oma/settings.json",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "Permanently delete private installation",
                Style::default().fg(Color::White),
            )),
            Line::from(Span::styled(
                "  ~/.oma/bin/cortex-code/1.0.73/coco",
                Style::default().fg(MUTED),
            )),
            Line::from(Span::styled(
                "History, credentials and shared caches are kept.",
                Style::default().fg(MUTED),
            )),
        ],
    };
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        match step {
            RemovalStep::Confirm => "enter remove   esc back",
            _ => "↑/↓ select   enter continue   esc back",
        },
        Style::default().fg(MUTED),
    )));
    frame.render_widget(Paragraph::new(lines), inner);
    area
}
