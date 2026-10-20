use crate::permission_flow::{dialog_for, PermissionReply};
use codeloom_core::config::{CodeLoomConfig, ProviderKind};
use codeloom_core::event::LiveEvent;
use codeloom_core::message::{ContentPart, Message, Role};
use codeloom_core::permission::{Decision, PermissionRequest};
use codeloom_core::session::{SessionId, SessionInfo};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Dónde va el teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Composer,
    Transcript,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionChoice {
    Resume(SessionId),
    New,
}

#[derive(Debug, Clone)]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    pub api_key_env: String,
}

impl From<&CodeLoomConfig> for ProviderSettings {
    fn from(config: &CodeLoomConfig) -> Self {
        Self {
            kind: config.provider.kind,
            model: config.model.clone(),
            base_url: config.provider.base_url.clone(),
            api_key_env: config.provider.api_key_env.clone(),
        }
    }
}

/// Petición de permiso en pantalla, con su canal de respuesta.
pub struct PendingPermission {
    pub dialog: batten::state::PermissionDialog,
    reply: PermissionReply,
}

pub struct App {
    /// Transcript persistente: se actualiza con los eventos, no por frame.
    pub transcript: batten::state::Transcript,
    /// Entrada de la respuesta en curso (los deltas se añaden a ella).
    assistant: Option<batten::state::EntryId>,
    pub composer: batten::state::Composer,
    pub slash_selected: usize,
    pub slash_dismissed: Option<String>,
    pub slash_feedback: Option<String>,
    pub history: batten::state::History,
    pub status: String,
    pub phase: String,
    phase_started_at: Option<Instant>,
    phase_elapsed: Duration,
    pub phase_durations: Vec<(String, Duration)>,
    pub should_quit: bool,
    pub cancel_requested: bool,
    pub(crate) last_escape: Option<Instant>,
    pub cwd: String,
    pub model: String,
    pub theme: batten::widgets::Theme,
    pub theme_preset: batten::widgets::ThemePreset,
    pub matrix_rain: batten::widgets::matrix_rain::MatrixRain,
    pub animation: batten::widgets::Anim,
    pub tools: VecDeque<batten::state::ToolItem>,
    /// Entrada del transcript de cada herramienta y las líneas con las que
    /// está sincronizada (usado por el look `Inline`).
    tool_entries: HashMap<String, (batten::state::EntryId, Vec<batten::state::Line>)>,
    pub jobs: BTreeMap<codeloom_core::job::JobId, codeloom_core::job::JobSnapshot>,
    pub subagents: BTreeMap<i64, codeloom_core::session::SubagentDescriptor>,
    pub tools_open: bool,
    pub tool_selected: usize,
    pub tool_scroll: (u16, u16),
    pub reasoning: String,
    pub thinking: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub turn_input_tokens: u64,
    pub turn_output_tokens: u64,
    pub current_step: usize,
    pub max_steps: usize,
    pub changes: batten::state::FileChanges,
    /// Scroll del transcript (sigue el final salvo que el usuario suba).
    pub viewport: batten::state::Viewport,
    pub layout: batten::widgets::transcript::Layout,
    pub focus: Focus,
    /// Líneas seleccionadas del transcript, si hay.
    pub selection: Option<batten::state::Selection>,
    /// Texto pendiente de enviar al portapapeles (lo consume el bucle).
    pub clipboard: Option<String>,
    /// Número de prompts pendientes en `pending_prompts`.
    pub queued: usize,
    /// Prompts pendientes; el worker recibe solo uno cada vez.
    pub pending_prompts: VecDeque<String>,
    pub queue_open: bool,
    pub queue_selected: usize,
    pub queue_paused: bool,
    pub queue_editing: bool,
    pub queue_editor: batten::state::Composer,
    pub turn_in_flight: bool,
    /// Diálogo de permiso abierto, si hay.
    pub permission: Option<PendingPermission>,
    /// Peticiones esperando a que se cierre la actual.
    permission_queue: VecDeque<(PermissionRequest, PermissionReply)>,
    /// `true` cuando se acaba de abrir un diálogo y falta lanzar su efecto.
    pub fx_pending: bool,
    /// Sessions list for the session picker UI.
    /// When empty, automatically triggers creation of a new session.
    /// When non-empty, shows existing sessions to resume.
    pub sessions: Vec<SessionInfo>,
    /// Index of the currently selected session in the picker.
    /// - 0 = first session
    /// - sessions.len() = "New session" option
    pub session_selected: usize,
    pub session_id: Option<SessionId>,
    pub session_query: String,
    pub session_searching: bool,
    pub session_picker_return_to_chat: bool,
    pub session_list_refresh_requested: bool,
    /// Whether the session picker overlay is open.
    /// - false: empty sessions → automatically creating new session
    /// - true: non-empty sessions → user can select existing or create new
    pub session_picker_open: bool,
    /// What the user chose in the picker (None = still choosing).
    /// Set when user presses Enter or 'n' in the session picker.
    pub session_choice: Option<SessionChoice>,
    /// Whether the sidebar is expanded (visible on all screen widths).
    pub sidebar_expanded: bool,
    /// `true` when loading a session (shows loading spinner).
    /// Set after user makes a selection and before messages are loaded.
    pub session_loading: bool,
    pub provider_settings: Option<ProviderSettings>,
    pub settings_open: bool,
    pub settings_field: usize,
    pub settings_editing: bool,
    pub settings_editor: batten::state::Composer,
    pub settings_save_requested: bool,
    pub settings_saving: bool,
    pub settings_feedback: Option<String>,
    pub help_open: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn cycle_theme(&mut self) {
        use batten::widgets::ThemePreset;
        let presets = ThemePreset::ALL;
        let current = presets
            .iter()
            .position(|preset| *preset == self.theme_preset)
            .unwrap_or(0);
        self.theme_preset = presets[(current + 1) % presets.len()];
        self.theme = batten::widgets::Theme::preset(self.theme_preset);
        self.sync_all_tools();
    }

