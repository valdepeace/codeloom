//! Tema: tokens semánticos, glifos y carga desde `theme.toml` parcial.

use crate::state::StyleToken;
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;
use std::collections::BTreeMap;

/// Glifos visuales que cambian con el tema (iconos de tool, todo, campo del logo).
#[derive(Debug, Clone, Copy)]
pub struct Glyphs {
    pub tool_success: char,
    pub tool_error: char,
    pub tool_pending: char,
    pub todo_pending: char,
    pub todo_in_progress: char,
    pub todo_completed: char,
    pub field: char,
}

impl Default for Glyphs {
    fn default() -> Self {
        Self {
            tool_success: '\u{2713}',     // ✓
            tool_error: '×',              // ×
            tool_pending: '\u{25cf}',     // ●
            todo_pending: '\u{2022}',     // •
            todo_in_progress: '\u{2192}', // →
            todo_completed: '\u{2713}',   // ✓
            field: '\u{2571}',            // ╱
        }
    }
}

/// Tokens concretos para cada aspecto visual. Un campo por token semántico +
/// mapa extensible.
#[derive(Debug, Clone)]
pub struct Styles {
    pub default: Style,
    pub muted: Style,
    pub dim: Style,
    pub heading: Style,
    pub user: Style,
    pub assistant: Style,
    pub system: Style,
    pub error: Style,
    pub warning: Style,
    pub success: Style,
    pub tool_name: Style,
    pub tool_path: Style,
    pub tool_command: Style,
    pub tool_count: Style,
    pub tool_pending: Style,
    pub tool_running: Style,
    pub tool_success: Style,
    pub tool_error: Style,
    pub tool_canceled: Style,
    pub todo_pending: Style,
    pub todo_in_progress: Style,
    pub todo_completed: Style,
    pub accent: Style,
    pub accent_from: Style,
    pub accent_to: Style,
    pub logo_field: Style,
    pub scrollbar: Style,
    pub border: Style,
    pub diff_insert: Style,
    pub diff_delete: Style,
    pub diff_hunk: Style,
    pub selection: Style,
    pub code: Style,
    pub strong: Style,
    pub emphasis: Style,
}

impl Default for Styles {
    fn default() -> Self {
        Self {
            default: Style::default(),
            muted: Style::default().fg(Color::Indexed(244)),
            dim: Style::default().fg(Color::Indexed(240)),
            heading: Style::default()
                .fg(Color::Indexed(250))
                .add_modifier(Modifier::BOLD),
            user: Style::default().fg(Color::Cyan),
            assistant: Style::default().fg(Color::Indexed(252)),
            system: Style::default().fg(Color::Indexed(245)),
            error: Style::default().fg(Color::Red),
            warning: Style::default().fg(Color::Yellow),
            success: Style::default().fg(Color::Green),
            tool_name: Style::default()
                .fg(Color::Indexed(252))
                .add_modifier(Modifier::BOLD),
            // Derivados: ruta ← accent, comando ← default, recuento ← dim.
            tool_path: Style::default().fg(Color::Rgb(0, 212, 255)),
            tool_command: Style::default(),
            tool_count: Style::default().fg(Color::Indexed(240)),
            tool_pending: Style::default().fg(Color::Indexed(244)),
            tool_running: Style::default().fg(Color::Cyan),
            tool_success: Style::default().fg(Color::Green),
            tool_error: Style::default().fg(Color::Red),
            tool_canceled: Style::default().fg(Color::Indexed(240)),
            todo_pending: Style::default().fg(Color::Indexed(240)),
            todo_in_progress: Style::default().fg(Color::Green),
            todo_completed: Style::default().fg(Color::Green),
            accent: Style::default().fg(Color::Rgb(0, 212, 255)),
            accent_from: Style::default().fg(Color::Rgb(0, 212, 255)),
            accent_to: Style::default().fg(Color::Rgb(124, 92, 255)),
            logo_field: Style::default().fg(Color::Indexed(238)),
            scrollbar: Style::default().fg(Color::Indexed(240)),
            border: Style::default().fg(Color::Indexed(240)),
            diff_insert: Style::default().fg(Color::Green),
            diff_delete: Style::default().fg(Color::Red),
            diff_hunk: Style::default().fg(Color::Cyan),
            selection: Style::default().bg(Color::Indexed(238)),
            code: Style::default().fg(Color::Indexed(180)),
            strong: Style::default().add_modifier(Modifier::BOLD),
            emphasis: Style::default().add_modifier(Modifier::ITALIC),
        }
    }
}

