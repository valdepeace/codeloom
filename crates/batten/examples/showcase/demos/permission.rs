//! Demo del diálogo de permisos: un edit con diff y un bash con su comando.

use crate::demo::Demo;
use batten::state::{Diff, PermissionBody, PermissionChoice, PermissionDialog};
use batten::widgets::fx::{dialog_open, Effect, FxDuration};
use batten::widgets::permission;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const BEFORE: &str = "fn main() {\n    let x = 1;\n    println!(\"{}\", x);\n}\n";
const AFTER: &str =
    "fn main() {\n    let x = 2;\n    let y = x * 2;\n    println!(\"{}\", y);\n}\n";

pub struct PermissionDemo {
    dialogs: [PermissionDialog; 2],
    current: usize,
    pending_fx: bool,
    effect: Option<Effect>,
    last: Option<PermissionChoice>,
}

impl PermissionDemo {
    pub fn new() -> Self {
        Self {
            dialogs: [
                PermissionDialog::new(
                    "edit",
                    "src/main.rs",
                    PermissionBody::Diff(Diff::new(BEFORE, AFTER, 3)),
                ),
                PermissionDialog::new(
                    "bash",
                    "",
                    PermissionBody::Text("cargo test --workspace && git status".into()),
                ),
            ],
            current: 0,
            pending_fx: true,
            effect: None,
            last: None,
        }
    }
}

impl Demo for PermissionDemo {
    fn title(&self) -> &str {
        "permission"
    }

    fn hint(&self) -> &str {
        "n: otro caso · ←→ elegir · ↑↓ scroll · enter · o: efecto"
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        let d = &mut self.dialogs[self.current];
        match code {
            KeyCode::Char('n') => {
                self.current = (self.current + 1) % 2;
                self.pending_fx = true;
            }
            KeyCode::Left => d.prev(),
            KeyCode::Right => d.next(),
            KeyCode::Up => d.scroll_by(-1),
            KeyCode::Down => d.scroll_by(1),
            KeyCode::Enter => self.last = Some(d.choice()),
            KeyCode::Char('o') => self.pending_fx = true,
            _ => return false,
        }
        true
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let last = match self.last {
            Some(c) => format!("última decisión: {c:?}"),
            None => "sin decisión todavía".into(),
        };
        let bg = vec![
            Line::from("Simulación: el agente quiere ejecutar una tool."),
            Line::from(last),
        ];
        f.render_widget(Paragraph::new(bg), area);
        permission::render(f, area, &self.dialogs[self.current], theme);
        if self.pending_fx {
            self.effect = Some(dialog_open(theme));
            self.pending_fx = false;
        }
        if let Some(e) = self.effect.as_mut() {
            e.process(
                FxDuration::from_millis(50),
                f.buffer_mut(),
                permission::area(area, &self.dialogs[self.current]),
            );
        }
    }
}
