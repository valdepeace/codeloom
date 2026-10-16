//! Estado puro: sin ratatui, sin crossterm.

pub mod composer;
pub mod diff;
pub mod file_changes;
pub mod history;
pub mod markdown;
pub mod permission;
pub mod selection;
pub mod sidebar;
pub mod stream;
pub mod toast;
pub mod tone;
pub mod tool;
pub mod transcript;
pub mod viewport;

pub use composer::Composer;
pub use diff::{Diff, DiffKind, DiffLine};
pub use file_changes::{FileChange, FileChanges};
pub use history::History;
pub use permission::{PermissionBody, PermissionChoice, PermissionDialog};
pub use selection::Selection;
pub use sidebar::{Logo, Section, Sidebar};
pub use stream::Stream;
pub use toast::{Toast, Toasts};
pub use tone::Tone;
pub use tool::{ToolItem, ToolStatus};
pub use transcript::{Line, LineKind, SpanSpec, StyleToken, Transcript};
pub use viewport::{Scroll, Viewport};
