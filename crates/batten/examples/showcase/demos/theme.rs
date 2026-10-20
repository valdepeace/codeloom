//! Muestrario de las paletas disponibles en el showcase.

use crate::demo::Demo;
use batten::state::StyleToken;
use batten::widgets::{Theme, ThemePreset};
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct ThemeDemo {
    palettes: Vec<(ThemePreset, Theme)>,
}

impl ThemeDemo {
    pub fn new() -> Self {
        Self {
            palettes: ThemePreset::ALL
                .into_iter()
                .map(|preset| (preset, Theme::preset(preset)))
                .collect(),
        }
    }
}

impl Demo for ThemeDemo {
    fn title(&self) -> &str {
        "themes"
    }

    fn hint(&self) -> &str {
        "F7: siguiente paleta · Shift+F7: anterior · las demos usan la paleta activa"
    }

    fn render(&mut self, f: &mut Frame, area: Rect, active: &Theme) {
        let mut lines = vec![Line::from("Paletas base de Batten y referencias visuales:")];
        for (preset, theme) in &self.palettes {
            let selected = theme.styles.accent == active.styles.accent
                && theme.styles.default.bg == active.styles.default.bg;
            let marker = if selected { "› " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(marker, theme.style(StyleToken::Muted)),
                Span::styled("██ ", theme.styles.default),
                Span::styled(
                    format!("{:<17}", preset.name()),
                    theme.style(StyleToken::Accent),
                ),
                Span::styled(" user ", theme.style(StyleToken::User)),
                Span::styled(" assistant ", theme.style(StyleToken::Assistant)),
                Span::styled(" tool ", theme.style(StyleToken::ToolRunning)),
                Span::styled("✓", theme.style(StyleToken::Success)),
                Span::raw(" / "),
                Span::styled("×", theme.style(StyleToken::Error)),
            ]));
        }
        lines.push(Line::from(
            "F7 cambia la paleta global; luego abre chat o tool_item para comparar.",
        ));
        f.render_widget(Paragraph::new(lines), area);
    }
}
