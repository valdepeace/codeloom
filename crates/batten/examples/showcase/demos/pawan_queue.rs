//! Compact task queue from Pawan's standalone `QueuePanel`.

use crate::demo::Demo;
use batten::widgets::queue::{self, QueueEntry, QueueStatus};
use batten::widgets::{Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const ENTRIES: [QueueEntry<'static>; 7] = [
    QueueEntry {
        label: "scan src",
        status: QueueStatus::Done,
    },
    QueueEntry {
        label: "map modules",
        status: QueueStatus::Running,
    },
    QueueEntry {
        label: "read Cargo.toml",
        status: QueueStatus::Pending,
    },
    QueueEntry {
        label: "inspect tests",
        status: QueueStatus::Failed,
    },
    QueueEntry {
        label: "summarize",
        status: QueueStatus::Pending,
    },
    QueueEntry {
        label: "check docs",
        status: QueueStatus::Running,
    },
    QueueEntry {
        label: "report",
        status: QueueStatus::Pending,
    },
];

pub struct PawanQueueDemo {
    count: usize,
    tick: u64,
}

impl PawanQueueDemo {
    pub fn new() -> Self {
        Self {
            count: ENTRIES.len(),
            tick: 0,
        }
    }
}

impl Demo for PawanQueueDemo {
    fn title(&self) -> &str {
        "Pawan · task queue"
    }
    fn hint(&self) -> &str {
        "←/→: variar número de tareas · estados pending/running/done/failed"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let [heading, rows] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from("Pawan · compact subagent queue"),
                Line::from("El ancho y el alto limitan las filas visibles (máximo 3)."),
            ])
            .style(Style::default().fg(Color::White)),
            heading,
        );
        queue::render_pawan(frame, rows, &ENTRIES[..self.count], self.tick);
    }

    fn tick(&mut self, anim: &Anim) {
        self.tick = anim.frame;
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => self.count = self.count.saturating_sub(1).max(1),
            KeyCode::Right => self.count = (self.count + 1).min(ENTRIES.len()),
            _ => return false,
        }
        true
    }
}
