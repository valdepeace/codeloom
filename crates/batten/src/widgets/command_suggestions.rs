//! Completion flotante de comandos de Martty.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Comando o skill para el menú de sugerencias de Martty.
#[derive(Debug, Clone, Copy)]
pub struct MarttyCommand<'a> {
    pub usage: &'a str,
    pub description: &'a str,
    pub section: Option<&'a str>,
    pub skill: bool,
}

/// Dibuja el popup de commands/skills justo encima del composer.
pub fn render_martty(
    frame: &mut Frame,
    input: Rect,
    chat: Rect,
    commands: &[MarttyCommand<'_>],
    selected: usize,
    theme: &crate::widgets::Theme,
) -> Rect {
    const MAX_ROWS: usize = 12;
    if commands.is_empty() || input.width < 16 || chat.height < 3 {
        return Rect::default();
    }
    let count = commands.len();
    let selected = selected.min(count - 1);
    let visible = MAX_ROWS
        .min(count)
        .min(chat.height.saturating_sub(2) as usize);
    if visible == 0 {
        return Rect::default();
    }
    let start = selected
        .saturating_add(1)
        .saturating_sub(visible)
        .min(count - visible);
    let width = 64u16.min(input.width.saturating_sub(2));
    let height = visible as u16 + 2;
    if width < 16 {
        return Rect::default();
    }
    let area = Rect::new(input.x + 2, input.y.saturating_sub(height), width, height);
    frame.render_widget(Clear, area);

    let name_width = commands
        .iter()
        .map(|command| command.usage.width())
        .max()
        .unwrap_or(14)
        .clamp(14, 26);
    let mut lines = Vec::new();
    for (index, command) in commands.iter().enumerate().skip(start).take(visible) {
        let active = index == selected;
        let marker_style = Style::default().fg(theme
            .style(crate::state::StyleToken::Accent)
            .fg
            .unwrap_or(Color::Blue));
        let name_style = if active {
            marker_style.add_modifier(Modifier::BOLD)
        } else if command.skill {
            theme.style(crate::state::StyleToken::Muted)
        } else {
            theme.style(crate::state::StyleToken::Default)
        };
        let section_width = command.section.map_or(0, |section| section.width() + 3);
        let desc_width = usize::from(width).saturating_sub(name_width + 5 + section_width);
        let desc = if command.skill {
            format!("{} · skill", command.description)
        } else {
            command.description.to_string()
        };
        let mut spans = vec![
            Span::styled(if active { "▸ " } else { "  " }, marker_style),
            Span::styled(ellipsize(command.usage, name_width), name_style),
        ];
        if let Some(section) = command.section {
            spans.push(Span::styled(
                format!(" {section} ·"),
                theme.style(crate::state::StyleToken::Muted),
            ));
        }
        spans.push(Span::styled(
            format!(" {}", ellipsize(&desc, desc_width.saturating_sub(1))),
            theme.style(crate::state::StyleToken::Muted),
        ));
        lines.push(Line::from(spans));
    }
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.style(crate::state::StyleToken::Border))
        .title(Span::styled(
            if commands.iter().any(|command| command.skill) {
                " commands · skills "
            } else {
                " commands "
            },
            theme.style(crate::state::StyleToken::Muted),
        ))
        .style(theme.style(crate::state::StyleToken::Default));
    if count > visible {
        let arrows = match (start > 0, start + visible < count) {
            (true, true) => "↑↓",
            (true, false) => "↑",
            _ => "↓",
        };
        block = block.title_bottom(Span::styled(
            format!(" {}/{count} {arrows} ", selected + 1),
            theme.style(crate::state::StyleToken::Muted),
        ));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    area
}

fn ellipsize(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + char_width > width - 1 {
            break;
        }
        used += char_width;
        result.push(ch);
    }
    result.push('…');
    result
}
