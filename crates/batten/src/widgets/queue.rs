//! Compact task queue row based on Pawan's Ratatui queue panel.

use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueStatus {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct QueueEntry<'a> {
    pub label: &'a str,
    pub status: QueueStatus,
}

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Render Pawan's muted queue pieces, wrapping to at most three rows.
pub fn render_pawan(frame: &mut Frame, area: Rect, entries: &[QueueEntry<'_>], tick: u64) {
    if entries.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let spinner = SPINNER[((tick.saturating_mul(50) / 80) as usize) % SPINNER.len()];
    let budget = area.width as usize;
    let max_lines = (area.height as usize).clamp(1, 3);
    let mut rows: Vec<Vec<String>> = vec![Vec::new()];
    for entry in entries {
        let piece = match entry.status {
            QueueStatus::Pending => format!("○ {}", entry.label),
            QueueStatus::Running => format!("{spinner} {} ●", entry.label),
            QueueStatus::Done => format!("✓ {}", entry.label),
            QueueStatus::Failed => format!("✗ {}", entry.label),
        };
        let row = rows.last_mut().expect("queue starts with one row");
        let used = row
            .iter()
            .map(|s| s.len() + 1)
            .sum::<usize>()
            .saturating_sub(1);
        if !row.is_empty() && used.saturating_add(1).saturating_add(piece.len()) > budget {
            if rows.len() == max_lines {
                break;
            }
            rows.push(vec![piece]);
        } else {
            row.push(piece);
        }
    }
    for (offset, row) in rows.into_iter().enumerate() {
        let text = row.join(" ");
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(Color::Gray),
            ))),
            Rect::new(area.x, area.y.saturating_add(offset as u16), area.width, 1),
        );
    }
}
