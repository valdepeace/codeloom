//! Estado puro: sin ratatui, sin crossterm.

pub mod composer;
pub mod stream;
pub mod tool;
pub mod transcript;
pub mod viewport;

pub use composer::Composer;
pub use stream::Stream;
pub use tool::{ToolItem, ToolStatus};
pub use transcript::{Line, LineKind, SpanSpec, StyleToken, Transcript};
pub use viewport::Viewport;
