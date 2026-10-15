//! Estado puro: sin ratatui, sin crossterm.

pub mod stream;
pub mod transcript;

pub use stream::Stream;
pub use transcript::{Line, LineKind, SpanSpec, StyleToken, Transcript};
