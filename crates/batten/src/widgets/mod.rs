//! Widgets ratatui y tema. Sin crossterm.

pub mod chip;
pub mod composer;
pub mod file_changes;
mod gradient;
pub mod key_help;
pub mod logo;
pub mod rule;
pub mod scrollbar;
pub mod sidebar;
pub mod spinner;
pub mod status;
mod text;
pub mod theme;
pub mod tone;
pub mod tool_item;
pub mod transcript;

pub use chip::chip;
pub use composer::render as render_composer;
pub use key_help::key_help;
pub use rule::rule;
pub use spinner::{Anim, Animatable, FRAME_INTERVAL};
pub use status::status;
pub use theme::{Glyphs, Styles, Theme, ThemeError};
pub use tool_item::render as render_tool;
pub use transcript::render as render_transcript;
