//! Diálogo de permiso de Pawan reproducido desde su snapshot de 80×24.

use crate::demo::Demo;
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct PawanPermissionDemo;

impl Demo for PawanPermissionDemo {
    fn title(&self) -> &str {
        "Pawan · permission"
    }

    fn hint(&self) -> &str {
        "Snapshot de Pawan · terminal de referencia: 80×24"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        batten::widgets::permission::render_pawan(f, area, "bash", "echo hello");
    }
}
