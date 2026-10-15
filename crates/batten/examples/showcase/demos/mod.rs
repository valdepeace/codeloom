//! Catálogo de demos del showcase.

mod chat;
mod chip;
mod composer;
mod file_changes;
mod key_help;
mod logo;
mod panel;
mod progress;
mod rule;
mod sidebar;
mod spinner;
mod spinners;
mod status;
mod theme;
mod toast;
mod tool_item;
mod transcript;

use crate::demo::Demo;

/// Todos los demos, en el orden del índice.
pub fn catalog() -> Vec<Box<dyn Demo>> {
    vec![
        Box::new(transcript::TranscriptDemo::new()),
        Box::new(tool_item::ToolItemDemo::new()),
        Box::new(composer::ComposerDemo::new()),
        Box::new(spinner::SpinnerDemo::new()),
        Box::new(logo::LogoDemo::new()),
        Box::new(sidebar::SidebarDemo::new()),
        Box::new(theme::ThemeDemo::new()),
        Box::new(chat::ChatDemo::new()),
        Box::new(file_changes::FileChangesDemo::new()),
        Box::new(chip::ChipDemo::new()),
        Box::new(rule::RuleDemo::new()),
        Box::new(key_help::KeyHelpDemo::new()),
        Box::new(status::StatusDemo::new()),
        Box::new(progress::ProgressDemo::new()),
        Box::new(panel::PanelDemo::new()),
        Box::new(toast::ToastDemo::new()),
        Box::new(spinners::SpinnersDemo::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use batten::widgets::{Anim, Theme};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn catalog_titles_are_unique() {
        let cat = catalog();
        assert!(!cat.is_empty());
        let mut titles: Vec<&str> = cat.iter().map(|d| d.title()).collect();
        let n = titles.len();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), n, "títulos repetidos");
    }

    #[test]
    fn every_demo_renders_without_panicking() {
        let anim = Anim::new();
        for mut demo in catalog() {
            for (w, h) in [(0u16, 0u16), (1, 1), (40, 10), (120, 40)] {
                demo.tick(&anim);
                let backend = TestBackend::new(w, h);
                let mut term = Terminal::new(backend).unwrap();
                term.draw(|f| {
                    let area = f.area();
                    demo.render(f, area, &Theme::test());
                })
                .unwrap();
            }
        }
    }
}