    pub fn new() -> Self {
        Self {
            transcript: batten::state::Transcript::new(),
            assistant: None,
            composer: batten::state::Composer::new(),
            slash_selected: 0,
            slash_dismissed: None,
            slash_feedback: None,
            history: batten::state::History::new(),
            status: "ready".into(),
            phase: "ready".into(),
            phase_started_at: None,
            phase_elapsed: Duration::ZERO,
            phase_durations: Vec::new(),
            should_quit: false,
            cancel_requested: false,
            last_escape: None,
            cwd: String::new(),
            model: String::new(),
            theme: batten::widgets::Theme::default_dark(),
            theme_preset: batten::widgets::ThemePreset::Batten,
            matrix_rain: batten::widgets::matrix_rain::MatrixRain::default(),
            animation: batten::widgets::Anim::new(),
            tools: VecDeque::new(),
            tool_entries: HashMap::new(),
            jobs: BTreeMap::new(),
            subagents: BTreeMap::new(),
            tools_open: false,
            tool_selected: 0,
            tool_scroll: (0, 0),
            reasoning: String::new(),
            thinking: false,
            input_tokens: 0,
            output_tokens: 0,
            turn_input_tokens: 0,
            turn_output_tokens: 0,
            current_step: 0,
            max_steps: 0,
            changes: batten::state::FileChanges::new(),
            viewport: batten::state::Viewport::new(),
            layout: batten::widgets::transcript::Layout::new(),
            focus: Focus::Composer,
            selection: None,
            clipboard: None,
            queued: 0,
            pending_prompts: VecDeque::new(),
            queue_open: false,
            queue_selected: 0,
            queue_paused: false,
            queue_editing: false,
            queue_editor: batten::state::Composer::new(),
            turn_in_flight: false,
            permission: None,
            permission_queue: VecDeque::new(),
            fx_pending: false,
            sessions: Vec::new(),
            session_selected: 0,
            session_id: None,
            session_query: String::new(),
            session_searching: false,
            session_picker_return_to_chat: false,
            session_list_refresh_requested: false,
            session_picker_open: false,
            session_choice: None,
            session_loading: false,
            provider_settings: None,
            settings_open: false,
            settings_field: 0,
            settings_editing: false,
            settings_editor: batten::state::Composer::new(),
            settings_save_requested: false,
            settings_saving: false,
            settings_feedback: None,
            help_open: false,
            sidebar_expanded: false,
        }
    }