/// Colores que admite el terminal. Lo fija `Theme::for_profile`; los
/// widgets lo usan para los colores que generan al pintar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorDepth {
    #[default]
    TrueColor,
    Ansi256,
    Ansi16,
    NoColor,
}

/// Variante de presentación del composer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerStyle {
    /// Caja redondeada con título y pie (comportamiento histórico).
    Classic,
    /// Sin marco, prompt `> ` y `::: ` por línea, con wrap y scroll vertical.
    Crush,
}

/// Dónde se presentan las llamadas a herramientas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStyle {
    /// Tarjeta fija junto al composer (solo la última).
    Pinned,
    /// Entradas dentro del transcript, en orden cronológico.
    Inline,
}

/// Disposición de la pantalla: áreas laterales y cabecera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutStyle {
    /// Sidebar izquierdo y transcript a la derecha (disposición histórica).
    Workspace,
    /// Transcript a la izquierda y un rail de contexto a la derecha.
    ContextRail,
}

/// Marco visual de los paneles y de la barra de estado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeStyle {
    /// Paneles con borde y cabecera de dos filas (comportamiento histórico).
    Panels,
    /// Sin cajas: cabecera de una línea y separadores ligeros.
    Flat,
}

/// Presentación de cada elemento; un preset es paleta más `Look`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub composer: ComposerStyle,
    pub tool: ToolStyle,
    pub layout: LayoutStyle,
    pub chrome: ChromeStyle,
}

impl Look {
    pub const fn classic() -> Self {
        Self {
            composer: ComposerStyle::Classic,
            tool: ToolStyle::Pinned,
            layout: LayoutStyle::Workspace,
            chrome: ChromeStyle::Panels,
        }
    }

    pub const fn crush() -> Self {
        Self {
            composer: ComposerStyle::Crush,
            tool: ToolStyle::Inline,
            layout: LayoutStyle::ContextRail,
            chrome: ChromeStyle::Flat,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub styles: Styles,
    pub glyphs: Glyphs,
    pub extensions: BTreeMap<String, Style>,
    pub depth: ColorDepth,
    /// Pintar los enlaces como hipervínculos OSC 8.
    pub hyperlinks: bool,
    /// Presentación del composer y de las herramientas.
    pub look: Look,
}

/// Paletas de referencia disponibles en Batten y su showcase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    Batten,
    Martty,
    Mindfork,
    Pawan,
    Crush,
    MastraCode,
}

impl ThemePreset {
    pub const ALL: [Self; 6] = [
        Self::Batten,
        Self::Martty,
        Self::Mindfork,
        Self::Pawan,
        Self::Crush,
        Self::MastraCode,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Batten => "Batten",
            Self::Martty => "Martty",
            Self::Mindfork => "Mindfork",
            Self::Pawan => "Pawan",
            Self::Crush => "Crush · Charmtone",
            Self::MastraCode => "MastraCode",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error("invalid theme: {0}")]
    Parse(String),
    #[error("invalid color: {0}")]
    Color(String),
}

/// Forma deserializable del `theme.toml`: un mapa plano token → valor de
/// color.
#[derive(Debug, Deserialize)]
struct ThemeFile {
    #[serde(default)]
    styles: BTreeMap<String, String>,
}

fn parse_color(s: &str) -> Result<Color, ThemeError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let parse =
            |i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ThemeError::Color(s.into()));
        return Ok(Color::Rgb(parse(0)?, parse(2)?, parse(4)?));
    }
    if let Some(idx) = s.strip_prefix("indexed:") {
        return idx
            .parse::<u8>()
            .map(Color::Indexed)
            .map_err(|_| ThemeError::Color(s.into()));
    }
    let c = match s.to_ascii_lowercase().as_str() {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "white" => Color::White,
        _ => return Err(ThemeError::Color(s.into())),
    };
    Ok(c)
}

