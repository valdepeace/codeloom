//! Demo del tema: tokens resueltos con el default y con un theme.toml parcial.

use crate::demo::Demo;
use batten::state::StyleToken;
use batten::widgets::Theme;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const TOKENS: &[(&str, StyleToken)] = &[
    ("default", StyleToken::Default),
    ("muted", StyleToken::Muted),
    ("dim", StyleToken::Dim),
    ("heading", StyleToken::Heading),
    ("user", StyleToken::User),
    ("assistant", StyleToken::Assistant),
    ("system", StyleToken::System),
    ("error", StyleToken::Error),
    ("warning", StyleToken::Warning),
    ("success", StyleToken::Success),
    ("tool_name", StyleToken::ToolName),
    ("tool_running", StyleToken::ToolRunning),
    ("tool_success", StyleToken::ToolSuccess),
    ("tool_error", StyleToken::ToolError),
    ("accent", StyleToken::Accent),
    ("accent_from", StyleToken::AccentFrom),
    ("accent_to", StyleToken::AccentTo),
    ("logo_field", StyleToken::LogoField),
];

pub struct ThemeDemo {
    default: Theme,
    partial: Theme,
    alt: bool,
}

impl ThemeDemo {
    pub fn new() -> Self {
        let partial = Theme::from_toml(
            "[styles]\naccent_from = \"magenta\"\naccent_to = \"yellow\"\nerror = \"indexed:196\"\n",
        )
        .unwrap_or_else(|_| Theme::default_dark());
        Self {
            default: Theme::default_dark(),
            partial,
            alt: false,
        }
    }
}

impl Demo for ThemeDemo {
    fn title(&self) -> &str {
        "theme"
    }

    fn hint(&self) -> &str {
        "t: alternar default / theme.toml parcial"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, _theme: &Theme) {
        let title = if self.alt {
            "tema: theme.toml parcial"
        } else {
            "tema: default_dark"
        };
        let th = if self.alt {
            &self.partial
        } else {
            &self.default
        };
        let mut lines = vec![Line::from(title.to_string())];
        for (name, token) in TOKENS {
            lines.push(Line::from(vec![
                Span::styled("Aa ", th.style(*token)),
                Span::styled((*name).to_string(), th.style(*token)),
            ]));
        }
        f.render_widget(Paragraph::new(lines), area);
    }

    fn handle_key(&mut self, code: KeyCode) -> bool {
        if code == KeyCode::Char('t') {
            self.alt = !self.alt;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::KeyCode;

    #[test]
    fn t_toggles_between_default_and_partial() {
        let mut d = ThemeDemo::new();
        let before = d.alt;
        d.handle_key(KeyCode::Char('t'));
        assert_ne!(d.alt, before);
    }
}
