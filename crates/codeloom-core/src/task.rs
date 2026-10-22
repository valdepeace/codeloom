//! Proyección hacia los estados de tarea de A2A; el modelo interno no cambia.
//!
//! Cabe señalar que este módulo no fija los nombres del cable (en la v1.0 son
//! `TASK_STATE_*`; en la 0.3, `input-required` y compañía): esa traducción
//! corresponde al adaptador del borde.

use crate::job::JobStatus;
use crate::session::SubagentStatus;

/// Estado de una tarea según A2A.
///
/// Cabe señalar que se omite a propósito el valor `unspecified` de protobuf,
/// que es el valor cero del enumerado y no un estado real de la tarea.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
            // A2A carece de equivalente a «sin terminar». Se proyecta a fallo porque
            // la tarea quedó sin terminar cuando la aplicación se cerró.
            SubagentStatus::Unfinished => Self::Failed,
        }
    }
}

/// Proyección de un subagente a su estado A2A, con la espera de su padre.
///
/// Dícese de la marca `awaiting_input` que solo cobra sentido mientras el hijo
/// sigue en marcha: un hijo terminado ya no espera nada.
pub fn subagent_task_state(status: SubagentStatus, awaiting_input: bool) -> TaskState {
    if status == SubagentStatus::Running && awaiting_input {
        TaskState::InputRequired
    } else {
        TaskState::from(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn awaiting_input_only_matters_while_running() {
        assert_eq!(
            subagent_task_state(SubagentStatus::Running, true),
            TaskState::InputRequired
        );
        assert_eq!(
            subagent_task_state(SubagentStatus::Running, false),
            TaskState::Working
        );
        assert_eq!(
            subagent_task_state(SubagentStatus::Completed, true),
            TaskState::Completed
        );
        assert_eq!(
            subagent_task_state(SubagentStatus::Failed, true),
            TaskState::Failed
        );
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
    }

    #[test]
    fn unfinished_projects_to_failed() {
        assert_eq!(
            TaskState::from(SubagentStatus::Unfinished),
            TaskState::Failed
        );
        assert_eq!(
            subagent_task_state(SubagentStatus::Unfinished, true),
            TaskState::Failed
        );
    }
}
