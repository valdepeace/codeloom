//! Catálogo «CodeLoom tools»: una entrada por herramienta real de CodeLoom.
//!
//! Cada demo toma su llamada de ejemplo de `batten::state::tool_atom_fixtures`
//! y la pinta con la misma función que usa la app (`tool_lines_with` a través de
//! la fixture, y `transcript::render`). No se dibuja nada a mano.

use crate::demo::Demo;
use batten::state::tool_atom_fixtures::{fixtures, FixtureState, ToolFixture};
use batten::state::{EntryId, Transcript, Viewport};
use batten::widgets::{transcript, Theme};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct CodeLoomToolDemo {
    title: String,
    fixture: ToolFixture,
    state: FixtureState,
    expanded: bool,
    transcript: Transcript,
    entry: EntryId,
    viewport: Viewport,
    layout: transcript::Layout,
}

impl CodeLoomToolDemo {
    /// Demo de la herramienta `name`. Panica si no hay fixture para ella, lo que
    /// el test `every_real_tool_has_a_fixture` impide.
    pub fn new(name: &str) -> Self {
        let fixture = fixtures()
            .into_iter()
            .find(|fixture| fixture.name == name)
            .unwrap_or_else(|| panic!("sin fixture para {name}"));
        let state = FixtureState::Success;
        let mut transcript = Transcript::new();
        let entry = transcript.push_lines(fixture.lines(state, false));
        Self {
            title: format!("CodeLoom · tool · {name}"),
            fixture,
            state,
            expanded: false,
            transcript,
            entry,
            viewport: Viewport::new(),
            layout: transcript::Layout::new(),
        }
    }

    /// Rehace las líneas de la llamada con el estado y el plegado actuales.
    fn refresh(&mut self) {
        let lines = self.fixture.lines(self.state, self.expanded);
        self.transcript.replace(self.entry, lines);
    }
}

impl Demo for CodeLoomToolDemo {
    fn title(&self) -> &str {
        &self.title
    }

    fn hint(&self) -> &str {
        "r: cambiar estado (pendiente, en curso, éxito, error, permiso, cancelado) · e: plegada/expandida"
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let [header, body, footer] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .areas(area);
        let fold = if self.expanded {
            "expandida"
        } else {
            "plegada"
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{}  ", self.title), theme.styles.tool_name),
                Span::styled("estado: ", theme.styles.muted),
                Span::styled(self.state.label(), theme.styles.accent),
                Span::styled(format!(" · {fold}"), theme.styles.muted),
            ])),
            header,
        );
        transcript::render(
            frame,
            body,
            &self.transcript,
            &mut self.viewport,
            &mut self.layout,
            None,
            theme,
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "r: cambiar estado · e: plegar/expandir",
                theme.styles.muted,
            ))),
            footer,
        );
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('r') => self.state = self.state.next(),
            KeyCode::Char('e') => self.expanded = !self.expanded,
            _ => return false,
        }
        self.refresh();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_cycles_state_and_e_toggles_expansion() {
        let mut demo = CodeLoomToolDemo::new("bash");
        assert_eq!(demo.state, FixtureState::Success);

        demo.handle_key(KeyCode::Char('r'));
        assert_eq!(demo.state, FixtureState::Error);

        // Seis pulsaciones de `r` dan la vuelta completa al ciclo.
        for _ in 0..5 {
            demo.handle_key(KeyCode::Char('r'));
        }
        assert_eq!(demo.state, FixtureState::Success);

        let folded = demo.transcript.entry_lines(0).len();
        demo.handle_key(KeyCode::Char('e'));
        assert!(demo.expanded);
        assert!(demo.transcript.entry_lines(0).len() > folded);
        demo.handle_key(KeyCode::Char('e'));
        assert!(!demo.expanded);
    }

    #[test]
    fn unknown_keys_are_not_handled() {
        let mut demo = CodeLoomToolDemo::new("view");
        assert!(!demo.handle_key(KeyCode::Char('x')));
    }
}
