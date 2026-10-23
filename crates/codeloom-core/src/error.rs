use crate::task::TaskState;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("config error: {0}")]
    Config(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("tool error: {0}")]
    Tool(String),
    #[error("store error: {0}")]
    Store(String),
    #[error("cancelled")]
    Cancelled,
    /// Equivale al `TaskNotFoundError` de A2A: el hijo no existe o no es de este padre.
    #[error("task not found: {0}")]
    TaskNotFound(i64),
    /// Equivale al `TaskNotCancelableError` de A2A: la tarea ya está en un estado terminal.
    #[error("task {child} is not cancelable: it is {state:?}")]
    TaskNotCancelable { child: i64, state: TaskState },
}

pub type Result<T> = std::result::Result<T, CoreError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_a2a_task_errors_say_which_task_and_why() {
        assert_eq!(CoreError::TaskNotFound(7).to_string(), "task not found: 7");
        let no_cancelable = CoreError::TaskNotCancelable {
            child: 7,
            state: TaskState::Completed,
        };
        assert_eq!(
            no_cancelable.to_string(),
            "task 7 is not cancelable: it is Completed"
        );
    }
}
