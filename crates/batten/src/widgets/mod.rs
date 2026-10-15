//! Widgets ratatui y tema. Sin crossterm.

pub mod spinner;
pub mod theme;
pub mod tool_item;
pub mod transcript;

pub use spinner::{Anim, Animatable, FRAME_INTERVAL};
pub use theme::{Glyphs, Styles, Theme, ThemeError};
pub use tool_item::render as render_tool;
pub use transcript::render as render_transcript;
