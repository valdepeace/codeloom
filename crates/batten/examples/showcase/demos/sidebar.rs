//! Demo del sidebar genérico (logo + secciones).

use crate::demo::Demo;
use batten::state::Sidebar;
use batten::widgets::{sidebar, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct SidebarDemo {
    sidebar: Sidebar,
    extra: usize,
}

impl SidebarDemo {
    pub fn new() -> Self {
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("estado", vec!["working".into()]);
        sidebar.push_section("modelo", vec!["mistral-nemo".into()]);
        Self { sidebar, extra: 0 }
    }
}

impl Demo for SidebarDemo {
    fn title(&self) -> &str {
        "sidebar"
    }

    fn hint(&self) -> &str {
        "+: añadir sección · -: quitar la última"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        sidebar::render(f, area, &self.sidebar, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('+') => {
                self.extra += 1;
                self.sidebar
                    .push_section(format!("extra {}", self.extra), vec!["—".into()]);
            }
            KeyCode::Char('-') => {
                self.sidebar.sections.pop();
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn plus_and_minus_add_and_remove_sections() {
        let mut d = SidebarDemo::new();
        let before = d.sidebar.sections.len();
        d.handle_key(KeyCode::Char('+'));
        assert_eq!(d.sidebar.sections.len(), before + 1);
        d.handle_key(KeyCode::Char('-'));
        assert_eq!(d.sidebar.sections.len(), before);
    }
}
