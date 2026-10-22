//! Estado visual de una tarea de agente: los ocho estados de A2A, cada uno con
//! su glifo de una columna y su etiqueta.
//!
//! Estado puro, sin ratatui: el color se resuelve aparte, en `widgets::theme`,
//! a partir de los tokens del tema.

/// Estado visual de una tarea, según los estados de A2A.
///
/// Cabe señalar que no todos tienen productor hoy: `Submitted`, `AuthRequired`
/// y `Rejected` se pintan igualmente, aunque el runtime aún no los genere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Submitted,
    Working,
    InputRequired,
    AuthRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}

impl AgentState {
    /// Los ocho estados, en el orden de la tabla de A2A.
    pub const ALL: [AgentState; 8] = [
        AgentState::Submitted,
        AgentState::Working,
        AgentState::InputRequired,
        AgentState::AuthRequired,
        AgentState::Completed,
        AgentState::Failed,
        AgentState::Canceled,
        AgentState::Rejected,
    ];

    /// Glifo de una sola columna. Son distintos entre sí, de modo que el estado
    /// se distingue también sin color. El rail no ensancha la línea al pintarlo.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Submitted => "○",
            Self::Working => "●",
            Self::InputRequired => "?",
            Self::AuthRequired => "⚿",
            Self::Completed => "✓",
            Self::Failed => "✗",
            Self::Canceled => "⊘",
            Self::Rejected => "⊖",
        }
    }

    /// Etiqueta del estado, con los nombres de A2A en minúsculas.
    pub fn label(self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Working => "working",
            Self::InputRequired => "input-required",
            Self::AuthRequired => "auth-required",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Canceled => "canceled",
            Self::Rejected => "rejected",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn agent_state_all_has_eight_unique_glyphs_and_labels() {
        assert_eq!(AgentState::ALL.len(), 8);

        let glyphs: HashSet<&str> = AgentState::ALL.iter().map(|s| s.glyph()).collect();
        assert_eq!(glyphs.len(), 8, "glifos repetidos: {glyphs:?}");

        let labels: HashSet<&str> = AgentState::ALL.iter().map(|s| s.label()).collect();
        assert_eq!(labels.len(), 8, "etiquetas repetidas: {labels:?}");

        let esperadas = [
            (AgentState::Submitted, "○", "submitted"),
            (AgentState::Working, "●", "working"),
            (AgentState::InputRequired, "?", "input-required"),
            (AgentState::AuthRequired, "⚿", "auth-required"),
            (AgentState::Completed, "✓", "completed"),
            (AgentState::Failed, "✗", "failed"),
            (AgentState::Canceled, "⊘", "canceled"),
            (AgentState::Rejected, "⊖", "rejected"),
        ];
        for (estado, glifo, etiqueta) in esperadas {
            assert_eq!(estado.glyph(), glifo, "glifo de {etiqueta}");
            assert_eq!(estado.label(), etiqueta, "etiqueta de {glifo}");
        }
    }

    #[test]
    fn agent_state_glyphs_are_one_column_wide() {
        for estado in AgentState::ALL {
            assert_eq!(
                estado.glyph().width(),
                1,
                "el glifo de {} ocupa más de una columna",
                estado.label()
            );
        }
    }
}
