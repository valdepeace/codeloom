//! Estado puro: sin ratatui, sin crossterm.

pub mod composer;
pub mod file_changes;
pub mod sidebar;
pub mod stream;
pub mod tool;
pub mod transcript;
pub mod viewport;

pub use composer::Composer;
pub use file_changes::{FileChange, FileChanges};
pub use sidebar::{Logo, Section, Sidebar};
pub use stream::Stream;
pub use tool::{ToolItem, ToolStatus};
pub use transcript::{Line, LineKind, SpanSpec, StyleToken, Transcript};
pub use viewport::{Scroll, Viewport};