impl Theme {
    /// Paleta de terminal inspirada en un proyecto de referencia.
    /// Los componentes y el layout siguen siendo los de Batten.
    pub fn preset(preset: ThemePreset) -> Self {
        use ThemePreset::*;

        let mut theme = Self::default_dark();
        let (bg, fg, muted, accent, user, assistant, warning, error, success, border) = match preset
        {
            Batten => return theme,
            // Martty's DEFAULT_DARK DeepSeek token map.
            Martty => (
                Color::Rgb(15, 17, 21),
                Color::Rgb(237, 243, 254),
                Color::Rgb(151, 157, 166),
                Color::Rgb(86, 134, 254),
                Color::Rgb(103, 158, 254),
                Color::Rgb(78, 209, 126),
                Color::Rgb(247, 173, 49),
                Color::Rgb(242, 90, 90),
                Color::Rgb(78, 209, 126),
                Color::Rgb(44, 44, 46),
            ),
            // Mindfork's dark palette roles over CANVAS_DARK.
            Mindfork => (
                Color::Rgb(15, 17, 21),
                Color::Rgb(201, 204, 210),
                Color::Rgb(133, 139, 146),
                Color::Rgb(220, 175, 97),
                Color::Rgb(121, 169, 219),
                Color::Rgb(111, 192, 130),
                Color::Rgb(220, 175, 97),
                Color::Rgb(223, 105, 92),
                Color::Rgb(111, 192, 130),
                Color::Rgb(54, 58, 66),
            ),
            // Pawan's built-in DEFAULT Ratatui palette.
            Pawan => (
                Color::Rgb(0, 0, 0),
                Color::Rgb(255, 255, 255),
                Color::Rgb(136, 136, 136),
                Color::Rgb(0, 204, 204),
                Color::Rgb(0, 204, 204),
                Color::Rgb(230, 230, 230),
                Color::Rgb(255, 184, 108),
                Color::Rgb(255, 85, 85),
                Color::Rgb(80, 250, 123),
                Color::Rgb(68, 68, 68),
            ),
            // Crush's Charmtone Pantera tokens from charmtoneOpts.
            Crush => (
                Color::Rgb(32, 31, 38),
                Color::Rgb(241, 239, 239),
                Color::Rgb(191, 188, 200),
                Color::Rgb(107, 80, 255),
                Color::Rgb(107, 80, 255),
                Color::Rgb(241, 239, 239),
                Color::Rgb(245, 239, 52),
                Color::Rgb(235, 66, 104),
                Color::Rgb(0, 255, 178),
                Color::Rgb(45, 44, 53),
            ),
            // MastraCode's darkSurface and darkTheme source-map values.
            MastraCode => (
                Color::Rgb(2, 2, 2),
                Color::Rgb(250, 250, 250),
                Color::Rgb(140, 140, 148),
                Color::Rgb(98, 246, 157),
                Color::Rgb(250, 250, 250),
                Color::Rgb(217, 217, 220),
                Color::Rgb(245, 158, 11),
                Color::Rgb(239, 68, 68),
                Color::Rgb(98, 246, 157),
                Color::Rgb(63, 63, 70),
            ),
        };

        theme.styles.default = Style::default().fg(fg).bg(bg);
        theme.styles.muted = Style::default().fg(muted);
        theme.styles.dim = Style::default().fg(muted);
        theme.styles.heading = Style::default().fg(fg).add_modifier(Modifier::BOLD);
        theme.styles.user = Style::default().fg(user);
        theme.styles.assistant = Style::default().fg(assistant);
        theme.styles.system = Style::default().fg(muted);
        theme.styles.error = Style::default().fg(error);
        theme.styles.warning = Style::default().fg(warning);
        theme.styles.success = Style::default().fg(success);
        theme.styles.tool_name = Style::default().fg(fg).add_modifier(Modifier::BOLD);
        theme.styles.tool_pending = Style::default().fg(muted);
        theme.styles.tool_running = Style::default().fg(accent);
        theme.styles.tool_success = Style::default().fg(success);
        theme.styles.tool_error = Style::default().fg(error);
        theme.styles.tool_canceled = Style::default().fg(muted);
        theme.styles.todo_pending = Style::default().fg(muted);
        theme.styles.todo_in_progress = Style::default().fg(accent);
        theme.styles.todo_completed = Style::default().fg(success);
        theme.styles.accent = Style::default().fg(accent);
        theme.styles.accent_from = Style::default().fg(accent);
        theme.styles.accent_to = Style::default().fg(user);
        theme.styles.border = Style::default().fg(border);
        theme.styles.scrollbar = Style::default().fg(border);
        theme.styles.logo_field = Style::default().fg(muted);
        theme.styles.selection = Style::default().bg(border);
        theme.styles.diff_insert = Style::default().fg(success);
        theme.styles.diff_delete = Style::default().fg(error);
        theme.styles.diff_hunk = Style::default().fg(accent);
        theme.styles.code = Style::default().fg(accent);
        // Tokens de pieza: por defecto derivados de los existentes del preset.
        // Deben ir después de `accent`, `default` y `dim`, que se fijan arriba.
        theme.styles.tool_path = theme.styles.accent;
        theme.styles.tool_command = theme.styles.default;
        theme.styles.tool_count = theme.styles.dim;
        theme.glyphs.tool_pending = '●';
        if preset == Crush {
            theme.look = Look::crush();
            // Crush rompe el derivado de la ruta con su verde menta (`success`,
            // 0,255,178), que no compite con el violeta de `accent`. El recuento
            // conserva el `dim` derivado y el comando, el `default`.
            theme.styles.tool_path = theme.styles.success;
        }
        theme
    }

