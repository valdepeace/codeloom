//! Tool call agrupada de Pawan basada en el renderer de mensajes.

use crate::demo::Demo;
use batten::state::ToolItem;
use batten::widgets::{tool_item, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const OUTPUT: &str = "Read src/main.rs\nFound fn main() at line 4\nFound App::run() at line 8\nFound setup_terminal() at line 21\nFound restore_terminal() at line 34\nNo unsafe blocks in this file\nImports: std::io, crossterm, ratatui\nFile length: 42 lines\nUTF-8, LF line endings\nRead completed successfully";

pub struct PawanToolCallDemo {
    running: bool,
    expanded: bool,
}

impl PawanToolCallDemo {
    pub fn new() -> Self {
        Self {
            running: false,
            expanded: false,
        }
    }

    fn item(&self) -> ToolItem {
        let mut item = ToolItem::new("read-main", "read_file");
        item.set_summary("src/main.rs");
        item.set_output(OUTPUT);
        if self.running {
            item.start();
        } else {
            item.set_result(false);
        }
        item.expanded = self.expanded;
        item
    }
}

impl Demo for PawanToolCallDemo {
    fn title(&self) -> &str {
        "Pawan · tool call"
    }

    fn hint(&self) -> &str {
        "r: ejecución/completada · e: plegar/desplegar salida"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let [title, card] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(Line::from("read_file · resultado de ejemplo"))
                .style(Style::default().fg(Color::White)),
            title,
        );
        tool_item::render_pawan(frame, card, &self.item(), 148);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('r') => self.running = !self.running,
            KeyCode::Char('e') => self.expanded = !self.expanded,
            _ => return false,
        }
        true
    }
}
