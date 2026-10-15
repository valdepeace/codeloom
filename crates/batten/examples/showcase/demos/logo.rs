//! Demo del logo paramétrico (fuente de bloques).

use crate::demo::Demo;
use batten::widgets::logo::{self, Wordmark};
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct LogoDemo {
    name: String,
}

impl LogoDemo {
    pub fn new() -> Self {
        Self {
            name: "batten".into(),
        }
    }
}

impl Demo for LogoDemo {
    fn title(&self) -> &str {
        "logo"
    }

    fn hint(&self) -> &str {
        "escribe para cambiar el nombre · Backspace: borrar"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let wm = Wordmark::new(&self.name);
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(logo::FONT_ROWS as u16 + 1),
            Constraint::Min(1),
        ])
        .split(area);
        f.render_widget(
            Paragraph::new(format!("nombre: {}  ({} cols)", self.name, wm.width())),
            rows[0],
        );
        if wm.width() <= area.width as usize {
            f.render_widget(Paragraph::new(logo::render(&wm, theme)), rows[1]);
        } else {
            f.render_widget(
                Paragraph::new(logo::compact(&self.name, area.width as usize, theme)),
                rows[1],
            );
        }
        f.render_widget(
            Paragraph::new(Line::from(
                "compacto (mismo widget, sin sitio para bloques):",
            )),
            rows[2],
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char(c) => self.name.push(c),
            KeyCode::Backspace => {
                self.name.pop();
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
    fn typing_appends_to_the_name() {
        let mut d = LogoDemo::new();
        for c in "hi".chars() {
            d.handle_key(KeyCode::Char(c));
        }
        assert!(d.name.starts_with("batten"));
        assert!(d.name.ends_with("hi"));
    }

    #[test]
    fn backspace_removes_a_char() {
        let mut d = LogoDemo::new();
        d.handle_key(KeyCode::Char('x'));
        d.handle_key(KeyCode::Backspace);
        assert!(!d.name.ends_with('x'));
    }
}