    pub fn default_dark() -> Self {
        Self {
            styles: Styles::default(),
            glyphs: Glyphs::default(),
            extensions: BTreeMap::new(),
            depth: ColorDepth::TrueColor,
            hyperlinks: true,
            look: Look::classic(),
        }
    }

    /// Tema fijo y determinista para tests.
    pub fn test() -> Self {
        Self::default_dark()
    }

    /// Adapta un color generado al pintar a la profundidad del tema.
    /// `None` = sin color (usar el del terminal).
    pub fn adapt(&self, color: Color) -> Option<Color> {
        match self.depth {
            ColorDepth::TrueColor => Some(color),
            ColorDepth::NoColor => None,
            ColorDepth::Ansi256 | ColorDepth::Ansi16 => adapt_lower(self.depth, color),
        }
    }

    /// Resuelve un token semántico a su estilo. Si el nombre no coincide con
    /// un campo de `Styles`, devuelve el estilo de extensión correspondiente
    /// o un estilo por defecto.
    pub fn style(&self, token: StyleToken) -> Style {
        self.set_named_style(token).unwrap_or_else(|| {
            let key = String::from(token_to_key(token));
            self.extensions.get(&key).copied().unwrap_or_default()
        })
    }

    /// Construye un tema sobrescribiendo campos nombrados en `[styles]`.
    /// Los nombres no reconocidos se almacenan como extensiones.
    pub fn from_toml(src: &str) -> Result<Self, ThemeError> {
        let file: ThemeFile = toml::from_str(src).map_err(|e| ThemeError::Parse(e.to_string()))?;
        let mut theme = Self::default_dark();
        for (name, value) in file.styles {
            let color = parse_color(&value)?;
            let style = Style::default().fg(color);
            if !theme.apply_override(&name, style) {
                theme.extensions.insert(name.clone(), style);
            }
        }
        Ok(theme)
    }

