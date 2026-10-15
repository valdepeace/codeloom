//! Demo de tool items en todos sus estados.

use crate::demo::Demo;
use batten::state::ToolItem;
use batten::widgets::{tool_item, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

/// Construye un `ToolItem` en la fase indicada (0..=5).
fn make(name: &str, phase: usize) -> ToolItem {
    let mut t = ToolItem::new(name, name);
    match phase % 6 {
        0 => {}                   // Pending
        1 => t.start(),           // Running
        2 => t.set_result(false), // Success
        3 => t.set_result(true),  // Error
        4 => {
            t.start();
            t.cancel();
        } // Canceled
        _ => {
            t.start();
            t.await_permission();
        } // AwaitingPermission
    }
    t
}

pub struct ToolItemDemo {
    names: Vec<&'static str>,
    phases: Vec<usize>,
    items: Vec<ToolItem>,
    selected: usize,
    anim: Anim,
}

impl ToolItemDemo {
    pub fn new() -> Self {
        let names = vec!["view", "bash", "edit", "grep", "glob", "write"];
        let phases = vec![0, 1, 2, 3, 4, 5];
        let items = names
            .iter()
            .zip(&phases)
            .map(|(n, p)| make(n, *p))
            .collect();
        Self {
            names,
            phases,
            items,
            selected: 0,
            anim: Anim::new(),
        }
    }
}

impl Demo for ToolItemDemo {
    fn title(&self) -> &str {
        "tool_item"
    }

    fn hint(&self) -> &str {
        "↓/↑: elegir · n: cambiar estado · Enter: expandir"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let n = self.items.len().max(1);
        let rows = Layout::vertical(vec![Constraint::Length(2); n]).split(area);
        for (i, item) in self.items.iter_mut().enumerate() {
            if i >= rows.len() {
                break;
            }
            tool_item::render(f, rows[i], item, &self.anim, theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Down => self.selected = (self.selected + 1).min(self.items.len() - 1),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Char('n') => {
                self.phases[self.selected] += 1;
                let name = self.names[self.selected];
                self.items[self.selected] = make(name, self.phases[self.selected]);
            }
            KeyCode::Enter | KeyCode::Char('x') => self.items[self.selected].toggle_expanded(),
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
    fn n_cycles_the_selected_state() {
        let mut d = ToolItemDemo::new();
        let before = d.items[0].status();
        d.handle_key(KeyCode::Char('n'));
        assert_ne!(d.items[0].status(), before);
    }

    #[test]
    fn arrows_move_the_selection_with_clamp() {
        let mut d = ToolItemDemo::new();
        d.handle_key(KeyCode::Up);
        assert_eq!(d.selected, 0);
        d.handle_key(KeyCode::Down);
        assert_eq!(d.selected, 1);
    }
}
