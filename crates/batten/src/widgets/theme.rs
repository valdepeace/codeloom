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
    pub tool_pending: Style,
    pub tool_running: Style,
    pub tool_success: Style,
    pub tool_error: Style,
    pub tool_canceled: Style,
    pub todo_pending: Style,
    pub todo_in_progress: Style,
    pub todo_completed: Style,
    pub accent: Style,
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
            tool_pending: Style::default().fg(Color::Indexed(244)),
            tool_running: Style::default().fg(Color::Cyan),
            tool_success: Style::default().fg(Color::Green),
            tool_error: Style::default().fg(Color::Red),
            tool_canceled: Style::default().fg(Color::Indexed(240)),
            todo_pending: Style::default().fg(Color::Indexed(240)),
            todo_in_progress: Style::default().fg(Color::Green),
            todo_completed: Style::default().fg(Color::Green),
            accent: Style::default().fg(Color::Rgb(0, 212, 255)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub styles: Styles,
    pub glyphs: Glyphs,
    pub extensions: BTreeMap<String, Style>,
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
    pub fn default_dark() -> Self {
        Self {
            styles: Styles::default(),
            glyphs: Glyphs::default(),
            extensions: BTreeMap::new(),
        }
    }

    /// Tema fijo y determinista para tests.
    pub fn test() -> Self {
        Self::default_dark()
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
            StyleToken::ToolPending => s.tool_pending,
            StyleToken::ToolRunning => s.tool_running,
            StyleToken::ToolSuccess => s.tool_success,
            StyleToken::ToolError => s.tool_error,
            StyleToken::ToolCanceled => s.tool_canceled,
            StyleToken::TodoPending => s.todo_pending,
            StyleToken::TodoInProgress => s.todo_in_progress,
            StyleToken::TodoCompleted => s.todo_completed,
            StyleToken::Accent => s.accent,
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
            "tool_pending" => s.tool_pending = style,
            "tool_running" => s.tool_running = style,
            "tool_success" => s.tool_success = style,
            "tool_error" => s.tool_error = style,
            "tool_canceled" => s.tool_canceled = style,
            "todo_pending" => s.todo_pending = style,
            "todo_in_progress" => s.todo_in_progress = style,
            "todo_completed" => s.todo_completed = style,
            "accent" => s.accent = style,
            _ => return false,
        }
        true
    }
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
        StyleToken::ToolPending => "tool_pending",
        StyleToken::ToolRunning => "tool_running",
        StyleToken::ToolSuccess => "tool_success",
        StyleToken::ToolError => "tool_error",
        StyleToken::ToolCanceled => "tool_canceled",
        StyleToken::TodoPending => "todo_pending",
        StyleToken::TodoInProgress => "todo_in_progress",
        StyleToken::TodoCompleted => "todo_completed",
        StyleToken::Accent => "accent",
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
}
