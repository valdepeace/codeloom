//! Catálogo de demos del showcase.

mod activity_reference;
mod agent_states;
mod ansi;
mod button;
mod chat;
mod chip;
mod codeloom_tools;
mod composer;
mod crush_atoms;
mod crush_chat;
mod crush_chrome;
mod crush_code_surfaces;
mod crush_composer;
mod crush_context_sidebar;
mod crush_dialogs;
mod crush_landing;
mod crush_logo;
mod crush_message_states;
mod crush_misc;
mod crush_thinking;
mod crush_tool_call;
mod crush_tool_renderers;
mod dashboard;
mod dotmax_spinners;
mod dotmax_visual_lab;
mod file_changes;
mod fx;
mod key_help;
mod links;
mod logo;
mod martty_attachment_preview;
mod martty_command_menu;
mod martty_composer;
mod martty_harness_add;
mod martty_harness_picker;
mod martty_harness_remove;
mod martty_logo;
mod martty_model_picker;
mod martty_tool_call;
mod martty_welcome;
mod matrix_rain;
mod mindfork_chat_list;
mod mindfork_composer;
mod mindfork_help_tabs;
mod mindfork_light_components;
mod mindfork_logo;
mod mindfork_model_settings;
mod mindfork_reference;
mod mindfork_self_model;
mod mindfork_status_bar;
mod mindfork_tool_call;
mod mindfork_tools_settings;
mod panel;
mod pawan_fuzzy_search;
mod pawan_help;
mod pawan_messages;
mod pawan_model_picker;
mod pawan_permission;
mod pawan_queue;
mod pawan_status_bar;
mod pawan_tool_call;
mod pawan_welcome;
mod permission;
mod profile;
mod progress;
mod rule;
mod sidebar;
mod spinner;
mod spinners;
mod status;
mod theme;
mod thinking_reference;
mod toast;
mod tool_item;
mod transcript;

use crate::demo::Demo;

pub struct CatalogItem {
    pub component: &'static str,
    pub variant: &'static str,
    pub provider: &'static str,
    pub demo: Box<dyn Demo>,
}

fn provider_for(variant: &str) -> &'static str {
    [
        "Batten",
        "CodeLoom",
        "Crush",
        "Martty",
        "Mindfork",
        "Pawan",
        "MastraCode",
        "dotmax",
    ]
    .into_iter()
    .find(|provider| variant.starts_with(provider))
    .unwrap_or("Batten")
}

fn item(component: &'static str, variant: &'static str, demo: Box<dyn Demo>) -> CatalogItem {
    CatalogItem {
        component,
        variant,
        provider: provider_for(variant),
        demo,
    }
}

