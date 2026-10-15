//! Demo de la composición completa: transcript + tool + sidebar + composer.

use crate::demo::Demo;
use batten::state::{Composer, Line, LineKind, Sidebar, ToolItem, Transcript, Viewport};
use batten::widgets::{composer, sidebar, tool_item, transcript, Anim, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::Frame;

pub struct ChatDemo {
    transcript: Transcript,
    viewport: Viewport,
    composer: Composer,
    sidebar: Sidebar,
    tool: ToolItem,
    anim: Anim,
    answering: bool,
    elapsed_frames: u32,
}

impl ChatDemo {
    pub fn new() -> Self {
        let mut transcript = Transcript::new();
        transcript.push(Line::new(LineKind::User, "lee hello.txt"));
        let mut sidebar = Sidebar::with_logo("batten");
        sidebar.push_section("estado", vec!["working".into()]);
        sidebar.push_section("modelo", vec!["mistral-nemo".into()]);
        let mut tool = ToolItem::new("c1", "view");
        tool.start();
        Self {
            transcript,
            viewport: Viewport::new(),
            composer: Composer::new(),
            sidebar,
            tool,
            anim: Anim::new(),
            answering: true,
            elapsed_frames: 0,
        }
    }
}

impl Demo for ChatDemo {
    fn title(&self) -> &str {
        "chat"
    }

    fn hint(&self) -> &str {
        "escribe · Enter: enviar · ↓/↑: scroll"
    }

    fn tick(&mut self, anim: &Anim) {
        self.anim = *anim;
        self.elapsed_frames = self.elapsed_frames.saturating_add(1);
        // A los ~25 frames (≈ FRAME_INTERVAL) la tool termina y llega la respuesta.
        if self.answering && self.elapsed_frames == 25 {
            self.tool.set_result(false);
            self.transcript
                .push(Line::new(LineKind::Assistant, "hola mundo"));
            self.answering = false;
        }
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let wide = area.width >= 60;
        let (body, side) = if wide {
            let cols = Layout::horizontal([Constraint::Min(1), Constraint::Length(28)]).split(area);
            (cols[0], Some(cols[1]))
        } else {
            (area, None)
        };

        let rows = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .split(body);
        tool_item::render(f, rows[0], &self.tool, &self.anim, theme);
        transcript::render(f, rows[1], &self.transcript, &self.viewport, theme);
        composer::render(f, rows[2], &self.composer, theme);
        if let Some(s) = side {
            sidebar::render(f, s, &self.sidebar, theme);
        }
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter => {
                if let Some(text) = self.composer.submit() {
                    self.transcript.push(Line::new(LineKind::User, text));
                    self.viewport.follow_bottom();
                }
            }
            KeyCode::Backspace => self.composer.backspace(),
            KeyCode::Down => self.viewport.scroll_down(1),
            KeyCode::Up => self.viewport.scroll_up(1),
            KeyCode::Char(c) => self.composer.push_char(c),
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
    fn enter_appends_a_user_line() {
        let mut d = ChatDemo::new();
        let before = d.transcript.len();
        d.handle_key(KeyCode::Char('h'));
        d.handle_key(KeyCode::Char('i'));
        d.handle_key(KeyCode::Enter);
        assert_eq!(d.transcript.len(), before + 1);
        assert_eq!(d.transcript.last_mut().unwrap().plain(), "hi");
    }
}
