//! Demo de la sección de ficheros modificados.

use crate::demo::Demo;
use batten::state::{FileChange, FileChanges};
use batten::widgets::{file_changes, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::Frame;

pub struct FileChangesDemo {
    changes: FileChanges,
    counter: usize,
}

impl FileChangesDemo {
    pub fn new() -> Self {
        let mut changes = FileChanges::new();
        for (path, added, removed) in [
            ("src/main.rs", 12, 3),
            ("crates/batten/src/widgets/sidebar.rs", 40, 8),
            (
                "docs/specs/2026-10-15-loom-modified-files-design.md",
                191,
                0,
            ),
        ] {
            changes.upsert(FileChange {
                path: path.into(),
                added,
                removed,
            });
        }
        Self {
            changes,
            counter: 0,
        }
    }
}

impl Demo for FileChangesDemo {
    fn title(&self) -> &str {
        "file_changes"
    }

    fn hint(&self) -> &str {
        "a: añadir fichero · d: quitar el último"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        file_changes::render(f, area, "ficheros modificados", &self.changes, theme);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('a') => {
                self.counter += 1;
                self.changes.upsert(FileChange {
                    path: format!("src/nuevo-{}.rs", self.counter),
                    added: 10,
                    removed: 0,
                });
            }
            KeyCode::Char('d') => {
                self.changes.pop();
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
    fn a_adds_and_d_removes() {
        let mut d = FileChangesDemo::new();
        let before = d.changes.len();
        d.handle_key(KeyCode::Char('a'));
        assert_eq!(d.changes.len(), before + 1);
        d.handle_key(KeyCode::Char('d'));
        assert_eq!(d.changes.len(), before);
    }
}
