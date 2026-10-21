//! Proyección hacia los estados de tarea de A2A; el modelo interno no cambia.

use crate::job::JobStatus;
use crate::session::SubagentStatus;
use serde::{Deserialize, Serialize};

/// Estado de una tarea según A2A v1.0.
///
/// Cabe señalar que se omite a propósito el valor `unspecified` de protobuf,
/// que es el valor cero del enumerado y no un estado real de la tarea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Submitted,
    Working,
    InputRequired,
    AuthRequired,
    Completed,
    Failed,
    Canceled,
    Rejected,
}

impl TaskState {
    /// Dícese de los estados que ya no admiten transiciones posteriores.
    ///
    /// `InputRequired` y `AuthRequired` no son terminales: la tarea sigue viva,
    /// solo que pausada a la espera de un actor externo.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Canceled | Self::Rejected
        )
    }

    /// Literal del protocolo; coincide con la serialización de `serde`.
    pub fn as_str(self) -> &'static str {
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

impl From<JobStatus> for TaskState {
    fn from(status: JobStatus) -> Self {
        match status {
            // Una parada pedida pero aún no terminada sigue siendo trabajo en curso.
            JobStatus::Running | JobStatus::Stopping => Self::Working,
            JobStatus::Completed => Self::Completed,
            JobStatus::Failed => Self::Failed,
            JobStatus::Killed => Self::Canceled,
        }
    }
}

impl From<SubagentStatus> for TaskState {
    fn from(status: SubagentStatus) -> Self {
        match status {
            SubagentStatus::Running => Self::Working,
            SubagentStatus::Completed => Self::Completed,
            SubagentStatus::Failed => Self::Failed,
            SubagentStatus::Killed => Self::Canceled,
            // A2A carece de equivalente a «interrumpida». Se proyecta a fallo porque
            // la tarea quedó sin terminar cuando la aplicación se cerró.
            SubagentStatus::Interrupted => Self::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_protocol_names() {
        assert_eq!(
            serde_json::to_string(&TaskState::InputRequired).unwrap(),
            "\"input-required\""
        );
        assert_eq!(
            serde_json::to_string(&TaskState::AuthRequired).unwrap(),
            "\"auth-required\""
        );
        assert_eq!(
            serde_json::to_string(&TaskState::Canceled).unwrap(),
            "\"canceled\""
        );
        assert_eq!(
            serde_json::to_string(&TaskState::Submitted).unwrap(),
            "\"submitted\""
        );
        let back: TaskState = serde_json::from_str("\"input-required\"").unwrap();
        assert_eq!(back, TaskState::InputRequired);
    }

    #[test]
    fn as_str_matches_serialization() {
        for s in [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::InputRequired,
            TaskState::AuthRequired,
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::Rejected,
        ] {
            assert_eq!(
                serde_json::to_string(&s).unwrap(),
                format!("\"{}\"", s.as_str())
            );
        }
    }

    #[test]
    fn terminal_states_are_exactly_the_settled_ones() {
        for s in [
            TaskState::Completed,
            TaskState::Failed,
            TaskState::Canceled,
            TaskState::Rejected,
        ] {
            assert!(s.is_terminal());
        }
        for s in [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::InputRequired,
            TaskState::AuthRequired,
        ] {
            assert!(!s.is_terminal());
        }
    }

    #[test]
    fn job_status_projects_to_task_state() {
        assert_eq!(TaskState::from(JobStatus::Running), TaskState::Working);
        assert_eq!(TaskState::from(JobStatus::Stopping), TaskState::Working);
        assert_eq!(TaskState::from(JobStatus::Completed), TaskState::Completed);
        assert_eq!(TaskState::from(JobStatus::Failed), TaskState::Failed);
        assert_eq!(TaskState::from(JobStatus::Killed), TaskState::Canceled);
    }

    #[test]
    fn subagent_status_projects_to_task_state() {
        assert_eq!(TaskState::from(SubagentStatus::Running), TaskState::Working);
        assert_eq!(
            TaskState::from(SubagentStatus::Completed),
            TaskState::Completed
        );
        assert_eq!(TaskState::from(SubagentStatus::Failed), TaskState::Failed);
        assert_eq!(TaskState::from(SubagentStatus::Killed), TaskState::Canceled);
        assert_eq!(
            TaskState::from(SubagentStatus::Interrupted),
            TaskState::Failed
        );
    }
}