    pub fn with_context(cwd: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            cwd: cwd.into(),
            model: model.into(),
            ..Self::new()
        }
    }

    pub fn slash_query(&self) -> Option<&str> {
        let text = self.composer.text();
        if self.focus != Focus::Composer
            || self.composer.lines() != 1
            || self.permission.is_some()
            || self.session_picker_open
            || self.session_loading
            || self.settings_open
            || self.help_open
            || self.tools_open
            || self.queue_open
            || self.slash_dismissed.as_deref() == Some(text)
        {
            return None;
        }
        let query = text.trim_end().strip_prefix('/')?;
        (!query.contains(char::is_whitespace) && !query.contains('/')).then_some(text.trim_end())
    }

    pub fn slash_matches(&self) -> Vec<(crate::commands::Command, &'static str, &'static str)> {
        let Some(query) = self.slash_query() else {
            return Vec::new();
        };
        crate::commands::COMMANDS
            .iter()
            .copied()
            .filter(|(_, name, _)| name.starts_with(query))
            .collect()
    }

    pub fn set_provider_settings(&mut self, settings: ProviderSettings) {
        self.model = settings.model.clone();
        self.provider_settings = Some(settings);
    }

    pub fn open_settings(&mut self) {
        if self.provider_settings.is_some() {
            self.settings_open = true;
            self.settings_feedback = None;
        }
    }

    pub fn edit_settings_value(&mut self) {
        let Some(settings) = &self.provider_settings else {
            return;
        };
        let value = match self.settings_field {
            1 => &settings.model,
            2 => &settings.base_url,
            3 => &settings.api_key_env,
            _ => return,
        };
        self.settings_editor.set_text(value);
        self.settings_editing = true;
    }

    pub fn commit_settings_value(&mut self) {
        let value = self.settings_editor.text().to_string();
        if let Some(settings) = self.provider_settings.as_mut() {
            match self.settings_field {
                1 => settings.model = value,
                2 => settings.base_url = value,
                3 => settings.api_key_env = value,
                _ => {}
            }
        }
        self.settings_editing = false;
    }

    pub fn request_settings_save(&mut self) {
        if self.settings_saving {
            return;
        }
        if self.turn_in_flight {
            self.settings_feedback =
                Some("Wait for the active turn before applying settings".into());
            return;
        }
        let Some(settings) = self.provider_settings.as_mut() else {
            return;
        };
        settings.model = settings.model.trim().to_string();
        settings.base_url = settings.base_url.trim().to_string();
        settings.api_key_env = settings.api_key_env.trim().to_string();
        if settings.model.trim().is_empty()
            || settings.base_url.trim().is_empty()
            || settings.api_key_env.trim().is_empty()
        {
            self.settings_feedback = Some("Model, URL and API key variable are required".into());
            return;
        }
        self.settings_save_requested = true;
        self.settings_feedback = None;
    }

    pub fn settings_saved(&mut self, result: Result<(), String>) {
        self.settings_saving = false;
        self.settings_feedback = Some(match result {
            Ok(()) => {
                if let Some(settings) = &self.provider_settings {
                    self.model = settings.model.clone();
                }
                "Settings saved and applied".into()
            }
            Err(error) => error,
        });
    }

    /// Set the sessions list and update the session picker state.
    ///
    /// When the sessions list is empty, automatically starts creating a new session:
    /// - Sets session_picker_open = false (new session starts immediately)
    /// - Sets session_loading = true (shows loading spinner)
    /// - Sets session_choice = Some(New) (queues new session creation)
    ///
    /// When sessions list is non-empty, shows the session picker for resuming:
    /// - Sets session_picker_open = true (user can select a session)
    /// - Sets session_loading = false (picker is available immediately)
    /// - Leaves session_choice = None (waiting for user selection)
    pub fn set_sessions(&mut self, sessions: Vec<SessionInfo>) {
        let is_empty = sessions.is_empty();
        self.sessions = sessions;
        self.session_query.clear();
        self.session_searching = false;
        self.session_choice = None;

        if is_empty {
            // No existing sessions - create new one
            self.session_picker_open = false;
            self.session_loading = true;
            self.session_choice = Some(SessionChoice::New);
        } else {
            // Existing sessions available - show picker
            self.session_picker_open = true;
            self.session_loading = false;
        }
    }

    pub fn matching_sessions(&self) -> impl Iterator<Item = &SessionInfo> {
        let query = self.session_query.to_lowercase();
        self.sessions
            .iter()
            .filter(move |session| session.preview.to_lowercase().contains(&query))
    }

    pub fn update_sessions(&mut self, sessions: Vec<SessionInfo>) {
        let selected = self
            .matching_sessions()
            .nth(self.session_selected)
            .map(|session| session.id);
        let selected_new = self.session_selected == self.matching_sessions().count();
        self.sessions = sessions;
        let matches = self.matching_sessions().collect::<Vec<_>>();
        self.session_selected = if selected_new {
            matches.len()
        } else {
            selected
                .and_then(|id| matches.iter().position(|session| session.id == id))
                .unwrap_or_else(|| self.session_selected.min(matches.len()))
        };
        self.session_picker_open = true;
        self.session_loading = false;
    }

    pub fn restore_messages(&mut self, messages: Vec<Message>) {
        self.transcript = batten::state::Transcript::new();
        self.assistant = None;
        self.history = batten::state::History::new();
        self.tools.clear();
        self.tool_entries.clear();
        self.tools_open = false;
        self.tool_selected = 0;
        self.tool_scroll = (0, 0);
        self.changes = batten::state::FileChanges::new();
        self.input_tokens = 0;
        self.output_tokens = 0;
        self.turn_input_tokens = 0;
        self.turn_output_tokens = 0;
        for message in messages {
            if let Some(usage) = message.usage.as_ref() {
                self.input_tokens = self
                    .input_tokens
                    .saturating_add(u64::from(usage.input_tokens));
                self.output_tokens = self
                    .output_tokens
                    .saturating_add(u64::from(usage.output_tokens));
            }
            match message.role {
                Role::User => {
                    let text = message.text();
                    self.history.push(text.clone());
                    self.push_user(&text);
                }
                Role::Assistant => {
                    for part in message.parts {
                        match part {
                            ContentPart::Text { text } => self.push_assistant(&text),
                            ContentPart::ToolCall { id, name, input } => {
                                self.tool_mut(&id, &name).set_summary(input.to_string());
                                self.sync_tool(&id);
                            }
                            _ => {}
                        }
                    }
                }
                Role::Tool => {
                    for part in message.parts {
                        if let ContentPart::ToolResult {
                            call_id,
                            content,
                            is_error,
                        } = part
                        {
                            if let Some(tool) = self.tools.iter_mut().find(|t| t.call_id == call_id)
                            {
                                tool.set_result(is_error);
                                tool.set_output(content);
                            }
                            self.sync_tool(&call_id);
                        }
                    }
                }
                Role::System => {}
            }
        }
        self.assistant = None;
        for tool in &mut self.tools {
            if tool.is_spinning() {
                tool.cancel();
            }
        }
        self.sync_all_tools();
        self.viewport.follow_bottom();
        self.status = "ready".into();
        self.phase = "ready".into();
        self.phase_started_at = None;
        self.phase_elapsed = Duration::ZERO;
        self.phase_durations.clear();
        self.current_step = 0;
        self.max_steps = 0;
        self.session_loading = false;
    }

    pub fn load_session(&mut self, session: SessionId, messages: Vec<Message>) {
        self.session_id = Some(session);
        self.subagents.clear();
        self.session_selected = 0;
        self.session_query.clear();
        self.session_searching = false;
        self.session_choice = None;
        self.session_loading = false;
        self.session_picker_open = false;
        self.session_picker_return_to_chat = false;
        self.restore_messages(messages);
    }

    pub fn load_subagents(
        &mut self,
        session: SessionId,
        subagents: Vec<codeloom_core::session::SubagentDescriptor>,
    ) {
        if self.session_id == Some(session) {
            self.subagents = subagents
                .into_iter()
                .map(|subagent| (subagent.child.0, subagent))
                .collect();
        }
    }

    pub fn session_load_failed(&mut self) {
        self.session_loading = false;
        self.session_picker_open = !self.sessions.is_empty();
        self.session_picker_return_to_chat = false;
    }

    /// Alterna el foco entre composer y transcript.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Composer => Focus::Transcript,
            Focus::Transcript => Focus::Composer,
        };
    }

    /// Encola una petición; si no hay diálogo abierto, la muestra ya.
    pub fn enqueue_permission(&mut self, req: PermissionRequest, reply: PermissionReply) {
        self.tools_open = false;
        self.queue_open = false;
        self.queue_editing = false;
        self.help_open = false;
        self.settings_open = false;
        self.settings_editing = false;
        self.permission_queue.push_back((req, reply));
        self.open_next_permission();
    }

    /// Responde al diálogo abierto y pasa al siguiente de la cola.
    pub fn resolve_permission(&mut self, decision: Decision) {
        if let Some(p) = self.permission.take() {
            let _ = p.reply.send(decision);
        }
        self.open_next_permission();
    }

    fn open_next_permission(&mut self) {
        if self.permission.is_some() {
            return;
        }
        while let Some((req, reply)) = self.permission_queue.pop_front() {
            if reply.is_closed() {
                continue;
            }
            self.permission = Some(PendingPermission {
                dialog: dialog_for(&req),
                reply,
            });
            self.fx_pending = true;
            break;
        }
    }

    /// Mensaje del usuario: entrada nueva; la siguiente respuesta empieza otra.
    pub fn push_user(&mut self, text: &str) {
        use batten::state::{Format, LineKind};
        self.transcript
            .push_entry(LineKind::User, Format::Plain, text);
        self.assistant = None;
    }

    /// Delta de la respuesta: se añade a la entrada en curso (markdown).
    pub fn push_assistant(&mut self, delta: &str) {
        use batten::state::{Format, LineKind};
        match self.assistant {
            Some(id) => self.transcript.append(id, delta),
            None => {
                let id = self
                    .transcript
                    .push_entry(LineKind::Assistant, Format::Markdown, delta);
                self.assistant = Some(id);
            }
        }
    }

    pub fn apply(&mut self, ev: LiveEvent) {
        match ev {
            LiveEvent::TurnStarted => {
                self.turn_in_flight = true;
                self.status = "working".into();
                self.finish_phase();
                self.phase_durations.clear();
                self.phase = "preparing".into();
                self.phase_elapsed = Duration::ZERO;
                self.phase_started_at = Some(Instant::now());
                self.matrix_rain.start();
                self.reasoning.clear();
                self.thinking = false;
                self.current_step = 0;
                self.max_steps = 0;
                self.turn_input_tokens = 0;
                self.turn_output_tokens = 0;
            }
            LiveEvent::StepStarted { step, max_steps } => {
                self.current_step = step;
                self.max_steps = max_steps;
                self.set_phase("waiting for provider");
            }
            LiveEvent::TurnEnded => {
                self.turn_in_flight = false;
                self.cancel_requested = false;
                self.status = "ready".into();
                self.finish_phase();
                self.matrix_rain.stop();
                self.assistant = None;
                self.thinking = false;
            }
            LiveEvent::Delta { text } => {
                self.set_phase("responding");
                self.thinking = false;
                self.push_assistant(&text)
            }
            LiveEvent::ReasoningDelta { text } => {
                self.set_phase("thinking");
                self.thinking = true;
                self.reasoning.push_str(&text);
            }
            LiveEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                self.input_tokens = self.input_tokens.saturating_add(u64::from(input_tokens));
                self.output_tokens = self.output_tokens.saturating_add(u64::from(output_tokens));
                self.turn_input_tokens = self
                    .turn_input_tokens
                    .saturating_add(u64::from(input_tokens));
                self.turn_output_tokens = self
                    .turn_output_tokens
                    .saturating_add(u64::from(output_tokens));
            }
            LiveEvent::ToolStarted { call_id, name } => {
                self.set_phase("running tools");
                let tool = self.tool_mut(&call_id, &name);
                tool.start();
                self.sync_tool(&call_id);
            }
            LiveEvent::ToolFinished { call_id, is_error } => {
                if let Some(tool) = self.tools.iter_mut().find(|tool| tool.call_id == call_id) {
                    tool.set_result(is_error);
                }
                self.sync_tool(&call_id);
                if !self
                    .tools
                    .iter()
                    .any(|tool| tool.status() == batten::state::ToolStatus::Running)
                {
                    self.finish_phase_as(if is_error {
                        "tool failed"
                    } else {
                        "tools complete"
                    });
                }
            }
            LiveEvent::MessageAdded(message) => self.apply_message_added(message),
            LiveEvent::SessionMessageAdded { session, message } => {
                if self.session_id == Some(session) {
                    self.apply_message_added(message);
                }
            }
            LiveEvent::Error { message } => {
                self.turn_in_flight = false;
                self.cancel_requested = false;
                self.status = format!("error: {message}");
                self.finish_phase();
                self.thinking = false;
                self.matrix_rain.stop();
            }
            LiveEvent::FileDiff {
                path,
                added,
                removed,
            } => self.changes.upsert(batten::state::FileChange {
                path,
                added,
                removed,
            }),
            LiveEvent::JobChanged(snapshot) => {
                if snapshot.kind == "subagent" {
                    if let Some(subagent) = self
                        .subagents
                        .values_mut()
                        .find(|subagent| subagent.job_id == snapshot.id.0)
                    {
                        subagent.status = match snapshot.status {
                            codeloom_core::job::JobStatus::Running
                            | codeloom_core::job::JobStatus::Stopping => {
                                codeloom_core::session::SubagentStatus::Running
                            }
                            codeloom_core::job::JobStatus::Completed => {
                                codeloom_core::session::SubagentStatus::Completed
                            }
                            codeloom_core::job::JobStatus::Killed => {
                                codeloom_core::session::SubagentStatus::Killed
                            }
                            codeloom_core::job::JobStatus::Failed => {
                                codeloom_core::session::SubagentStatus::Failed
                            }
                        };
                    }
                }
                self.jobs.insert(snapshot.id.clone(), snapshot);
            }
            LiveEvent::SubagentChanged(subagent) => {
                if Some(subagent.parent) == self.session_id {
                    self.subagents.insert(subagent.child.0, subagent);
                }
            }
        }
    }

    fn apply_message_added(&mut self, message: Message) {
        if message.role == Role::User {
            self.push_user(&message.text());
        } else {
            for part in message.parts {
                match part {
                    ContentPart::ToolCall { id, name, input } => {
                        self.tool_mut(&id, &name).set_summary(input.to_string());
                        self.sync_tool(&id);
                    }
                    ContentPart::ToolResult {
                        call_id,
                        content,
                        is_error,
                    } => {
                        if let Some(tool) =
                            self.tools.iter_mut().find(|tool| tool.call_id == call_id)
                        {
                            tool.set_result(is_error);
                            tool.set_output(content);
                        }
                        self.sync_tool(&call_id);
                    }
                    _ => {}
                }
            }
        }
    }

    fn set_phase(&mut self, phase: &str) {
        if self.phase != phase {
            self.finish_phase();
            self.phase = phase.into();
            self.phase_elapsed = Duration::ZERO;
            self.phase_started_at = Some(Instant::now());
        }
    }

    fn finish_phase(&mut self) {
        if let Some(started_at) = self.phase_started_at.take() {
            self.phase_elapsed = started_at.elapsed();
            self.record_phase(self.phase_elapsed);
        }
    }

    fn finish_phase_as(&mut self, phase: &str) {
        let elapsed = self
            .phase_started_at
            .map_or(self.phase_elapsed, |started_at| started_at.elapsed());
        if self.phase_started_at.take().is_some() {
            self.record_phase(elapsed);
        }
        self.phase = phase.into();
        self.phase_elapsed = elapsed;
    }

    fn record_phase(&mut self, elapsed: Duration) {
        let name = match self.phase.as_str() {
            "waiting for provider" => "Provider",
            "thinking" => "Thinking",
            "running tools" => "Tools",
            _ => return,
        };
        if let Some((_, total)) = self.phase_durations.iter_mut().find(|(key, _)| key == name) {
            *total += elapsed;
        } else {
            self.phase_durations.push((name.into(), elapsed));
        }
    }

    pub fn phase_summary(&self) -> String {
        self.phase_durations
            .iter()
            .map(|(name, duration)| format!("{name} {:.1}s", duration.as_secs_f64()))
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn phase_duration(&self) -> Duration {
        self.phase_started_at
            .map_or(self.phase_elapsed, |started_at| started_at.elapsed())
    }

    fn tool_mut(&mut self, call_id: &str, name: &str) -> &mut batten::state::ToolItem {
        if let Some(index) = self.tools.iter().position(|tool| tool.call_id == call_id) {
            return self.tools.get_mut(index).unwrap();
        }
        if self.tools.len() == 8 {
            self.tools.pop_front();
            if self.tool_selected == 0 {
                self.tool_scroll = (0, 0);
            }
            self.tool_selected = self.tool_selected.saturating_sub(1);
        }
        self.tools
            .push_back(batten::state::ToolItem::new(call_id, name));
        self.tools.back_mut().unwrap()
    }

    /// Visibilidad de las herramientas en el transcript (look `Inline`).
    fn tool_inline(&self) -> bool {
        self.theme.look.tool == batten::widgets::ToolStyle::Inline
    }

    /// Sincroniza las líneas de una herramienta con su entrada del transcript.
    /// Solo reemplaza la entrada si sus líneas cambiaron: no se llama desde el
    /// camino de los deltas, que se ejecuta por cada token.
    pub fn sync_tool(&mut self, call_id: &str) {
        let Some(item) = self.tools.iter().find(|tool| tool.call_id == call_id) else {
            return;
        };
        let lines = batten::state::tool_lines(item);
        let inline = self.tool_inline();
        match self.tool_entries.get_mut(call_id) {
            Some((id, cache)) => {
                if *cache == lines {
                    return;
                }
                let shown = if inline { lines.clone() } else { Vec::new() };
                *cache = lines;
                self.transcript.replace(*id, shown);
            }
            // La entrada se crea siempre, para que un cambio posterior de look
            // la encuentre en su sitio cronológico. Con el look clásico nace
            // vacía y no corta el texto del asistente.
            None => {
                let shown = if inline { lines.clone() } else { Vec::new() };
                let id = self.transcript.push_lines(shown);
                self.tool_entries.insert(call_id.to_string(), (id, lines));
                if inline {
                    self.assistant = None;
                }
            }
        }
    }

    /// Aplica la visibilidad del look actual a todas las herramientas.
    pub fn sync_all_tools(&mut self) {
        let inline = self.tool_inline();
        for (id, lines) in self.tool_entries.values() {
            let shown = if inline { lines.clone() } else { Vec::new() };
            self.transcript.replace(*id, shown);
        }
        let ids: Vec<String> = self.tools.iter().map(|tool| tool.call_id.clone()).collect();
        for id in ids {
            self.sync_tool(&id);
        }
    }

    /// Texto pegado (bracketed paste): entra de una vez en el composer y
    /// nunca envía. Se ignora con un diálogo abierto o sin foco en el composer.
    pub fn paste(&mut self, text: &str) {
        if self.permission.is_some() || self.focus != Focus::Composer {
            return;
        }
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        self.composer.insert_str(&text);
        self.history.reset();
    }

    pub fn push_char(&mut self, c: char) {
        self.composer.insert_char(c);
        self.history.reset();
    }
    pub fn newline(&mut self) {
        self.composer.newline();
        self.history.reset();
    }
    pub fn backspace(&mut self) {
        self.composer.backspace();
        self.history.reset();
    }
    pub fn delete(&mut self) {
        self.composer.delete();
        self.history.reset();
    }
    pub fn move_left(&mut self) {
        self.composer.move_left();
    }
    pub fn move_right(&mut self) {
        self.composer.move_right();
    }
    pub fn move_home(&mut self) {
        self.composer.move_home();
    }
    pub fn move_end(&mut self) {
        self.composer.move_end();
    }

    /// ↑: cursor si hay varias líneas; si no, historial.
    pub fn up(&mut self) {
        if !self.history.is_browsing() && self.composer.lines() > 1 {
            self.composer.move_up();
            return;
        }
        let current = self.composer.text().to_string();
        if let Some(prev) = self.history.prev(&current) {
            let prev = prev.to_string();
            self.composer.set_text(&prev);
        }
    }

    /// ↓: historial si se está navegando; si no, cursor.
    pub fn down(&mut self) {
        if !self.history.is_browsing() {
            self.composer.move_down();
            return;
        }
        if let Some(next) = self.history.next() {
            let next = next.to_string();
            self.composer.set_text(&next);
        }
    }

    pub fn submit(&mut self) -> Option<String> {
        let text = self.composer.submit()?;
        self.history.push(text.clone());
        Some(text)
    }

    /// Toggle the sidebar visibility.
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_expanded = !self.sidebar_expanded;
    }

    /// Construye el sidebar de batten con la marca y las secciones de sesión.
    pub fn sidebar(&self) -> batten::state::Sidebar {
        let mut sb = batten::state::Sidebar::with_logo("CodeLoom");
        sb.push_section("AGENT", vec![self.status.clone(), self.model.clone()]);
        sb.push_section(
            "PROJECT",
            vec![
                self.cwd.clone(),
                format!("{} changed files", self.changes.len()),
            ],
        );
        let running = self
            .tools
            .iter()
            .filter(|tool| tool.status() == batten::state::ToolStatus::Running)
            .count();
        sb.push_section(
            "FLOCK",
            vec![
                format!("{running} tools running"),
                format!(
                    "{} jobs running",
                    self.jobs
                        .values()
                        .filter(|job| Some(job.owner) == self.session_id)
                        .filter(|job| matches!(
                            job.status,
                            codeloom_core::job::JobStatus::Running
                                | codeloom_core::job::JobStatus::Stopping
                        ))
                        .count()
                ),
                format!("{} prompts queued", self.queued),
                format!(
                    "{} subagents running",
                    self.subagents
                        .values()
                        .filter(|subagent| {
                            Some(subagent.parent) == self.session_id
                                && subagent.status
                                    == codeloom_core::session::SubagentStatus::Running
                        })
                        .count()
                ),
            ],
        );
        let mut session = Vec::new();
        if let Some(id) = self.session_id {
            session.push(format!("session #{}", id.0));
        }
        session.push(format!(
            "↑ {} · ↓ {}",
            self.input_tokens, self.output_tokens
        ));
        sb.push_section("SESSION", session);
        sb
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deltas_accumulate_into_one_item() {
        let mut app = App::new();
        app.apply(codeloom_core::event::LiveEvent::Delta { text: "a".into() });
        app.apply(codeloom_core::event::LiveEvent::Delta { text: "b".into() });
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(app.transcript.line(0).unwrap().plain(), "ab");
    }

    #[test]
    fn submit_clears_composer() {
        let mut app = App::new();
        app.push_char('h');
        app.push_char('i');
        assert_eq!(app.submit(), Some("hi".to_string()));
        assert!(app.composer.is_empty());
        assert_eq!(app.submit(), None);
    }

    #[test]
    fn user_line_comes_from_the_event_not_from_submit() {
        let mut app = App::new();
        app.push_char('h');
        app.push_char('i');
        assert_eq!(app.submit(), Some("hi".to_string()));
        assert!(app.transcript.is_empty());
        app.apply(codeloom_core::event::LiveEvent::MessageAdded(
            codeloom_core::message::Message::user("hi"),
        ));
        assert_eq!(
            app.transcript.line(0).unwrap().kind,
            batten::state::LineKind::User
        );
    }

    #[test]
    fn scoped_subagent_notices_only_reach_their_session() {
        let mut app = App::new();
        app.session_id = Some(SessionId(2));
        let message = Message::user("[Subagent review completed]\nready");
        app.apply(LiveEvent::SessionMessageAdded {
            session: SessionId(1),
            message: message.clone(),
        });
        assert!(app.transcript.is_empty());
        app.apply(LiveEvent::SessionMessageAdded {
            session: SessionId(2),
            message,
        });
        assert_eq!(
            app.transcript.line(0).unwrap().plain(),
            "[Subagent review completed]"
        );
        assert_eq!(app.transcript.line(1).unwrap().plain(), "ready");
    }

    #[test]
    fn loading_a_session_restores_and_scopes_durable_subagents() {
        let mut app = App::new();
        let parent = SessionId(2);
        let child = SessionId(9);
        app.load_session(parent, Vec::new());
        app.load_subagents(
            parent,
            vec![codeloom_core::session::SubagentDescriptor {
                parent,
                child,
                job_id: "subagent-1".into(),
                label: "review".into(),
                status: codeloom_core::session::SubagentStatus::Interrupted,
                result: Some("partial output".into()),
                notice_delivered: false,
            }],
        );
        assert_eq!(
            app.subagents.get(&child.0).unwrap().status,
            codeloom_core::session::SubagentStatus::Interrupted
        );

        app.load_session(SessionId(3), Vec::new());
        assert!(app.subagents.is_empty());
    }

    #[test]
    fn a_user_message_starts_a_new_answer_entry() {
        let mut app = App::new();
        app.push_assistant("uno");
        app.push_user("pregunta");
        app.push_assistant("dos");
        assert_eq!(app.transcript.entry_count(), 3);
        assert_eq!(app.transcript.line(2).unwrap().plain(), "dos");
    }

    #[test]
    fn restores_transcript_history_and_tool_results() {
        let mut app = App::new();
        app.turn_input_tokens = 99;
        app.turn_output_tokens = 50;
        app.restore_messages(vec![
            Message::user("hola"),
            Message::assistant(vec![
                ContentPart::Text {
                    text: "respuesta".into(),
                },
                ContentPart::ToolCall {
                    id: "c1".into(),
                    name: "view".into(),
                    input: serde_json::json!({"path":"src/main.rs"}),
                },
            ]),
            Message::new(
                Role::Tool,
                vec![ContentPart::ToolResult {
                    call_id: "c1".into(),
                    content: "contenido".into(),
                    is_error: false,
                }],
            ),
        ]);

        // Con el look clásico la entrada de la tool existe pero está vacía: lo
        // visible son solo las dos líneas de los mensajes.
        assert_eq!(app.transcript.len(), 2);
        assert_eq!(app.transcript.line(0).unwrap().plain(), "hola");
        assert_eq!(app.transcript.line(1).unwrap().plain(), "respuesta");
        assert_eq!(app.history.prev(""), Some("hola"));
        assert_eq!(app.tools.len(), 1);
        assert_eq!(app.tools[0].output.as_deref(), Some("contenido"));
        assert_eq!(app.tools[0].status(), batten::state::ToolStatus::Success);
        assert_eq!((app.turn_input_tokens, app.turn_output_tokens), (0, 0));
    }

    #[test]
    fn empty_session_list_starts_a_new_session_and_nonempty_list_opens_picker() {
        let mut app = App::new();
        app.set_sessions(Vec::new());
        assert!(app.session_loading);
        assert_eq!(app.session_choice, Some(SessionChoice::New));

        app.set_sessions(vec![SessionInfo {
            id: SessionId(1),
            preview: "pregunta anterior".into(),
            message_count: 2,
        }]);
        assert!(app.session_picker_open);
        assert!(!app.session_loading);
    }

    #[test]
    fn updating_sessions_keeps_selected_session_or_new_choice() {
        let mut app = App::new();
        app.set_sessions(vec![
            SessionInfo {
                id: SessionId(1),
                preview: "one".into(),
                message_count: 1,
            },
            SessionInfo {
                id: SessionId(2),
                preview: "two".into(),
                message_count: 1,
            },
        ]);
        app.session_selected = 1;
        app.update_sessions(vec![
            SessionInfo {
                id: SessionId(2),
                preview: "two".into(),
                message_count: 1,
            },
            SessionInfo {
                id: SessionId(1),
                preview: "one".into(),
                message_count: 1,
            },
        ]);
        assert_eq!(app.session_selected, 0);

        app.session_selected = 2;
        app.update_sessions(vec![SessionInfo {
            id: SessionId(2),
            preview: "two".into(),
            message_count: 1,
        }]);
        assert_eq!(app.session_selected, 1);
    }

    #[test]
    fn loading_session_clears_picker_search_state() {
        let mut app = App::new();
        app.changes.upsert(batten::state::FileChange {
            path: "old.rs".into(),
            added: 1,
            removed: 0,
        });
        app.session_selected = 3;
        app.session_query = "old filter".into();
        app.session_searching = true;
        app.session_choice = Some(SessionChoice::New);
        app.session_loading = true;
        app.session_picker_open = true;
        app.session_picker_return_to_chat = true;

        app.load_session(SessionId(7), Vec::new());

        assert_eq!(app.session_id, Some(SessionId(7)));
        assert_eq!(app.session_selected, 0);
        assert!(app.session_query.is_empty());
        assert!(!app.session_searching);
        assert_eq!(app.session_choice, None);
        assert!(!app.session_loading);
        assert!(!app.session_picker_open);
        assert!(!app.session_picker_return_to_chat);
        assert!(app.changes.is_empty());
    }

    #[test]
    fn failed_session_load_reopens_picker_when_sessions_exist() {
        let mut app = App::new();
        app.sessions = vec![SessionInfo {
            id: SessionId(1),
            preview: "previous".into(),
            message_count: 2,
        }];
        app.session_loading = true;
        app.session_picker_return_to_chat = true;

        app.session_load_failed();

        assert!(!app.session_loading);
        assert!(app.session_picker_open);
        assert!(!app.session_picker_return_to_chat);
    }

    #[test]
    fn with_context_stores_cwd_and_model() {
        let app = App::with_context("/tmp/proj", "mistral-nemo");
        assert_eq!(app.cwd, "/tmp/proj");
        assert_eq!(app.model, "mistral-nemo");
        assert_eq!(app.status, "ready");
    }

    #[test]
    fn sidebar_has_logo_and_session_sections() {
        let mut app = App::with_context("/proj", "mistral-nemo");
        app.session_id = Some(SessionId(42));
        let sb = app.sidebar();
        assert_eq!(sb.logo.as_ref().unwrap().name, "CodeLoom");
        let titles: Vec<&str> = sb.sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["AGENT", "PROJECT", "FLOCK", "SESSION"]);
        assert_eq!(sb.sections[1].lines[0], "/proj");
        assert_eq!(sb.sections[3].lines[0], "session #42");
    }

    #[test]
    fn paste_inserts_normalized_text_without_sending() {
        let mut app = App::new();
        app.paste("a\r\nb\rc");
        assert_eq!(app.composer.text(), "a\nb\nc");
    }

    #[test]
    fn paste_is_ignored_without_composer_focus() {
        let mut app = App::new();
        app.toggle_focus();
        app.paste("x");
        assert_eq!(app.composer.text(), "");
    }

    #[test]
    fn turn_start_does_not_mutate_pending_prompt_count() {
        let mut app = App::new();
        app.queued = 2;
        app.apply(codeloom_core::event::LiveEvent::TurnStarted);
        assert_eq!(app.queued, 2);
    }

    #[test]
    fn token_usage_tracks_session_and_current_turn_separately() {
        let mut app = App::new();
        app.apply(LiveEvent::Usage {
            input_tokens: 10,
            output_tokens: 4,
        });
        app.apply(LiveEvent::TurnStarted);
        app.apply(LiveEvent::Usage {
            input_tokens: 7,
            output_tokens: 2,
        });
        assert_eq!((app.turn_input_tokens, app.turn_output_tokens), (7, 2));
        assert_eq!((app.input_tokens, app.output_tokens), (17, 6));
        app.apply(LiveEvent::TurnStarted);
        assert_eq!((app.turn_input_tokens, app.turn_output_tokens), (0, 0));
        assert_eq!((app.input_tokens, app.output_tokens), (17, 6));
    }

    #[test]
    fn closed_provider_phases_accumulate_until_the_turn_ends() {
        let mut app = App::new();
        app.apply(LiveEvent::StepStarted {
            step: 1,
            max_steps: 2,
        });
        app.phase_started_at = Some(Instant::now() - Duration::from_secs(2));
        app.apply(LiveEvent::ReasoningDelta {
            text: "thinking".into(),
        });
        assert_eq!(app.phase_durations.len(), 1);
        assert_eq!(app.phase_durations[0].0, "Provider");
        assert!(app.phase_durations[0].1 >= Duration::from_secs(2));

        app.apply(LiveEvent::TurnStarted);
        assert!(app.phase_durations.is_empty());
    }

    #[test]
    fn applies_file_diffs() {
        let mut app = App::new();
        app.apply(codeloom_core::event::LiveEvent::FileDiff {
            path: "src/main.rs".into(),
            added: 3,
            removed: 1,
        });
        assert_eq!(app.changes.len(), 1);
        assert_eq!(app.changes.totals(), (3, 1));
    }

    #[test]
    fn provider_error_stops_thinking_state() {
        let mut app = App::new();
        app.turn_in_flight = true;
        app.thinking = true;
        app.apply(LiveEvent::Error {
            message: "provider unavailable".into(),
        });
        assert!(!app.turn_in_flight);
        assert!(!app.thinking);
        assert_eq!(app.status, "error: provider unavailable");
    }

    /// App con el look Crush (herramientas inline y composer sin caja).
    fn crush_app() -> App {
        let mut app = App::new();
        app.theme_preset = batten::widgets::ThemePreset::Crush;
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app
    }

    /// Mensaje de rol `Tool` con el resultado de una llamada.
    fn tool_result_message(call_id: &str, content: &str, is_error: bool) -> Message {
        Message::new(
            Role::Tool,
            vec![ContentPart::ToolResult {
                call_id: call_id.into(),
                content: content.into(),
                is_error,
            }],
        )
    }

    #[test]
    fn inline_tools_appear_in_order_between_messages() {
        let mut app = crush_app();
        app.push_user("hola");
        app.push_assistant("voy a mirar");
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        app.push_assistant("hecho");
        let texts: Vec<String> = app
            .transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect();
        let tool = texts.iter().position(|t| t.starts_with("View")).unwrap();
        let before = texts.iter().position(|t| t == "voy a mirar").unwrap();
        let after = texts.iter().position(|t| t == "hecho").unwrap();
        assert!(before < tool && tool < after);
    }

    #[test]
    fn text_after_a_tool_goes_below_it() {
        let mut app = crush_app();
        app.push_assistant("uno");
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        app.push_assistant("dos");
        assert_eq!(app.transcript.entry_count(), 3);
    }

    #[test]
    fn tool_entry_updates_in_place_with_result_and_output() {
        let mut app = crush_app();
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "Bash".into(),
        });
        let count = app.transcript.entry_count();
        app.apply_message_added(tool_result_message(
            "c1",
            "linea 1\nlinea 2\nlinea 3",
            false,
        ));
        assert_eq!(app.transcript.entry_count(), count);
        let texts: Vec<String> = app
            .transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect();
        assert!(texts.iter().any(|t| t == "linea 1"));
        assert!(texts.iter().any(|t| t.contains("lines hidden")));
    }

    #[test]
    fn switching_look_hides_and_restores_tool_entries() {
        let mut app = crush_app();
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        assert!(app.transcript.iter().any(|l| l.plain().starts_with("View")));
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Batten);
        app.sync_all_tools();
        assert!(!app.transcript.iter().any(|l| l.plain().starts_with("View")));
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.sync_all_tools();
        assert!(app.transcript.iter().any(|l| l.plain().starts_with("View")));
    }

    #[test]
    fn classic_look_keeps_the_transcript_free_of_tool_lines() {
        let mut app = App::new(); // tema Batten = Classic
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        assert!(!app.transcript.iter().any(|l| l.plain().starts_with("View")));
        assert_eq!(app.tools.len(), 1);
    }

    #[test]
    fn a_tool_run_under_classic_appears_in_place_after_switching_to_crush() {
        let mut app = App::new(); // tema Batten = Classic
        app.push_user("hola");
        app.push_assistant("miro");
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        app.push_user("siguiente");
        app.theme_preset = batten::widgets::ThemePreset::Crush;
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.sync_all_tools();
        let texts: Vec<String> = app
            .transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect();
        let tool = texts.iter().position(|t| t.starts_with("View")).unwrap();
        let next = texts.iter().position(|t| t == "siguiente").unwrap();
        assert!(
            tool < next,
            "la tool debe quedar antes del mensaje siguiente"
        );
    }

    #[test]
    fn classic_look_does_not_split_the_assistant_text_around_a_tool() {
        let mut app = App::new();
        app.push_assistant("uno");
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        app.push_assistant(" dos");
        let texts: Vec<String> = app
            .transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect();
        assert!(texts.iter().any(|t| t == "uno dos"));
    }

    #[test]
    fn restoring_a_session_places_tools_in_conversation_order() {
        let mut app = crush_app();
        app.restore_messages(vec![
            Message::user("hola"),
            Message::assistant(vec![
                ContentPart::Text {
                    text: "respuesta".into(),
                },
                ContentPart::ToolCall {
                    id: "c1".into(),
                    name: "View".into(),
                    input: serde_json::json!({"path":"src/main.rs"}),
                },
            ]),
            tool_result_message("c1", "contenido", false),
            Message::assistant(vec![ContentPart::Text {
                text: "despues".into(),
            }]),
        ]);
        let texts: Vec<String> = app
            .transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect();
        let answer = texts.iter().position(|t| t == "respuesta").unwrap();
        let tool = texts.iter().position(|t| t.starts_with("View")).unwrap();
        let after = texts.iter().position(|t| t == "despues").unwrap();
        assert!(answer < tool && tool < after);
    }
}