    fn set_named_style(&self, token: StyleToken) -> Option<Style> {
        let s = &self.styles;
        Some(match token {
            StyleToken::Default => s.default,
            StyleToken::Muted => s.muted,
            StyleToken::Dim => s.dim,
            StyleToken::Heading => s.heading,
            StyleToken::User => s.user,
            StyleToken::Assistant => s.assistant,
            StyleToken::System => s.system,
            StyleToken::Error => s.error,
            StyleToken::Warning => s.warning,
            StyleToken::Success => s.success,
            StyleToken::ToolName => s.tool_name,
            StyleToken::ToolPath => s.tool_path,
            StyleToken::ToolCommand => s.tool_command,
            StyleToken::ToolCount => s.tool_count,
            StyleToken::ToolPending => s.tool_pending,
            StyleToken::ToolRunning => s.tool_running,
            StyleToken::ToolSuccess => s.tool_success,
            StyleToken::ToolError => s.tool_error,
            StyleToken::ToolCanceled => s.tool_canceled,
            StyleToken::TodoPending => s.todo_pending,
            StyleToken::TodoInProgress => s.todo_in_progress,
            StyleToken::TodoCompleted => s.todo_completed,
            StyleToken::Accent => s.accent,
            StyleToken::AccentFrom => s.accent_from,
            StyleToken::AccentTo => s.accent_to,
            StyleToken::LogoField => s.logo_field,
            StyleToken::Scrollbar => s.scrollbar,
            StyleToken::Border => s.border,
            StyleToken::DiffInsert => s.diff_insert,
            StyleToken::DiffDelete => s.diff_delete,
            StyleToken::DiffHunk => s.diff_hunk,
            StyleToken::Selection => s.selection,
            StyleToken::Code => s.code,
            StyleToken::Strong => s.strong,
            StyleToken::Emphasis => s.emphasis,
        })
    }

    fn apply_override(&mut self, name: &str, style: Style) -> bool {
        let s = &mut self.styles;
        match name {
            "default" => s.default = style,
            "muted" => s.muted = style,
            "dim" => s.dim = style,
            "heading" => s.heading = style,
            "user" => s.user = style,
            "assistant" => s.assistant = style,
            "system" => s.system = style,
            "error" => s.error = style,
            "warning" => s.warning = style,
            "success" => s.success = style,
            "tool_name" => s.tool_name = style,
            "tool_path" => s.tool_path = style,
            "tool_command" => s.tool_command = style,
            "tool_count" => s.tool_count = style,
            "tool_pending" => s.tool_pending = style,
            "tool_running" => s.tool_running = style,
            "tool_success" => s.tool_success = style,
            "tool_error" => s.tool_error = style,
            "tool_canceled" => s.tool_canceled = style,
            "todo_pending" => s.todo_pending = style,
            "todo_in_progress" => s.todo_in_progress = style,
            "todo_completed" => s.todo_completed = style,
            "accent" => s.accent = style,
            "accent_from" => s.accent_from = style,
            "accent_to" => s.accent_to = style,
            "logo_field" => s.logo_field = style,
            "scrollbar" => s.scrollbar = style,
            "border" => s.border = style,
            "diff_insert" => s.diff_insert = style,
            "diff_delete" => s.diff_delete = style,
            "diff_hunk" => s.diff_hunk = style,
            "selection" => s.selection = Style::default().bg(style.fg.unwrap_or(Color::Reset)),
            "code" => s.code = style,
            "strong" => s.strong = style,
            "emphasis" => s.emphasis = style,
            _ => return false,
        }
        true
    }
}

