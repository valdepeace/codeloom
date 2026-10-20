//! Welcome wordmark de Martty desde `website/public/ui_start.png`.

use crate::demo::Demo;
use batten::widgets::{logo, Theme, ThemePreset};
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

pub struct MarttyWelcomeDemo {
    whale: Vec<Color>,
    whale_width: u32,
    whale_height: u32,
}

impl MarttyWelcomeDemo {
    pub fn new() -> Self {
        let image = image::load_from_memory(include_bytes!("../assets/martty-whale.png"))
            .expect("the checked-in Martty whale fixture is valid PNG")
            .to_rgb8();
        let (whale_width, whale_height) = image.dimensions();
        let whale = image
            .pixels()
            .map(|pixel| Color::Rgb(pixel[0], pixel[1], pixel[2]))
            .collect();
        Self {
            whale,
            whale_width,
            whale_height,
        }
    }
}

impl Demo for MarttyWelcomeDemo {
    fn title(&self) -> &str {
        "Martty · welcome"
    }

    fn hint(&self) -> &str {
        "ballena pixelada · wordmark · entorno · modelo · atajos"
    }

    fn fullscreen(&self) -> bool {
        true
    }

    fn render(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        let theme = Theme::preset(ThemePreset::Martty);
        let whale_height = (area.height / 4).clamp(5, 14);
        let whale_width = ((u64::from(whale_height) * 2 * u64::from(self.whale_width)
            + u64::from(self.whale_height) / 2)
            / u64::from(self.whale_height.max(1)))
        .min(u64::from(area.width.saturating_sub(6))) as u16;
        let whale_x = area.x + area.width.saturating_sub(whale_width) / 2;
        logo::render_martty_whale(
            frame,
            Rect::new(whale_x, area.y + 1, whale_width, whale_height),
            &self.whale,
            self.whale_width,
            self.whale_height,
        );
        let wordmark_y = area.y + whale_height + 2;
        frame.render_widget(
            Paragraph::new(logo::martty_wordmark(&theme, area.width))
                .alignment(ratatui::layout::Alignment::Center),
            Rect::new(area.x, wordmark_y, area.width, 7),
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "https://martty.sh",
                Style::default().fg(Color::Rgb(151, 157, 166)),
            )))
            .alignment(ratatui::layout::Alignment::Center),
            Rect::new(area.x, wordmark_y + 7, area.width, 1),
        );
        let info_y = (area.y + (area.height * 52 / 100)).max(wordmark_y + 10);
        let info = [
            ("version", "dsh-tui 0.2.20"),
            ("model", "deepseek-official · deepseek-v4-flash"),
            ("workspace", "~/project/Martty"),
            ("session", "8af91bd8-23f3-46ed-8d21-ca0465a1c458"),
            ("runtime", "dsh-acp"),
            ("credentials", "ACP authenticate · DeepSeek API key"),
            (
                "access",
                "full workspace access — prefer a disposable checkout",
            ),
        ];
        let lines = info
            .iter()
            .map(|(label, value)| {
                Line::from(vec![
                    Span::styled(
                        format!("{label:<12}"),
                        Style::default().fg(Color::Rgb(133, 139, 146)),
                    ),
                    Span::styled(*value, Style::default().fg(Color::Rgb(201, 204, 210))),
                ])
            })
            .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(lines),
            Rect::new(area.x + 3, info_y, area.width.saturating_sub(6), 8),
        );
        frame.render_widget(
            Paragraph::new(Line::from(
                "/help commands · /keys shortcuts · ! shell · esc interrupt",
            ))
            .style(Style::default().fg(Color::Rgb(151, 157, 166))),
            Rect::new(
                area.x + 3,
                area.bottom().saturating_sub(2),
                area.width.saturating_sub(6),
                1,
            ),
        );
    }

    fn handle_key(&mut self, _code: KeyCode) -> bool {
        false
    }
}
