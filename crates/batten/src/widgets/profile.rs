//! Rebaja de los colores del tema según lo que soporte el terminal.

use super::theme::{ColorDepth, Styles, Theme};
use ratatui::style::Style;
pub use termprofile::TermProfile;

impl Theme {
    /// Adapta todos los estilos al perfil del terminal (truecolor → 256 → 16 →
    /// sin color). La detección es cosa de la app: `TermProfile::detect`.
    pub fn for_profile(&self, profile: TermProfile) -> Self {
        let a = |st: Style| -> Style { profile.adapt_style(st) };
        let s = &self.styles;
        let styles = Styles {
            default: a(s.default),
            muted: a(s.muted),
            dim: a(s.dim),
            heading: a(s.heading),
            user: a(s.user),
            assistant: a(s.assistant),
            system: a(s.system),
            error: a(s.error),
            warning: a(s.warning),
            success: a(s.success),
            tool_name: a(s.tool_name),
            tool_path: a(s.tool_path),
            tool_command: a(s.tool_command),
            tool_count: a(s.tool_count),
            tool_pending: a(s.tool_pending),
            tool_running: a(s.tool_running),
            tool_success: a(s.tool_success),
            tool_error: a(s.tool_error),
            tool_canceled: a(s.tool_canceled),
            todo_pending: a(s.todo_pending),
            todo_in_progress: a(s.todo_in_progress),
            todo_completed: a(s.todo_completed),
            accent: a(s.accent),
            accent_from: a(s.accent_from),
            accent_to: a(s.accent_to),
            logo_field: a(s.logo_field),
            scrollbar: a(s.scrollbar),
            border: a(s.border),
            diff_insert: a(s.diff_insert),
            diff_delete: a(s.diff_delete),
            diff_hunk: a(s.diff_hunk),
            selection: a(s.selection),
            code: a(s.code),
            strong: a(s.strong),
            emphasis: a(s.emphasis),
        };
        let extensions = self
            .extensions
            .iter()
            .map(|(k, v)| (k.clone(), a(*v)))
            .collect();
        Theme {
            styles,
            glyphs: self.glyphs,
            extensions,
            depth: match profile {
                TermProfile::TrueColor => ColorDepth::TrueColor,
                TermProfile::Ansi256 => ColorDepth::Ansi256,
                TermProfile::Ansi16 => ColorDepth::Ansi16,
                TermProfile::NoColor | TermProfile::NoTty => ColorDepth::NoColor,
            },
            hyperlinks: self.hyperlinks,
            look: self.look,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn truecolor_is_identity() {
        let th = Theme::test();
        let t = th.for_profile(TermProfile::TrueColor);
        assert_eq!(t.styles.accent, th.styles.accent);
        assert_eq!(t.styles.muted, th.styles.muted);
    }

    #[test]
    fn ansi256_turns_rgb_into_indexed() {
        let t = Theme::test().for_profile(TermProfile::Ansi256);
        assert!(matches!(t.styles.accent.fg, Some(Color::Indexed(_))));
    }

    #[test]
    fn ansi16_has_no_rgb_or_indexed() {
        let t = Theme::test().for_profile(TermProfile::Ansi16);
        for fg in [t.styles.accent.fg, t.styles.muted.fg, t.styles.border.fg] {
            assert!(fg.is_some());
            assert!(!matches!(
                fg,
                Some(Color::Rgb(..)) | Some(Color::Indexed(_))
            ));
        }
    }

    #[test]
    fn no_color_strips_colors_but_keeps_modifiers() {
        let t = Theme::test().for_profile(TermProfile::NoColor);
        assert_eq!(t.styles.error.fg, None);
        assert!(t.styles.heading.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn profile_sets_the_depth() {
        use super::ColorDepth;
        assert_eq!(
            Theme::test().for_profile(TermProfile::Ansi256).depth,
            ColorDepth::Ansi256
        );
        assert_eq!(
            Theme::test().for_profile(TermProfile::NoTty).depth,
            ColorDepth::NoColor
        );
    }
}
