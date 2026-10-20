//! Comandos locales del composer; nunca se envían al provider.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Help,
    New,
    Sessions,
    Settings,
    Queue,
    Tools,
    Theme,
    Cancel,
    Pause,
    Resume,
    Sidebar,
    Quit,
}

pub const COMMANDS: &[(Command, &str, &str)] = &[
    (Command::Help, "/help", "Keyboard shortcuts"),
    (Command::New, "/new", "New session (when idle)"),
    (Command::Sessions, "/sessions", "Switch session (when idle)"),
    (
        Command::Settings,
        "/settings",
        "Provider and model settings",
    ),
    (Command::Queue, "/queue", "Manage pending prompts"),
    (Command::Tools, "/tools", "Inspect recent tool calls"),
    (Command::Theme, "/theme", "Cycle terminal color theme"),
    (
        Command::Cancel,
        "/cancel",
        "Cancel active turn and pause queue",
    ),
    (Command::Pause, "/pause", "Pause pending prompts"),
    (Command::Resume, "/resume", "Resume pending prompts"),
    (Command::Sidebar, "/sidebar", "Toggle workspace sidebar"),
    (Command::Quit, "/quit", "Exit CodeLoom"),
];
