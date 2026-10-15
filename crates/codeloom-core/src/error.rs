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
}

pub type Result<T> = std::result::Result<T, CoreError>;