#[cfg(feature = "termprofile")]
fn adapt_lower(depth: ColorDepth, color: Color) -> Option<Color> {
    let profile = match depth {
        ColorDepth::Ansi256 => termprofile::TermProfile::Ansi256,
        _ => termprofile::TermProfile::Ansi16,
    };
    profile.adapt_color(color)
}

/// Sin `termprofile` no hay tablas de conversión: se deja el color.
#[cfg(not(feature = "termprofile"))]
fn adapt_lower(_depth: ColorDepth, color: Color) -> Option<Color> {
    Some(color)
}

fn token_to_key(t: StyleToken) -> &'static str {
    match t {
        StyleToken::Default => "default",
        StyleToken::Muted => "muted",
        StyleToken::Dim => "dim",
        StyleToken::Heading => "heading",
        StyleToken::User => "user",
        StyleToken::Assistant => "assistant",
        StyleToken::System => "system",
        StyleToken::Error => "error",
        StyleToken::Warning => "warning",
        StyleToken::Success => "success",
        StyleToken::ToolName => "tool_name",
        StyleToken::ToolPath => "tool_path",
        StyleToken::ToolCommand => "tool_command",
        StyleToken::ToolCount => "tool_count",
        StyleToken::ToolPending => "tool_pending",
        StyleToken::ToolRunning => "tool_running",
        StyleToken::ToolSuccess => "tool_success",
        StyleToken::ToolError => "tool_error",
        StyleToken::ToolCanceled => "tool_canceled",
        StyleToken::TodoPending => "todo_pending",
        StyleToken::TodoInProgress => "todo_in_progress",
        StyleToken::TodoCompleted => "todo_completed",
        StyleToken::Accent => "accent",
        StyleToken::AccentFrom => "accent_from",
        StyleToken::AccentTo => "accent_to",
        StyleToken::LogoField => "logo_field",
        StyleToken::Scrollbar => "scrollbar",
        StyleToken::Border => "border",
        StyleToken::DiffInsert => "diff_insert",
        StyleToken::DiffDelete => "diff_delete",
        StyleToken::DiffHunk => "diff_hunk",
        StyleToken::Selection => "selection",
        StyleToken::Code => "code",
        StyleToken::Strong => "strong",
        StyleToken::Emphasis => "emphasis",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StyleToken;
    use ratatui::style::Color;

    #[test]
    fn test_theme_resolves_tokens() {
        let th = Theme::test();
        assert_eq!(th.style(StyleToken::Error).fg, Some(Color::Red));
        assert_eq!(th.style(StyleToken::Success).fg, Some(Color::Green));
    }

    #[test]
    fn glyphs_have_defaults() {
        let g = Glyphs::default();
        assert_eq!(g.tool_success, '✓');
        assert_eq!(g.tool_error, '×');
        assert_eq!(g.todo_completed, '✓');
    }

    #[test]
    fn partial_toml_overrides_only_named_tokens() {
        let th = Theme::from_toml(
            r#"
            [styles]
            error = "indexed:196"
            "#,
        )
        .unwrap();
        assert_eq!(th.style(StyleToken::Error).fg, Some(Color::Indexed(196)));
        // Un token no nombrado conserva el default.
        assert_eq!(
            th.style(StyleToken::Success).fg,
            Theme::default_dark().style(StyleToken::Success).fg
        );
    }

    #[test]
    fn hex_and_named_colors_parse() {
        let th = Theme::from_toml(
            r#"
            [styles]
            accent = '#00d4ff'
            user = 'cyan'
            "#,
        )
        .unwrap();
        assert_eq!(
            th.style(StyleToken::Accent).fg,
            Some(Color::Rgb(0, 212, 255))
        );
        assert_eq!(th.style(StyleToken::User).fg, Some(Color::Cyan));
    }

    #[test]
    fn unknown_token_goes_to_extensions() {
        let th = Theme::from_toml(
            r#"
            [styles]
            my_custom = "magenta"
            "#,
        )
        .unwrap();
        assert_eq!(
            th.extensions.get("my_custom").unwrap().fg,
            Some(Color::Magenta)
        );
    }

    #[test]
    fn invalid_color_is_an_error() {
        assert!(Theme::from_toml("[styles]\nerror = \"not-a-color\"\n").is_err());
    }

    #[test]
    fn accent_gradient_tokens_resolve() {
        let th = Theme::test();
        assert_eq!(
            th.style(StyleToken::AccentFrom).fg,
            Some(Color::Rgb(0, 212, 255))
        );
        assert_eq!(
            th.style(StyleToken::AccentTo).fg,
            Some(Color::Rgb(124, 92, 255))
        );
        assert_eq!(
            th.style(StyleToken::LogoField).fg,
            Some(Color::Indexed(238))
        );
    }

    #[test]
    fn accent_tokens_can_be_overridden() {
        let th = Theme::from_toml("[styles]\naccent_from = \"magenta\"\n").unwrap();
        assert_eq!(th.style(StyleToken::AccentFrom).fg, Some(Color::Magenta));
    }

    #[test]
    fn scrollbar_token_resolves_and_overrides() {
        let th = Theme::test();
        assert_eq!(
            th.style(StyleToken::Scrollbar).fg,
            Some(Color::Indexed(240))
        );
        let th = Theme::from_toml("[styles]\nscrollbar = \"magenta\"\n").unwrap();
        assert_eq!(th.style(StyleToken::Scrollbar).fg, Some(Color::Magenta));
    }

    #[test]
    fn border_token_resolves_and_overrides() {
        let th = Theme::test();
        assert_eq!(th.style(StyleToken::Border).fg, Some(Color::Indexed(240)));
        let th = Theme::from_toml("[styles]\nborder = \"magenta\"\n").unwrap();
        assert_eq!(th.style(StyleToken::Border).fg, Some(Color::Magenta));
    }

    #[test]
    fn diff_tokens_resolve_and_override() {
        let th = Theme::test();
        assert_eq!(th.style(StyleToken::DiffInsert).fg, Some(Color::Green));
        assert_eq!(th.style(StyleToken::DiffHunk).fg, Some(Color::Cyan));
        let th = Theme::from_toml("[styles]\ndiff_delete = \"magenta\"\n").unwrap();
        assert_eq!(th.style(StyleToken::DiffDelete).fg, Some(Color::Magenta));
    }

    #[test]
    fn selection_token_is_a_background() {
        let th = Theme::test();
        assert_eq!(
            th.style(StyleToken::Selection).bg,
            Some(Color::Indexed(238))
        );
        let th = Theme::from_toml("[styles]\nselection = \"blue\"\n").unwrap();
        assert_eq!(th.style(StyleToken::Selection).bg, Some(Color::Blue));
    }

    #[test]
    fn markdown_tokens_resolve() {
        use ratatui::style::Modifier;
        let th = Theme::test();
        assert_eq!(th.style(StyleToken::Code).fg, Some(Color::Indexed(180)));
        assert!(th
            .style(StyleToken::Strong)
            .add_modifier
            .contains(Modifier::BOLD));
        assert!(th
            .style(StyleToken::Emphasis)
            .add_modifier
            .contains(Modifier::ITALIC));
    }

    #[test]
    fn adapt_follows_the_depth() {
        let mut th = Theme::test();
        let rgb = Color::Rgb(0, 212, 255);
        assert_eq!(th.adapt(rgb), Some(rgb));
        th.depth = ColorDepth::NoColor;
        assert_eq!(th.adapt(rgb), None);
    }

    #[cfg(feature = "termprofile")]
    #[test]
    fn adapt_lowers_rgb_with_termprofile() {
        let mut th = Theme::test();
        th.depth = ColorDepth::Ansi256;
        assert!(matches!(
            th.adapt(Color::Rgb(0, 212, 255)),
            Some(Color::Indexed(_))
        ));
        th.depth = ColorDepth::Ansi16;
        let c = th.adapt(Color::Rgb(0, 212, 255));
        assert!(c.is_some() && !matches!(c, Some(Color::Rgb(..)) | Some(Color::Indexed(_))));
    }

    #[test]
    fn only_the_crush_preset_has_its_own_look() {
        for preset in ThemePreset::ALL {
            let theme = Theme::preset(preset);
            let expected = if preset == ThemePreset::Crush {
                Look::crush()
            } else {
                Look::classic()
            };
            assert_eq!(theme.look, expected, "{}", preset.name());
        }
        assert_eq!(Theme::default_dark().look, Look::classic());
        assert_eq!(Theme::test().look, Look::classic());
        assert_eq!(Look::crush().composer, ComposerStyle::Crush);
        assert_eq!(Look::crush().tool, ToolStyle::Inline);
        assert_eq!(Look::classic().composer, ComposerStyle::Classic);
        assert_eq!(Look::classic().tool, ToolStyle::Pinned);
        assert_eq!(Look::classic().layout, LayoutStyle::Workspace);
        assert_eq!(Look::classic().chrome, ChromeStyle::Panels);
        assert_eq!(Look::crush().layout, LayoutStyle::ContextRail);
        assert_eq!(Look::crush().chrome, ChromeStyle::Flat);
    }

    #[test]
    fn new_tokens_resolve_in_every_preset() {
        for preset in ThemePreset::ALL {
            let theme = Theme::preset(preset);
            let name = preset.name();
            assert!(
                theme.style(StyleToken::ToolPath).fg.is_some(),
                "{name}: ToolPath sin color"
            );
            assert!(
                theme.style(StyleToken::ToolCount).fg.is_some(),
                "{name}: ToolCount sin color"
            );
            assert_eq!(
                theme.style(StyleToken::ToolCommand),
                theme.style(StyleToken::Default),
                "{name}: ToolCommand debe resolver al estilo por defecto"
            );
        }
    }

    #[test]
    fn defaults_are_derived_from_existing_styles() {
        // Martty no define valores propios de pieza: hereda los derivados.
        let theme = Theme::preset(ThemePreset::Martty);
        assert_eq!(
            theme.style(StyleToken::ToolPath),
            theme.style(StyleToken::Accent)
        );
        assert_eq!(
            theme.style(StyleToken::ToolCommand),
            theme.style(StyleToken::Default)
        );
        assert_eq!(
            theme.style(StyleToken::ToolCount),
            theme.style(StyleToken::Dim)
        );

        // Y el tema por defecto, sin preset, sigue la misma regla.
        let theme = Theme::test();
        assert_eq!(
            theme.style(StyleToken::ToolPath),
            theme.style(StyleToken::Accent)
        );
        assert_eq!(
            theme.style(StyleToken::ToolCount),
            theme.style(StyleToken::Dim)
        );
    }

    #[test]
    fn crush_overrides_tool_path_or_count() {
        let crush = Theme::preset(ThemePreset::Crush);
        let differs = crush.style(StyleToken::ToolPath) != crush.style(StyleToken::Accent)
            || crush.style(StyleToken::ToolCount) != crush.style(StyleToken::Dim);
        assert!(differs, "Crush debe definir su propia ruta o recuento");
    }

    #[test]
    fn tool_piece_tokens_can_be_overridden() {
        let th = Theme::from_toml("[styles]\ntool_path = \"magenta\"\ntool_count = \"green\"\n")
            .unwrap();
        assert_eq!(th.style(StyleToken::ToolPath).fg, Some(Color::Magenta));
        assert_eq!(th.style(StyleToken::ToolCount).fg, Some(Color::Green));
        assert!(th.extensions.is_empty());
    }
}