/// Todos los demos, en el orden del índice.
pub fn catalog() -> Vec<CatalogItem> {
    vec![
        // Mismas familias, variantes de cada implementación una junto a otra.
        item(
            "Composer",
            "Batten",
            Box::new(composer::ComposerDemo::new()),
        ),
        item(
            "Composer",
            "Mindfork",
            Box::new(mindfork_composer::MindforkComposerDemo::new()),
        ),
        item(
            "Composer",
            "Martty",
            Box::new(martty_composer::MarttyComposerDemo::new()),
        ),
        item(
            "Composer",
            "Crush",
            Box::new(crush_composer::CrushComposerDemo::new()),
        ),
        item("Chat", "Batten", Box::new(chat::ChatDemo::new())),
        item(
            "Dashboard",
            "Batten · workspace composite",
            Box::new(dashboard::DashboardDemo::new()),
        ),
        item(
            "Chat",
            "Mindfork",
            Box::new(mindfork_reference::MindforkReference::new()),
        ),
        item(
            "Chat",
            "Crush · sidebar + chat",
            Box::new(crush_chat::CrushChatDemo::new(true)),
        ),
        item(
            "Transcript",
            "Batten",
            Box::new(transcript::TranscriptDemo::new()),
        ),
        item(
            "Transcript",
            "Pawan · mensajes",
            Box::new(pawan_messages::PawanMessagesDemo::new()),
        ),
        item(
            "Transcript",
            "Crush",
            Box::new(crush_chat::CrushChatDemo::new(false)),
        ),
        item(
            "Assistant message",
            "Crush · pending",
            Box::new(crush_message_states::CrushMessageDemo::new(
                crush_message_states::CrushMessageKind::Pending,
            )),
        ),
        item(
            "Plan card",
            "Crush",
            Box::new(crush_message_states::CrushMessageDemo::new(
                crush_message_states::CrushMessageKind::Plan,
            )),
        ),
        item(
            "Assistant message",
            "Crush · refusal",
            Box::new(crush_message_states::CrushMessageDemo::new(
                crush_message_states::CrushMessageKind::Refusal,
            )),
        ),
        item(
            "Inline question",
            "Crush",
            Box::new(crush_message_states::CrushMessageDemo::new(
                crush_message_states::CrushMessageKind::Question,
            )),
        ),
        item(
            "Assistant message",
            "Crush · summary",
            Box::new(crush_message_states::CrushMessageDemo::new(
                crush_message_states::CrushMessageKind::Summary,
            )),
        ),
        item(
            "User message",
            "Crush · attachments",
            Box::new(crush_message_states::CrushMessageDemo::new(
                crush_message_states::CrushMessageKind::UserAttachment,
            )),
        ),
        item(
            "Tool call",
            "Batten",
            Box::new(tool_item::ToolItemDemo::new()),
        ),
        item(
            "Tool call",
            "Mindfork",
            Box::new(mindfork_tool_call::MindforkToolCallDemo::new()),
        ),
        item(
            "Tool call",
            "Martty",
            Box::new(martty_tool_call::MarttyToolCallDemo::new()),
        ),
        item(
            "Tool call",
            "Pawan",
            Box::new(pawan_tool_call::PawanToolCallDemo::new()),
        ),
        item(
            "Tool call",
            "Crush · generic states",
            Box::new(crush_tool_call::CrushToolCallDemo::new()),
        ),
        item(
            "Tool renderer",
            "Crush · shell/jobs",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Shell,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · files",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::File,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · search",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Search,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · web",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Web,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · agents",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Agent,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · MCP",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Mcp,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · diagnostics/LSP",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Diagnostics,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · todos",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Todos,
            )),
        ),
        item(
            "Tool renderer",
            "Crush · generic fallback",
            Box::new(crush_tool_renderers::CrushToolRendererDemo::new(
                crush_tool_renderers::CrushToolKind::Generic,
            )),
        ),
        item(
            "Diff output",
            "Crush",
            Box::new(crush_code_surfaces::CrushDiffDemo::new()),
        ),
        item(
            "Code navigation tools",
            "Crush",
            Box::new(crush_code_surfaces::CrushCodeNavigationDemo::new()),
        ),
        item(
            "Thinking",
            "Mindfork + Martty",
            Box::new(thinking_reference::ThinkingReferenceDemo::new()),
        ),
        item(
            "Thinking",
            "Crush",
            Box::new(crush_thinking::CrushThinkingDemo::new()),
        ),
        item("Status bar", "Batten", Box::new(status::StatusDemo::new())),
        item(
            "Status bar",
            "Mindfork",
            Box::new(mindfork_status_bar::MindforkStatusBarDemo::new()),
        ),
        item(
            "Status bar",
            "Pawan",
            Box::new(pawan_status_bar::PawanStatusBarDemo::new()),
        ),
        item(
            "Status bar",
            "Crush · help/footer",
            Box::new(crush_chrome::CrushFooterDemo::new()),
        ),
        item(
            "Compact header",
            "Crush",
            Box::new(crush_chrome::CrushCompactHeaderDemo::new()),
        ),
        item(
            "Welcome",
            "Martty",
            Box::new(martty_welcome::MarttyWelcomeDemo::new()),
        ),
        item(
            "Welcome",
            "Pawan",
            Box::new(pawan_welcome::PawanWelcomeDemo),
        ),
        item(
            "Welcome",
            "Crush · landing",
            Box::new(crush_landing::CrushLandingDemo),
        ),
        item(
            "Onboarding",
            "Crush · initialize",
            Box::new(crush_misc::CrushMiscDemo::new(
                crush_misc::CrushMiscKind::Initialize,
            )),
        ),
        item(
            "Session details",
            "Crush · compact overlay",
            Box::new(crush_misc::CrushMiscDemo::new(
                crush_misc::CrushMiscKind::SessionDetails,
            )),
        ),
        item(
            "Exit banner",
            "Crush",
            Box::new(crush_misc::CrushMiscDemo::new(
                crush_misc::CrushMiscKind::ExitBanner,
            )),
        ),
        item(
            "Inline editor",
            "Crush",
            Box::new(crush_misc::CrushMiscDemo::new(
                crush_misc::CrushMiscKind::InlineEditor,
            )),
        ),
        item(
            "Sidebar scroll",
            "Crush",
            Box::new(crush_misc::CrushMiscDemo::new(
                crush_misc::CrushMiscKind::SidebarScroll,
            )),
        ),
        item(
            "Help",
            "Crush · expanded hints",
            Box::new(crush_misc::CrushMiscDemo::new(
                crush_misc::CrushMiscKind::Help,
            )),
        ),
        item("Logo", "Batten", Box::new(logo::LogoDemo::new())),
        item(
            "Matrix rain",
            "Batten · background",
            Box::new(matrix_rain::MatrixRainDemo::new()),
        ),
        item(
            "Logo",
            "Batten · dot matrix 5×7",
            Box::new(logo::DotLogoDemo),
        ),
        item(
            "Logo",
            "Crush · wide title art",
            Box::new(crush_logo::CrushLogoDemo(
                crush_logo::CrushLogoVariant::WideTitle,
            )),
        ),
        item(
            "Logo",
            "Crush · sidebar",
            Box::new(crush_logo::CrushLogoDemo(
                crush_logo::CrushLogoVariant::Sidebar,
            )),
        ),
        item(
            "Logo",
            "Crush · sidebar fallback",
            Box::new(crush_logo::CrushLogoDemo(
                crush_logo::CrushLogoVariant::SidebarFallback,
            )),
        ),
        item(
            "Logo",
            "Martty · whale + wordmark",
            Box::new(martty_logo::MarttyLogoDemo::new()),
        ),
        item(
            "Logo",
            "Mindfork · glyph, wordmark y lockup",
            Box::new(mindfork_logo::MindforkLogoDemo),
        ),
        item(
            "Model picker",
            "Martty",
            Box::new(martty_model_picker::MarttyModelPickerDemo::new()),
        ),
        item(
            "Model picker",
            "Pawan",
            Box::new(pawan_model_picker::PawanModelPickerDemo::new()),
        ),
        item(
            "Model picker",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Models,
            )),
        ),
        item(
            "Permission",
            "Batten",
            Box::new(permission::PermissionDemo::new()),
        ),
        item(
            "Permission",
            "Pawan",
            Box::new(pawan_permission::PawanPermissionDemo),
        ),
        item(
            "Permission",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Permission,
            )),
        ),
        item(
            "Search",
            "Pawan · fuzzy",
            Box::new(pawan_fuzzy_search::PawanFuzzySearchDemo::new()),
        ),
        item("Help", "Pawan", Box::new(pawan_help::PawanHelpDemo)),
        item(
            "Help",
            "Mindfork · tab strip",
            Box::new(mindfork_help_tabs::MindforkHelpTabsDemo::new()),
        ),
        item(
            "Task queue",
            "Pawan",
            Box::new(pawan_queue::PawanQueueDemo::new()),
        ),
        item(
            "Conversation list",
            "Crush · sessions",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Sessions,
            )),
        ),
        item(
            "Conversation list",
            "Mindfork",
            Box::new(mindfork_chat_list::MindforkChatListDemo::new()),
        ),
        item(
            "Settings",
            "Mindfork · Tools",
            Box::new(mindfork_tools_settings::MindforkToolsSettingsDemo),
        ),
        item(
            "Settings",
            "Mindfork · Model-server",
            Box::new(mindfork_model_settings::MindforkModelSettingsDemo),
        ),
        item(
            "Settings",
            "Mindfork · light",
            Box::new(mindfork_light_components::MindforkLightComponentsDemo::new()),
        ),
        item(
            "Harness picker",
            "Martty · seleccionar",
            Box::new(martty_harness_picker::MarttyHarnessPickerDemo::new()),
        ),
        item(
            "Harness picker",
            "Martty · añadir",
            Box::new(martty_harness_add::MarttyHarnessAddDemo::new()),
        ),
        item(
            "Harness picker",
            "Martty · eliminar",
            Box::new(martty_harness_remove::MarttyHarnessRemoveDemo::new()),
        ),
        item(
            "Attachment preview",
            "Martty",
            Box::new(martty_attachment_preview::MarttyAttachmentPreviewDemo::new()),
        ),
        item(
            "Command menu",
            "Martty",
            Box::new(martty_command_menu::MarttyCommandMenuDemo::new()),
        ),
        item(
            "Command menu",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Commands,
            )),
        ),
        item(
            "File picker",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Files,
            )),
        ),
        item(
            "Question dialog",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Question,
            )),
        ),
        item(
            "Autocomplete",
            "Crush",
            Box::new(crush_atoms::CrushAtomDemo::new(
                crush_atoms::CrushAtomKind::Completions,
            )),
        ),
        item(
            "Attachment chips",
            "Crush",
            Box::new(crush_atoms::CrushAtomDemo::new(
                crush_atoms::CrushAtomKind::Attachments,
            )),
        ),
        item(
            "Todo / queue pills",
            "Crush",
            Box::new(crush_atoms::CrushAtomDemo::new(
                crush_atoms::CrushAtomKind::Pills,
            )),
        ),
        item(
            "Settings",
            "Crush · providers",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Providers,
            )),
        ),
        item(
            "Credentials",
            "Crush · API key",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::ApiKey,
            )),
        ),
        item(
            "Credentials",
            "Crush · OAuth",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::OAuth,
            )),
        ),
        item(
            "Settings",
            "Crush · MCP",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Mcp,
            )),
        ),
        item(
            "Settings",
            "Crush · MCP authentication",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::McpAuth,
            )),
        ),
        item(
            "Settings",
            "Crush · router",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Router,
            )),
        ),
        item(
            "Settings",
            "Crush · notifications",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Notifications,
            )),
        ),
        item(
            "Settings",
            "Crush · themes",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Themes,
            )),
        ),
        item(
            "Theme editor",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::ThemeEditor,
            )),
        ),
        item(
            "Tool arguments",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Arguments,
            )),
        ),
        item(
            "Plan handoff",
            "Crush",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::PlanHandoff,
            )),
        ),
        item(
            "Confirmation",
            "Crush · quit",
            Box::new(crush_dialogs::CrushDialogDemo::new(
                crush_dialogs::CrushDialogKind::Quit,
            )),
        ),
        item(
            "Batten · primitivas exclusivas",
            "Button",
            Box::new(button::ButtonDemo::new()),
        ),
        item(
            "Batten · primitivas exclusivas",
            "File changes",
            Box::new(file_changes::FileChangesDemo::new()),
        ),
        item(
            "Chip / badge",
            "Batten · base",
            Box::new(chip::ChipDemo::new()),
        ),
        item(
            "Divider / rail",
            "Batten · base",
            Box::new(rule::RuleDemo::new()),
        ),
        item(
            "Key help",
            "Batten · base",
            Box::new(key_help::KeyHelpDemo::new()),
        ),
        item(
            "Progress / context bar",
            "Batten · base",
            Box::new(progress::ProgressDemo::new()),
        ),
        item("Panel", "Batten · base", Box::new(panel::PanelDemo::new())),
        item(
            "Batten · primitivas exclusivas",
            "Toast",
            Box::new(toast::ToastDemo::new()),
        ),
        item(
            "Activity spinner",
            "Batten · frames",
            Box::new(spinners::SpinnersDemo::new()),
        ),
        item(
            "Progress catalog",
            "dotmax · 60 temas",
            Box::new(dotmax_spinners::DotmaxSpinnersDemo::new()),
        ),
        item(
            "Braille graphics",
            "dotmax · visual lab",
            Box::new(dotmax_visual_lab::DotmaxVisualLabDemo::new()),
        ),
        item(
            "Batten · primitivas exclusivas",
            "Effects",
            Box::new(fx::FxDemo::new()),
        ),
        item(
            "Code output",
            "Batten · ANSI",
            Box::new(ansi::AnsiDemo::new()),
        ),
        item(
            "Links / mention",
            "Batten · base",
            Box::new(links::LinksDemo::new()),
        ),
        item(
            "Profile / self-model",
            "Batten",
            Box::new(profile::ProfileDemo::new()),
        ),
        item(
            "Profile / self-model",
            "Mindfork",
            Box::new(mindfork_self_model::MindforkSelfModelDemo),
        ),
        item(
            "Navigation rail",
            "Batten · base",
            Box::new(sidebar::SidebarDemo::new()),
        ),
        item(
            "Context sidebar",
            "Crush · full sidebar",
            Box::new(crush_context_sidebar::CrushContextSidebarDemo),
        ),
        item(
            "Tool activity",
            "Cross-reference comparison",
            Box::new(activity_reference::ActivityReferenceDemo::new()),
        ),
        item(
            "Batten · primitivas exclusivas",
            "Themes",
            Box::new(theme::ThemeDemo::new()),
        ),
        item(
            "Activity spinner",
            "Batten · single",
            Box::new(spinner::SpinnerDemo::new()),
        ),
        // Una entrada por herramienta real de CodeLoom, con su pieza de render.
        item(
            "CodeLoom tools",
            "CodeLoom · bash",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("bash")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · view",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("view")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · edit",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("edit")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · grep",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("grep")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · glob",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("glob")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · todo_write",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("todo_write")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · ask_user",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("ask_user")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · subagent",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("subagent")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · subagent_control",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("subagent_control")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · subagent_list",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("subagent_list")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · job_start",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("job_start")),
        ),
        item(
            "CodeLoom tools",
            "CodeLoom · job_list",
            Box::new(codeloom_tools::CodeLoomToolDemo::new("job_list")),
        ),
        item(
            "Agent state",
            "Batten · A2A",
            Box::new(agent_states::AgentStatesDemo::new()),
        ),
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
        let mut titles: Vec<&str> = cat.iter().map(|entry| entry.demo.title()).collect();
        let n = titles.len();
        titles.sort_unstable();
        titles.dedup();
        assert_eq!(titles.len(), n, "títulos repetidos");
    }

    #[test]
    fn agent_states_demo_is_registered() {
        let cat = catalog();
        assert!(cat
            .iter()
            .any(|entry| entry.component == "Agent state" && entry.demo.title() == "agent states"));
    }

    #[test]
    fn every_demo_renders_without_panicking() {
        let anim = Anim::new();
        for mut entry in catalog() {
            let demo = &mut entry.demo;
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
