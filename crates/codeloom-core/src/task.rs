//! Estado de tarea de A2A: es el modelo de un subagente, no una proyección suya.
//!
//! Cabe señalar que el estado persistido usa los nombres del proto v1.0
//! (`TASK_STATE_*`, vía `proto_name`). La traducción a los nombres del cable
//! (y a los de la v0.3, `input-required` y compañía) corresponde al adaptador
//! del borde; la interfaz deriva de aquí su presentación, sin reescribirlo.

use crate::a2a::Message;
use crate::job::JobStatus;
use crate::message::now_ms;

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
    /// Los ocho estados reales, en el orden de la tabla de A2A.
    pub const ALL: [TaskState; 8] = [
        TaskState::Submitted,
        TaskState::Working,
        TaskState::InputRequired,
        TaskState::AuthRequired,
        TaskState::Completed,
        TaskState::Failed,
        TaskState::Canceled,
        TaskState::Rejected,
    ];

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

    /// Nombre del proto v1.0 con el que se persiste el estado, p. ej. `TASK_STATE_WORKING`.
    pub fn proto_name(self) -> &'static str {
        match self {
            Self::Submitted => "TASK_STATE_SUBMITTED",
            Self::Working => "TASK_STATE_WORKING",
            Self::InputRequired => "TASK_STATE_INPUT_REQUIRED",
            Self::AuthRequired => "TASK_STATE_AUTH_REQUIRED",
            Self::Completed => "TASK_STATE_COMPLETED",
            Self::Failed => "TASK_STATE_FAILED",
            Self::Canceled => "TASK_STATE_CANCELED",
            Self::Rejected => "TASK_STATE_REJECTED",
        }
    }

    /// Estado a partir de su nombre del proto v1.0. Las cadenas antiguas no pasan por aquí:
    /// el almacén las traduce aparte.
    pub fn from_proto_name(name: &str) -> Option<TaskState> {
        Self::ALL
            .into_iter()
            .find(|state| state.proto_name() == name)
    }

    /// A2A no fija una tabla de transiciones: lo único que impone es que un estado
    /// terminal no se reinicie. Por eso cualquier otro destino se admite.
    pub fn can_transition_to(self, _next: TaskState) -> bool {
        !self.is_terminal()
    }
}

/// Estado de una tarea con su mensaje y su marca de tiempo, como el `TaskStatus` del proto.
///
/// Cabe señalar que `message` es un `Message` de A2A con sus `Part`s, como en el proto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStatus {
    pub state: TaskState,
    pub message: Option<Message>,
    pub timestamp_ms: i64,
}

impl TaskStatus {
    /// Estado sin mensaje, con la marca de tiempo del momento de crearlo.
    pub fn new(state: TaskState) -> Self {
        Self {
            state,
            message: None,
            timestamp_ms: now_ms(),
        }
    }

    /// Añade el mensaje de texto que explica el estado. Se guarda como un `Message` del agente
    /// con una sola `Part` de texto, de modo que quien produce el estado no cambia.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(Message::agent_text(message));
        self
    }

    /// Texto del mensaje de estado, o `None` si no tiene mensaje.
    pub fn message_text(&self) -> Option<String> {
        self.message.as_ref().map(Message::text)
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
    fn proto_names_roundtrip_for_all_states() {
        for state in TaskState::ALL {
            assert_eq!(
                TaskState::from_proto_name(state.proto_name()),
                Some(state),
                "ida y vuelta de {state:?}"
            );
            assert!(state.proto_name().starts_with("TASK_STATE_"));
        }
        let mut nombres: Vec<&str> = TaskState::ALL.iter().map(|s| s.proto_name()).collect();
        nombres.sort_unstable();
        nombres.dedup();
        assert_eq!(nombres.len(), 8, "nombres repetidos");
    }

    #[test]
    fn from_proto_name_rejects_unknown() {
        assert_eq!(TaskState::from_proto_name("running"), None);
        assert_eq!(TaskState::from_proto_name("TASK_STATE_UNSPECIFIED"), None);
        assert_eq!(TaskState::from_proto_name(""), None);
    }

    #[test]
    fn terminal_states_cannot_transition() {
        for from in TaskState::ALL.into_iter().filter(|s| s.is_terminal()) {
            for to in TaskState::ALL {
                assert!(!from.can_transition_to(to), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn non_terminal_states_can_transition_anywhere() {
        for from in TaskState::ALL.into_iter().filter(|s| !s.is_terminal()) {
            for to in TaskState::ALL {
                assert!(from.can_transition_to(to), "{from:?} -> {to:?}");
            }
        }
    }

    #[test]
    fn task_status_new_sets_timestamp() {
        let antes = crate::message::now_ms();
        let status = TaskStatus::new(TaskState::Working);
        let despues = crate::message::now_ms();
        assert_eq!(status.state, TaskState::Working);
        assert_eq!(status.message, None);
        assert!(antes <= status.timestamp_ms && status.timestamp_ms <= despues);

        let con_texto = TaskStatus::new(TaskState::Failed).with_message("roto");
        assert_eq!(con_texto.message_text().as_deref(), Some("roto"));
    }
}
