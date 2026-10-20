//! Estados comunes de tool de Crush.

use crate::demo::Demo;
use batten::state::{ToolItem, ToolStatus};
use batten::widgets::{tool_item, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const OUTPUT: &str = "Reading src/session.rs\nFound Session::load at line 42\nFound Session::save at line 87\nNo unsafe blocks in this file\nUTF-8, LF line endings\nRead completed successfully\nAdditional result line 1\nAdditional result line 2\nAdditional result line 3\nAdditional result line 4\nAdditional result line 5";

pub struct CrushToolCallDemo {
    status: ToolStatus,
    expanded: bool,
}

impl CrushToolCallDemo {
    pub fn new() -> Self {
        Self {
            status: ToolStatus::Success,
            expanded: false,
        }
    }

    fn item(&self) -> ToolItem {
        let mut item = ToolItem::new("call-01", "View");
        item.set_summary("src/session.rs");
        item.set_output(OUTPUT);
        item.expanded = self.expanded;
        match self.status {
            ToolStatus::Pending | ToolStatus::Running => item.start(),
            ToolStatus::AwaitingPermission => item.await_permission(),
            ToolStatus::Success => item.set_result(false),
            ToolStatus::Error => item.set_result(true),
            ToolStatus::Canceled => item.cancel(),
        }
        item
    }
}

impl Demo for CrushToolCallDemo {
    fn title(&self) -> &str {
        "Crush · tool call"
    }

    fn hint(&self) -> &str {
        "r: recorrer permiso/ejecución/éxito/error/cancelado · e: expandir"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [caption, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(Line::from(format!("View · estado {:?}", self.status)))
                .style(theme.styles.muted),
            caption,
        );
        tool_item::render_crush(frame, body, &self.item(), theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('r') => {
                self.status = match self.status {
                    ToolStatus::Pending => ToolStatus::Running,
                    ToolStatus::Running => ToolStatus::AwaitingPermission,
                    ToolStatus::AwaitingPermission => ToolStatus::Success,
                    ToolStatus::Success => ToolStatus::Error,
                    ToolStatus::Error | ToolStatus::Canceled => ToolStatus::Pending,
                };
            }
            KeyCode::Char('e') => self.expanded = !self.expanded,
            _ => return false,
        }
        true
    }
}
