//! Lista de sesiones de Mindfork, reconstruida desde `chat-list-dark-en.png`.

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

const BG: Color = Color::Rgb(15, 16, 19);
const FG: Color = Color::Rgb(201, 204, 210);
const MUTED: Color = Color::Rgb(133, 139, 146);
const GREEN: Color = Color::Rgb(111, 192, 130);
const BORDER: Color = Color::Rgb(108, 117, 128);
const KEY_BG: Color = Color::Rgb(33, 36, 42);

/// Sesión visible en la lista.
#[derive(Debug, Clone, Copy)]
pub struct SessionRow<'a> {
    pub title: &'a str,
    pub messages: usize,
    pub active: bool,
}

/// Lista de chats con búsqueda, selección y pie de atajos de Mindfork.
pub fn render_mindfork(
    frame: &mut Frame,
    area: Rect,
    rows: &[SessionRow<'_>],
    selected: usize,
    query: &str,
) {
    if area.width < 12 || area.height < 5 {
        return;
    }
    let outer = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .title(Span::styled(" ▤ Chats ", Style::default().fg(FG)))
        .style(Style::default().bg(BG));
    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let count = format!("dialogs: {}", rows.len());
    let count_width = count.width() as u16;
    frame.render_widget(
        Paragraph::new(Span::styled(count, Style::default().fg(MUTED))).alignment(Alignment::Right),
        Rect::new(
            area.right().saturating_sub(count_width + 2),
            area.y,
            count_width,
            1,
        ),
    );

    let [search, list, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(inner);
    let search_text = if query.is_empty() {
        "⌕  Search chats…".to_string()
    } else {
        format!("⌕  {query}")
    };
    let search_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(BG));
    let search_inner = search_block.inner(search);
    frame.render_widget(search_block, search);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(search_text, Style::default().fg(MUTED)),
            Span::styled("  titles  /", Style::default().fg(MUTED)),
        ]))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: false }),
        search_inner,
    );

    let visible = rows
        .iter()
        .enumerate()
        .take(list.height as usize)
        .map(|(index, row)| {
            let chosen = index == selected;
            let bg = if chosen { Color::Rgb(41, 43, 48) } else { BG };
            let mut spans = vec![Span::styled(
                if chosen { "▎ " } else { "  " },
                Style::default()
                    .fg(if row.active { GREEN } else { MUTED })
                    .bg(bg),
            )];
            if row.active {
                spans.push(Span::styled(
                    "● ",
                    Style::default()
                        .fg(GREEN)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::styled(
                    "● ",
                    Style::default().fg(Color::Rgb(63, 68, 77)).bg(bg),
                ));
            }
            spans.push(Span::styled(
                row.title.to_string(),
                Style::default()
                    .fg(if chosen {
                        FG
                    } else {
                        Color::Rgb(201, 204, 210)
                    })
                    .bg(bg)
                    .add_modifier(if chosen {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
            ));
            let right = format!("{} msg", row.messages);
            let used = spans.iter().map(|span| span.content.width()).sum::<usize>();
            let gap = usize::from(list.width).saturating_sub(used + right.width());
            spans.push(Span::raw(" ".repeat(gap)));
            spans.push(Span::styled(right, Style::default().fg(MUTED).bg(bg)));
            Line::from(spans)
        });
    frame.render_widget(Paragraph::new(visible.collect::<Vec<_>>()), list);

    let first = [
        ("PgUp/Dn", "select"),
        ("Enter", "open"),
        ("Ctrl+F", "search: titles"),
        ("F2", "rename"),
        ("Ctrl+R", "auto-name"),
    ];
    let second = [
        ("Ctrl+N", "new"),
        ("Ctrl+D", "clone"),
        ("Del", "delete"),
        ("F1", "help"),
        ("Tab", "sort: by modified"),
        ("Esc", "back"),
        ("Ctrl+Q", "quit"),
    ];
    let footer_lines = vec![
        footer_line(&first, inner.width as usize),
        footer_line(&second, inner.width as usize),
    ];
    frame.render_widget(
        Paragraph::new(footer_lines).style(Style::default().bg(BG)),
        footer,
    );
}

fn footer_line(items: &[(&str, &str)], width: usize) -> Line<'static> {
    let lengths = items
        .iter()
        .map(|(key, label)| key.width() + 2 + 1 + label.width())
        .collect::<Vec<_>>();
    let mut count = items.len();
    let mut total = lengths.iter().sum::<usize>();
    while count > 1 && total + count - 1 > width {
        count -= 1;
        total -= lengths[count];
    }
    let gap_count = count + 1;
    let spare = width.saturating_sub(total);
    let base = spare / gap_count;
    let extra = spare % gap_count;
    let key_style = Style::default().fg(MUTED).bg(KEY_BG);
    let label_style = Style::default().fg(MUTED);
    let mut spans = vec![Span::raw(" ".repeat(base + usize::from(extra > 0)))];
    for (index, (key, label)) in items.iter().take(count).enumerate() {
        spans.push(Span::styled(format!(" {key} "), key_style));
        spans.push(Span::styled(label.to_string(), label_style));
        if index + 1 < count {
            let extra_index = index + 1;
            spans.push(Span::raw(
                " ".repeat(base + usize::from(extra_index < extra.saturating_add(1))),
            ));
        }
    }
    Line::from(spans)
}
