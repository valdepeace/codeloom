//! Widgets ratatui y tema. Sin crossterm.

#[cfg(feature = "ansi")]
pub mod ansi;
pub mod attachment_preview;
pub mod button;
pub mod chip;
pub mod command_suggestions;
pub mod composer;
pub mod diff;
pub mod file_changes;
pub mod funcionalidad_table;
pub mod fuzzy_search;
#[cfg(feature = "fx")]
pub mod fx;
mod gradient;
pub mod harness_picker;
pub mod help_overlay;
pub mod key_help;
#[cfg(feature = "links")]
pub mod link;
pub mod logo;
pub mod matrix_rain;
pub mod model_picker;
pub mod panel;
pub mod permission;
#[cfg(feature = "termprofile")]
pub mod profile;
pub mod profile_view;
pub mod progress;
pub mod question;
pub mod queue;
pub mod raster;
pub mod rule;
pub mod scrollbar;
pub mod session_list;
pub mod settings_panel;
pub mod sidebar;
pub mod spinner;
pub mod status;
pub mod status_bar;
mod text;
pub mod theme;
pub mod thinking;
pub mod toast;
pub mod tone;
pub mod tool_item;
pub mod transcript;
pub mod welcome;

pub use button::{button, button_row};
pub use chip::chip;
pub use composer::render as render_composer;
pub use key_help::key_help;
pub use panel::{render as render_panel, Panel};
pub use progress::{progress, progress_smooth};

pub use funcionalidad_table::render_funcionalidad_table;
pub use rule::rule;
pub use spinner::{Anim, Animatable, FRAME_INTERVAL};
pub use status::status;
pub use theme::{
    ChromeStyle, ColorDepth, ComposerStyle, Glyphs, LayoutStyle, Look, Styles, Theme, ThemeError,
    ThemePreset, ToolStyle,
};
pub use thinking::render as render_thinking;
pub use toast::render as render_toast;
pub use tool_item::render as render_tool;
pub use transcript::render as render_transcript;
