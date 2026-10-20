//! Tool call de Martty: estado, request y viewport del resultado.

use crate::demo::Demo;
use batten::widgets::{tool_item, Anim, Theme, ThemePreset};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const REQUEST: &str = r#"{"path":"src/main.rs","include":"entry points and terminal setup"}"#;
const OUTPUT: &str = "use std::io;\nuse ratatui::Terminal;\n\nfn main() {\n    let mut app = App::new();\n    app.run()?;\n}\n// 42 lines total";

pub struct MarttyToolCallDemo {
    running: bool,
    expanded: bool,
}

impl MarttyToolCallDemo {
    pub fn new() -> Self {
        Self {
            running: true,
            expanded: false,
        }
    }
}

impl Demo for MarttyToolCallDemo {
    fn title(&self) -> &str {
        "Martty · tool call"
    }

    fn hint(&self) -> &str {
        "r: ejecución/completada · e: plegar/desplegar resultado"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Martty);
        let [title, body] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
        frame.render_widget(
            Paragraph::new(Line::from("read_file · ejecución del agente"))
                .style(theme.style(batten::state::StyleToken::Heading)),
            title,
        );
        let call = tool_item::MarttyToolCall {
            name: "read_file",
            title: "src/main.rs",
            request: REQUEST,
            output: OUTPUT,
            error: None,
            agent: Some("root"),
            running: self.running,
            expanded: self.expanded,
        };
        tool_item::render_martty(frame, body, &call, &Anim::default(), &theme);
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
