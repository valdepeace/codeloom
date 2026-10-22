//! Muestrario de los ocho estados de tarea de A2A: glifo, etiqueta y estilo.
//!
//! Cabe señalar que la tabla sale de `AgentState::ALL`, la misma que consume el
//! rail de loom, de modo que el showcase y la aplicación no pueden divergir.

use crate::demo::Demo;
use batten::state::AgentState;
use batten::widgets::Theme;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct AgentStatesDemo;

impl AgentStatesDemo {
    pub fn new() -> Self {
        Self
    }
}

/// Estados que el runtime todavía no produce; se pintan igualmente.
fn sin_productor(estado: AgentState) -> bool {
    matches!(
        estado,
        AgentState::Submitted | AgentState::AuthRequired | AgentState::Rejected
    )
}

impl Demo for AgentStatesDemo {
    fn title(&self) -> &str {
        "agent states"
    }

    fn hint(&self) -> &str {
        "los ocho estados de A2A · «sin productor»: el runtime aún no los emite"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines = vec![Line::from(Span::styled(
            "glifo  estado          productor",
            theme.styles.muted,
        ))];
        for estado in AgentState::ALL {
            let productor = if sin_productor(estado) {
                "sin productor"
            } else {
                "runtime"
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<6}", estado.glyph()),
                    estado.style(&theme.styles),
                ),
                Span::styled(
                    format!("{:<16}", estado.label()),
                    estado.style(&theme.styles),
                ),
                Span::styled(productor, theme.styles.muted),
            ]));
        }
        f.render_widget(Paragraph::new(lines), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use batten::state::AgentState;
    use batten::widgets::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn agent_states_demo_lists_all_eight_states() {
        let mut demo = AgentStatesDemo::new();
        let mut term = Terminal::new(TestBackend::new(80, 12)).unwrap();
        term.draw(|f| {
            let area = f.area();
            demo.render(f, area, &Theme::test());
        })
        .unwrap();

        let buffer = term.backend().buffer();
        let filas: Vec<String> = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect();
        for estado in AgentState::ALL {
            assert!(
                filas
                    .iter()
                    .any(|fila| fila.contains(estado.label()) && fila.contains(estado.glyph())),
                "falta la fila de {}",
                estado.label()
            );
        }
    }
}
