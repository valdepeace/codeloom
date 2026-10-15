//! Estado puro: sin ratatui, sin crossterm.

pub mod transcript;

pub use transcript::{Line, LineKind, SpanSpec, StyleToken, Transcript};
