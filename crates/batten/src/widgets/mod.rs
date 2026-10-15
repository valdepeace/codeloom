//! Widgets ratatui y tema. Sin crossterm.

pub mod spinner;
pub mod theme;
pub mod transcript;

pub use spinner::{Anim, Animatable, FRAME_INTERVAL};
pub use theme::{Glyphs, Styles, Theme, ThemeError};
pub use transcript::render;
