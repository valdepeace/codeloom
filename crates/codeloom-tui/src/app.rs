use crate::permission_flow::{dialog_for, spec_for, PermissionReply, QuestionReply};
use codeloom_core::config::{CodeLoomConfig, ProviderKind};
use codeloom_core::event::LiveEvent;
use codeloom_core::message::{ContentPart, Message, Role};
use codeloom_core::permission::{Decision, PermissionRequest};
use codeloom_core::question::{Answer, Question};
use codeloom_core::session::{SessionId, SessionInfo};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
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

/// Diálogo de preguntas abierto, con las preguntas originales y su canal de respuesta.
pub struct PendingQuestion {
    pub dialog: batten::state::QuestionDialog,
    questions: Vec<Question>,
    reply: QuestionReply,
}

/// Elemento de actividad (job o subagente) del transcript en el look Crush.
struct ActivityEntry {
    /// Clave estable: `job:{id}`, `subagent:{hijo}` o `notice:{n}`.
    key: String,
    id: batten::state::EntryId,
    /// Líneas con las que está sincronizado el elemento, esté o no visible.
    lines: Vec<batten::state::Line>,
    kind: &'static str,
    label: String,
    /// `true` si el elemento nació de un aviso y no de un evento en vivo.
    from_notice: bool,
    /// `true` si un aviso ya se asoció a este elemento en vivo.
    notice_consumed: bool,
}

/// Aviso de texto de un job o subagente terminado, ya interpretado.
enum Notice<'a> {
    Job {
        id: &'a str,
        status: batten::state::ToolStatus,
        label: &'a str,
        body: &'a str,
    },
    Subagent {
        label: &'a str,
        body: &'a str,
    },
}

/// Interpreta `[Job {id} {estado}] {etiqueta}\n{cuerpo}` y
/// `[Subagent {etiqueta} completed]\n{cuerpo}`. Devuelve `None` si el texto no
/// encaja con ninguno de los dos formatos.
fn parse_notice(text: &str) -> Option<Notice<'_>> {
    let (header, body) = text.split_once('\n').unwrap_or((text, ""));
    if let Some(rest) = header.strip_prefix("[Job ") {
        let (inside, label) = rest.split_once(']')?;
        let mut parts = inside.split_whitespace();
        let (id, status) = (parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        let status = match status {
            "completed" => batten::state::ToolStatus::Success,
            "killed" => batten::state::ToolStatus::Canceled,
            "failed" => batten::state::ToolStatus::Error,
            _ => return None,
        };
        return Some(Notice::Job {
            id,
            status,
            label: label.trim(),
            body,
        });
    }
    let rest = header.strip_prefix("[Subagent ")?;
    let (inside, _) = rest.split_once(']')?;
    let label = inside.strip_suffix(" completed")?;
    Some(Notice::Subagent { label, body })
}

/// Estado visible de un job, según el mapeo de la especificación.
fn job_tool_status(status: codeloom_core::job::JobStatus) -> batten::state::ToolStatus {
    use batten::state::ToolStatus;
    use codeloom_core::job::JobStatus;
    match status {
        JobStatus::Running | JobStatus::Stopping => ToolStatus::Running,
        JobStatus::Completed => ToolStatus::Success,
        JobStatus::Failed => ToolStatus::Error,
        JobStatus::Killed => ToolStatus::Canceled,
    }
}

/// Estado visible de un subagente, según el mapeo de la especificación.
fn subagent_tool_status(
    status: codeloom_core::session::SubagentStatus,
) -> batten::state::ToolStatus {
    use batten::state::ToolStatus;
    use codeloom_core::session::SubagentStatus;
    match status {
        SubagentStatus::Running => ToolStatus::Running,
        SubagentStatus::Completed => ToolStatus::Success,
        SubagentStatus::Failed => ToolStatus::Error,
        SubagentStatus::Killed | SubagentStatus::Interrupted => ToolStatus::Canceled,
    }
}

/// Herramienta con la que el modelo reemplaza la lista de tareas. Su llamada
/// no tiene tarjeta en el transcript: la lista se pinta como elemento propio.
const TODO_TOOL: &str = "todo_write";

/// Turnos automáticos que puede encadenar el padre tras cada mensaje del usuario.
pub const MAX_AUTO_TURNS: u32 = 8;

/// Métricas vivas de un subagente. Se alimentan solo de `LiveEvent::ChildEvent`
/// y no se persisten: al cambiar de sesión desaparecen.
#[derive(Debug, Clone, Default)]
pub struct AgentStats {
    /// Paso en curso del hijo dentro de su turno.
    pub step: usize,
    pub max_steps: usize,
    /// Herramienta que el hijo está ejecutando ahora, si hay alguna.
    pub tool: Option<String>,
    /// Tokens acumulados por el hijo; no suman al contador del turno del padre.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
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
    /// Fichero donde se guardan las preferencias de la interfaz. `None` = no persistir.
    pub prefs_path: Option<std::path::PathBuf>,
    pub matrix_rain: batten::widgets::matrix_rain::MatrixRain,
    pub animation: batten::widgets::Anim,
    pub tools: VecDeque<batten::state::ToolItem>,
    /// Entrada del transcript de cada herramienta y las líneas con las que
    /// está sincronizada (usado por el look `Inline`).
    tool_entries: HashMap<String, (batten::state::EntryId, Vec<batten::state::Line>)>,
    /// Elementos de actividad (jobs y subagentes) del transcript del look Crush.
    activity_entries: Vec<ActivityEntry>,
    /// Lista de tareas de la sesión abierta, tal como la dejó el último `todo_write`.
    pub todos: Vec<codeloom_core::todo::TodoItem>,
    /// Entrada del transcript de la lista de tareas y las líneas con las que
    /// está sincronizada.
    todo_entry: Option<(batten::state::EntryId, Vec<batten::state::Line>)>,
    /// Llamadas `todo_write` con entrada en `tool_entries`; su tarjeta nunca se pinta.
    todo_tool_calls: HashSet<String>,
    pub jobs: BTreeMap<codeloom_core::job::JobId, codeloom_core::job::JobSnapshot>,
    pub subagents: BTreeMap<i64, codeloom_core::session::SubagentDescriptor>,
    /// Hijos de la sesión abierta que esperan una respuesta de su padre (`input-required`).
    /// La clave es el id interno del hijo, como en `subagents`.
    pub awaiting_input: HashSet<i64>,
    /// Métricas de los hijos de la sesión abierta, por id interno del hijo. Solo
    /// contiene hijos de esa sesión: se filtra al recibir y se limpia al cargarla.
    pub agent_stats: HashMap<i64, AgentStats>,
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
    /// Hay un despertar pedido por un hijo (pregunta o cierre) que aún no se ha lanzado.
    pub wake_pending: bool,
    /// Despertar ya pedido al trabajador, sin turno iniciado ni `WakeIdle` de respuesta.
    pub wake_in_flight: bool,
    /// Turnos automáticos que quedan antes de exigir un mensaje del usuario.
    pub auto_turns_left: u32,
    /// Diálogo de permiso abierto, si hay.
    pub permission: Option<PendingPermission>,
    /// Peticiones esperando a que se cierre la actual.
    permission_queue: VecDeque<(PermissionRequest, PermissionReply)>,
    /// Diálogo de preguntas (`ask_user`) abierto, si hay.
    pub question: Option<PendingQuestion>,
    /// Llamadas de preguntas esperando a que se cierre la actual.
    question_queue: VecDeque<(Vec<Question>, QuestionReply)>,
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
    /// Oculta el rail derecho del layout `ContextRail` (look Crush).
    pub rail_hidden: bool,
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
    /// Pasa al siguiente preset de la rotación (`/theme`).
    pub fn cycle_theme(&mut self) {
        use batten::widgets::ThemePreset;
        let presets = ThemePreset::ALL;
        let current = presets
            .iter()
            .position(|preset| *preset == self.theme_preset)
            .unwrap_or(0);
        self.set_theme(presets[(current + 1) % presets.len()]);
    }

    /// Fija el tema, lo persiste si hay `prefs_path` y reaplica el look a lo ya dibujado.
    /// Un fallo al guardar no impide el cambio: se avisa en `slash_feedback`.
    pub fn set_theme(&mut self, preset: batten::widgets::ThemePreset) {
        self.apply_theme(preset);
        self.slash_feedback = None;
        if let Some(path) = &self.prefs_path {
            if let Err(err) = crate::prefs::save_theme(path, preset) {
                self.slash_feedback = Some(format!("Theme not saved: {err}"));
            }
        }
    }

    /// Fija el fichero de preferencias y aplica el tema guardado en él, si lo hay.
    /// No vuelve a escribir el fichero al aplicarlo.
    pub fn set_prefs_path(&mut self, path: std::path::PathBuf) {
        if let Some(preset) = crate::prefs::load_theme(&path) {
            self.apply_theme(preset);
        }
        self.prefs_path = Some(path);
    }

    fn apply_theme(&mut self, preset: batten::widgets::ThemePreset) {
        self.theme_preset = preset;
        self.theme = batten::widgets::Theme::preset(preset);
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
            prefs_path: None,
            matrix_rain: batten::widgets::matrix_rain::MatrixRain::default(),
            animation: batten::widgets::Anim::new(),
            tools: VecDeque::new(),
            tool_entries: HashMap::new(),
            activity_entries: Vec::new(),
            todos: Vec::new(),
            todo_entry: None,
            todo_tool_calls: HashSet::new(),
            jobs: BTreeMap::new(),
            subagents: BTreeMap::new(),
            awaiting_input: HashSet::new(),
            agent_stats: HashMap::new(),
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
            wake_pending: false,
            wake_in_flight: false,
            auto_turns_left: MAX_AUTO_TURNS,
            permission: None,
            permission_queue: VecDeque::new(),
            question: None,
            question_queue: VecDeque::new(),
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
            rail_hidden: false,
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

    /// Decide si debe pedirse un despertar del padre ahora. Solo lo pide con el
    /// padre ocioso: sin turno, sin diálogos, sin colas ni ajustes en curso y con
    /// presupuesto. Si el presupuesto se agotó, avisa una vez y descarta el despertar.
    pub fn take_wake(&mut self) -> bool {
        if !self.wake_pending
            || self.wake_in_flight
            || self.turn_in_flight
            || !self.pending_prompts.is_empty()
            || self.permission.is_some()
            || self.question.is_some()
            || self.queue_open
            || self.queue_paused
            || self.session_loading
            || self.session_picker_open
            || self.settings_open
            || self.settings_saving
        {
            return false;
        }
        if self.auto_turns_left == 0 {
            self.wake_pending = false;
            self.status = "auto-turns exhausted — write to continue".into();
            return false;
        }
        self.wake_pending = false;
        self.wake_in_flight = true;
        true
    }

    /// Rearma el presupuesto de turnos automáticos; lo llama el envío de cada
    /// mensaje del usuario.
    pub fn rearm_auto_turns(&mut self) {
        self.auto_turns_left = MAX_AUTO_TURNS;
    }

    /// Cancelación pedida por el usuario: un despertar pendiente deja de tener sentido.
    pub fn request_cancel(&mut self) {
        self.cancel_requested = true;
        self.wake_pending = false;
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
        self.activity_entries.clear();
        self.todos.clear();
        self.todo_entry = None;
        self.todo_tool_calls.clear();
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
                    self.user_message(&text);
                }
                Role::Assistant => {
                    for part in message.parts {
                        match part {
                            ContentPart::Text { text } => self.push_assistant(&text),
                            ContentPart::ToolCall { id, name, input } => {
                                if name == TODO_TOOL {
                                    if let Ok(items) = codeloom_core::todo::parse_todos(&input) {
                                        self.todos = items;
                                        self.sync_todos();
                                    }
                                }
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
        // Un aviso de la sesión anterior no debe despertar a la nueva.
        if self.session_id != Some(session) {
            self.wake_pending = false;
        }
        self.session_id = Some(session);
        self.subagents.clear();
        self.awaiting_input.clear();
        self.agent_stats.clear();
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

    /// Encola un lote de preguntas; si no hay diálogo abierto, lo muestra ya.
    /// Sin preguntas responde `None` de inmediato: el diálogo nunca se abre vacío.
    pub fn enqueue_questions(&mut self, questions: Vec<Question>, reply: QuestionReply) {
        if questions.is_empty() {
            let _ = reply.send(None);
            return;
        }
        self.tools_open = false;
        self.queue_open = false;
        self.queue_editing = false;
        self.help_open = false;
        self.settings_open = false;
        self.settings_editing = false;
        self.question_queue.push_back((questions, reply));
        self.open_next_question();
    }

    /// Procesa una tecla en el diálogo de preguntas abierto.
    pub fn question_key(&mut self, key: batten::state::QuestionKey) {
        use batten::state::QuestionOutcome;
        let Some(pending) = self.question.as_mut() else {
            return;
        };
        match pending.dialog.handle(key) {
            QuestionOutcome::Pending => {}
            QuestionOutcome::Done(selected) => {
                let answers = pending
                    .questions
                    .iter()
                    .zip(selected)
                    .map(|(question, selected)| Answer {
                        question: question.question.clone(),
                        selected,
                    })
                    .collect();
                self.finish_question(Some(answers));
            }
            QuestionOutcome::Cancelled => self.finish_question(None),
        }
    }

    /// Cierra todas las preguntas abiertas o encoladas respondiendo `None`.
    pub fn cancel_question(&mut self) {
        if let Some(pending) = self.question.take() {
            let _ = pending.reply.send(None);
        }
        for (_, reply) in self.question_queue.drain(..) {
            let _ = reply.send(None);
        }
    }

    /// Responde al diálogo abierto con `answers` y pasa al siguiente de la cola.
    fn finish_question(&mut self, answers: Option<Vec<Answer>>) {
        if let Some(pending) = self.question.take() {
            let _ = pending.reply.send(answers);
        }
        self.open_next_question();
    }

    fn open_next_question(&mut self) {
        if self.question.is_some() {
            return;
        }
        while let Some((questions, reply)) = self.question_queue.pop_front() {
            if reply.is_closed() {
                continue;
            }
            let specs = questions.iter().map(spec_for).collect();
            self.question = Some(PendingQuestion {
                dialog: batten::state::QuestionDialog::new(specs),
                questions,
                reply,
            });
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

    /// Tokens de entrada y salida sumados de todos los hijos de la sesión abierta.
    pub fn hierarchy_tokens(&self) -> (u64, u64) {
        self.agent_stats
            .values()
            .fold((0, 0), |(input, output), stats| {
                (
                    input.saturating_add(stats.input_tokens),
                    output.saturating_add(stats.output_tokens),
                )
            })
    }

    /// Número de subagentes de la sesión abierta que siguen en marcha.
    pub fn active_agents(&self) -> usize {
        self.subagents
            .values()
            .filter(|subagent| {
                Some(subagent.parent) == self.session_id
                    && subagent.status == codeloom_core::session::SubagentStatus::Running
            })
            .count()
    }

    /// Actualiza las métricas de un hijo de la sesión abierta. Solo toca `agent_stats`:
    /// el turno, el transcript y los contadores del padre quedan intactos.
    fn apply_child_event(&mut self, child: SessionId, event: LiveEvent) {
        // Los eventos que no se reenvían no crean métricas.
        if !event.is_child_relevant() {
            return;
        }
        let now = Instant::now();
        let stats = self
            .agent_stats
            .entry(child.0)
            .or_insert_with(|| AgentStats {
                started_at: Some(now),
                ..AgentStats::default()
            });
        match event {
            LiveEvent::TurnStarted => {
                // Un turno nuevo reinicia el progreso, pero conserva los tokens acumulados.
                stats.step = 0;
                stats.max_steps = 0;
                stats.tool = None;
                stats.started_at = Some(now);
                stats.finished_at = None;
            }
            LiveEvent::StepStarted { step, max_steps } => {
                stats.step = step;
                stats.max_steps = max_steps;
            }
            LiveEvent::ToolStarted { name, .. } => stats.tool = Some(name),
            LiveEvent::ToolFinished { .. } => stats.tool = None,
            LiveEvent::Usage {
                input_tokens,
                output_tokens,
            } => {
                stats.input_tokens = stats.input_tokens.saturating_add(u64::from(input_tokens));
                stats.output_tokens = stats.output_tokens.saturating_add(u64::from(output_tokens));
            }
            LiveEvent::TurnEnded | LiveEvent::Error { .. } => {
                stats.finished_at = Some(now);
                stats.tool = None;
            }
            _ => {}
        }
    }

    pub fn apply(&mut self, ev: LiveEvent) {
        match ev {
            LiveEvent::TurnStarted => {
                // Solo un turno despertado gasta presupuesto; un turno del usuario no.
                let woken = std::mem::take(&mut self.wake_in_flight);
                if woken {
                    self.auto_turns_left = self.auto_turns_left.saturating_sub(1);
                }
                self.turn_in_flight = true;
                self.status = if woken { "auto-turn" } else { "working" }.into();
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
                self.wake_in_flight = false;
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
                self.wake_in_flight = false;
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
                        // Un subagente que deja de estar en marcha ya no espera respuesta.
                        if subagent.status != codeloom_core::session::SubagentStatus::Running {
                            self.awaiting_input.remove(&subagent.child.0);
                        }
                    }
                }
                self.sync_job_activity(&snapshot);
                self.jobs.insert(snapshot.id.clone(), snapshot);
            }
            LiveEvent::SubagentChanged(subagent) => {
                if Some(subagent.parent) == self.session_id {
                    // Un hijo terminado ya no espera a su padre.
                    if subagent.status != codeloom_core::session::SubagentStatus::Running {
                        self.awaiting_input.remove(&subagent.child.0);
                    }
                    self.sync_subagent_activity(&subagent);
                    self.subagents.insert(subagent.child.0, subagent);
                }
            }
            LiveEvent::TodosChanged { session, items } => {
                if self.session_id == Some(session) {
                    self.todos = items;
                    self.sync_todos();
                }
            }
            LiveEvent::SubagentInputRequested { parent, child, .. } => {
                if Some(parent) == self.session_id {
                    self.awaiting_input.insert(child.0);
                    // Una pregunta del hijo pide que el padre lea el aviso.
                    self.wake_pending = true;
                }
            }
            LiveEvent::SubagentInputResolved { child, .. } => {
                self.awaiting_input.remove(&child.0);
            }
            LiveEvent::WakeIdle => self.wake_in_flight = false,
            LiveEvent::ChildEvent {
                parent,
                child,
                event,
            } => {
                if Some(parent) == self.session_id {
                    self.apply_child_event(child, *event);
                }
            }
            LiveEvent::SubagentSettled { parent, status, .. } => {
                // Un hijo que termina bien o con fallo despierta al padre; uno matado
                // por el usuario o interrumpido no.
                if Some(parent) == self.session_id
                    && matches!(
                        status,
                        codeloom_core::session::SubagentStatus::Completed
                            | codeloom_core::session::SubagentStatus::Failed
                    )
                {
                    self.wake_pending = true;
                }
            }
        }
    }

    fn apply_message_added(&mut self, message: Message) {
        if message.role == Role::User {
            self.user_message(&message.text());
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
        let todo = item.name == TODO_TOOL;
        let inline = self.tool_inline() && !todo;
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
                if todo {
                    self.todo_tool_calls.insert(call_id.to_string());
                }
                if inline {
                    self.assistant = None;
                }
            }
        }
    }

    /// Aplica la visibilidad del look actual a todas las herramientas, a los
    /// elementos de actividad y a la lista de tareas.
    pub fn sync_all_tools(&mut self) {
        let inline = self.tool_inline();
        for (call_id, (id, lines)) in &self.tool_entries {
            let shown = if inline && !self.todo_tool_calls.contains(call_id) {
                lines.clone()
            } else {
                Vec::new()
            };
            self.transcript.replace(*id, shown);
        }
        for entry in &self.activity_entries {
            let shown = if inline {
                entry.lines.clone()
            } else {
                Vec::new()
            };
            self.transcript.replace(entry.id, shown);
        }
        if let Some((id, lines)) = &self.todo_entry {
            let shown = if inline { lines.clone() } else { Vec::new() };
            self.transcript.replace(*id, shown);
        }
        let ids: Vec<String> = self.tools.iter().map(|tool| tool.call_id.clone()).collect();
        for id in ids {
            self.sync_tool(&id);
        }
    }

    /// Sincroniza la lista de tareas con su entrada del transcript. Como en
    /// `sync_tool`, la entrada se crea la primera vez (aunque la lista esté
    /// vacía) y después solo se reemplaza si cambia la caché. No se llama desde
    /// el camino de los deltas.
    fn sync_todos(&mut self) {
        use batten::state::ToolStatus;
        use codeloom_core::todo::TodoStatus;
        let items: Vec<(&str, ToolStatus)> = self
            .todos
            .iter()
            .map(|todo| {
                let status = match todo.status {
                    TodoStatus::Pending => ToolStatus::Pending,
                    TodoStatus::InProgress => ToolStatus::Running,
                    TodoStatus::Completed => ToolStatus::Success,
                };
                (todo.content.as_str(), status)
            })
            .collect();
        let lines = batten::state::todo_lines(&items);
        let inline = self.tool_inline();
        match self.todo_entry.as_mut() {
            Some((id, cache)) => {
                if *cache == lines {
                    return;
                }
                let shown = if inline { lines.clone() } else { Vec::new() };
                *cache = lines;
                self.transcript.replace(*id, shown);
            }
            None => {
                let shown = if inline { lines.clone() } else { Vec::new() };
                let id = self.transcript.push_lines(shown);
                self.todo_entry = Some((id, lines));
                if inline {
                    self.assistant = None;
                }
            }
        }
    }

    /// Mensaje del usuario. En Crush, los avisos de jobs y subagentes se pintan
    /// como elementos de actividad y no como mensajes; el resto sigue igual.
    fn user_message(&mut self, text: &str) {
        if self.tool_inline() && self.apply_notice(text) {
            return;
        }
        self.push_user(text);
    }

    /// Sincroniza un elemento de actividad con su entrada del transcript. Como
    /// `sync_tool`, solo reemplaza la entrada si sus líneas cambiaron, y no se
    /// llama desde el camino de los deltas.
    fn sync_activity(
        &mut self,
        key: &str,
        kind: &'static str,
        label: &str,
        lines: Vec<batten::state::Line>,
        from_notice: bool,
    ) {
        let inline = self.tool_inline();
        if let Some(entry) = self.activity_entries.iter_mut().find(|e| e.key == key) {
            if entry.lines == lines {
                return;
            }
            entry.label = label.to_string();
            entry.lines = lines;
            let shown = if inline {
                entry.lines.clone()
            } else {
                Vec::new()
            };
            self.transcript.replace(entry.id, shown);
            return;
        }
        // Como con las herramientas, la entrada se crea siempre para que un
        // cambio de look la encuentre en su sitio. Con el look clásico nace
        // vacía y no corta el texto del asistente.
        let shown = if inline { lines.clone() } else { Vec::new() };
        let id = self.transcript.push_lines(shown);
        self.activity_entries.push(ActivityEntry {
            key: key.to_string(),
            id,
            lines,
            kind,
            label: label.to_string(),
            from_notice,
            notice_consumed: false,
        });
        if inline {
            self.assistant = None;
        }
    }

    /// Elemento de actividad de un job de esta sesión. Los jobs de tipo
    /// `subagent` se muestran desde su propio evento.
    fn sync_job_activity(&mut self, job: &codeloom_core::job::JobSnapshot) {
        if job.kind == "subagent" || Some(job.owner) != self.session_id {
            return;
        }
        let status = job_tool_status(job.status);
        let body = if status == batten::state::ToolStatus::Running {
            None
        } else {
            job.detail
                .as_deref()
                .or_else(|| job.result.as_ref().map(|result| result.content.as_str()))
        };
        let progress = job
            .progress
            .as_ref()
            .map(|progress| (progress.fraction, progress.label.as_str()));
        let view = batten::state::ActivityView {
            kind: "job",
            label: &job.label,
            status,
            progress,
            body,
        };
        let lines = batten::state::activity_lines(&view);
        let key = format!("job:{}", job.id.0);
        self.sync_activity(&key, "job", &job.label, lines, false);
    }

    /// Elemento de actividad de un subagente de esta sesión.
    fn sync_subagent_activity(&mut self, subagent: &codeloom_core::session::SubagentDescriptor) {
        let status = subagent_tool_status(subagent.status);
        let body = if status == batten::state::ToolStatus::Running {
            None
        } else {
            subagent.result.as_deref()
        };
        let view = batten::state::ActivityView {
            kind: "subagent",
            label: &subagent.label,
            status,
            progress: None,
            body,
        };
        let lines = batten::state::activity_lines(&view);
        let key = format!("subagent:{}", subagent.child.0);
        self.sync_activity(&key, "subagent", &subagent.label, lines, false);
    }

    /// Convierte un aviso en elemento de actividad. Devuelve `false` si el texto
    /// no es un aviso reconocible. Un aviso de job no crea nada si ya existe su
    /// elemento; uno de subagente reclama el primer elemento en vivo sin consumir
    /// con su misma etiqueta, o crea uno nuevo si no hay ninguno.
    fn apply_notice(&mut self, text: &str) -> bool {
        let Some(notice) = parse_notice(text) else {
            return false;
        };
        match notice {
            Notice::Job {
                id,
                status,
                label,
                body,
            } => {
                let key = format!("job:{id}");
                if !self.activity_entries.iter().any(|e| e.key == key) {
                    self.push_notice_entry("job", status, label, body);
                }
            }
            Notice::Subagent { label, body } => {
                let live = self.activity_entries.iter_mut().find(|e| {
                    !e.from_notice && e.kind == "subagent" && e.label == label && !e.notice_consumed
                });
                match live {
                    Some(entry) => entry.notice_consumed = true,
                    None => self.push_notice_entry(
                        "subagent",
                        batten::state::ToolStatus::Success,
                        label,
                        body,
                    ),
                }
            }
        }
        true
    }

    /// Crea un elemento nacido de un aviso. Su clave usa el número de elementos
    /// existentes, que solo crece hasta el próximo `restore_messages`.
    fn push_notice_entry(
        &mut self,
        kind: &'static str,
        status: batten::state::ToolStatus,
        label: &str,
        body: &str,
    ) {
        let key = format!("notice:{}", self.activity_entries.len());
        let view = batten::state::ActivityView {
            kind,
            label,
            status,
            progress: None,
            body: Some(body),
        };
        let lines = batten::state::activity_lines(&view);
        self.sync_activity(&key, kind, label, lines, true);
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

    /// Alterna la visibilidad lateral: el rail con el layout `ContextRail` y
    /// el sidebar izquierdo con el resto. Cada uno conserva su propio estado.
    pub fn toggle_sidebar(&mut self) {
        if self.theme.look.layout == batten::widgets::LayoutStyle::ContextRail {
            self.rail_hidden = !self.rail_hidden;
        } else {
            self.sidebar_expanded = !self.sidebar_expanded;
        }
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
    use batten::state::ToolStatus;
    use codeloom_core::job::{JobId, JobProgress, JobResult, JobSnapshot, JobStatus};
    use codeloom_core::question::QuestionOption;
    use codeloom_core::session::{SubagentDescriptor, SubagentStatus};
    use serde_json::json;

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

    /// Job de prueba con etiqueta `build`, sin progreso ni resultado.
    fn job(id: &str, status: JobStatus, owner: SessionId) -> JobSnapshot {
        JobSnapshot {
            id: JobId(id.into()),
            kind: "shell".into(),
            label: "build".into(),
            owner,
            status,
            progress: None,
            output: String::new(),
            output_start: 0,
            output_end: 0,
            detail: None,
            result: None,
        }
    }

    /// Subagente de prueba (hijo `SessionId(9)`).
    fn sub(label: &str, status: SubagentStatus, parent: SessionId) -> SubagentDescriptor {
        SubagentDescriptor {
            parent,
            child: SessionId(9),
            job_id: "subagent-1".into(),
            label: label.into(),
            status,
            result: None,
            notice_delivered: false,
        }
    }

    /// Aviso que llega por la sesión 1.
    fn notice_for(text: &str) -> LiveEvent {
        LiveEvent::SessionMessageAdded {
            session: SessionId(1),
            message: Message::user(text),
        }
    }

    fn texts(app: &App) -> Vec<String> {
        app.transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect()
    }

    /// Cabeceras de actividad con su texto y su estado.
    fn heads(app: &App) -> Vec<(String, ToolStatus)> {
        app.transcript
            .iter()
            .filter_map(|l| match l.kind {
                batten::state::LineKind::ActivityHead(status) => {
                    Some((l.plain().to_string(), status))
                }
                _ => None,
            })
            .collect()
    }

    fn has_user_line(app: &App) -> bool {
        app.transcript
            .iter()
            .any(|l| l.kind == batten::state::LineKind::User)
    }

    #[test]
    fn job_activity_appears_and_updates_in_place() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        let mut running = job("build-1", JobStatus::Running, SessionId(1));
        running.progress = Some(JobProgress {
            fraction: 0.5,
            label: "building".into(),
        });
        app.apply(LiveEvent::JobChanged(running));
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(
            heads(&app),
            vec![("job  build".to_string(), ToolStatus::Running)]
        );
        assert!(texts(&app).iter().any(|t| t == "50% building"));

        let mut done = job("build-1", JobStatus::Completed, SessionId(1));
        done.result = Some(JobResult {
            content: "ok".into(),
        });
        app.apply(LiveEvent::JobChanged(done));
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(
            heads(&app),
            vec![("job  build".to_string(), ToolStatus::Success)]
        );
        let lines = texts(&app);
        assert!(lines.iter().any(|t| t == "ok"));
        assert!(!lines.iter().any(|t| t == "50% building"));
    }

    #[test]
    fn subagent_activity_shows_label_status_and_result() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            SubagentStatus::Running,
            SessionId(1),
        )));
        assert_eq!(
            heads(&app),
            vec![("subagent  review".to_string(), ToolStatus::Running)]
        );

        let mut done = sub("review", SubagentStatus::Completed, SessionId(1));
        done.result = Some("listo".into());
        app.apply(LiveEvent::SubagentChanged(done));
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(
            heads(&app),
            vec![("subagent  review".to_string(), ToolStatus::Success)]
        );
        assert!(texts(&app).iter().any(|t| t == "listo"));
    }

    #[test]
    fn foreign_and_subagent_kind_jobs_are_ignored() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::JobChanged(job(
            "build-1",
            JobStatus::Running,
            SessionId(2),
        )));
        let mut subagent_job = job("subagent-2", JobStatus::Running, SessionId(1));
        subagent_job.kind = "subagent".into();
        app.apply(LiveEvent::JobChanged(subagent_job));
        assert!(app.transcript.is_empty());
        assert_eq!(app.transcript.entry_count(), 0);
    }

    #[test]
    fn live_notice_is_suppressed_when_the_item_exists() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        let mut done = sub("review", SubagentStatus::Completed, SessionId(1));
        done.result = Some("listo".into());
        app.apply(LiveEvent::SubagentChanged(done));
        let count = app.transcript.entry_count();
        app.apply(notice_for("[Subagent review completed]\nlisto"));
        assert!(!has_user_line(&app));
        assert_eq!(app.transcript.entry_count(), count);

        let mut finished = job("build-1", JobStatus::Completed, SessionId(1));
        finished.result = Some(JobResult {
            content: "ok".into(),
        });
        app.apply(LiveEvent::JobChanged(finished));
        let count = app.transcript.entry_count();
        app.apply(notice_for("[Job build-1 completed] build\nok"));
        assert!(!has_user_line(&app));
        assert_eq!(app.transcript.entry_count(), count);
    }

    #[test]
    fn notice_before_event_does_not_duplicate() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(notice_for("[Subagent review completed]\nlisto"));
        let mut done = sub("review", SubagentStatus::Completed, SessionId(1));
        done.result = Some("listo".into());
        app.apply(LiveEvent::SubagentChanged(done));
        assert_eq!(app.transcript.entry_count(), 2);
        assert!(!has_user_line(&app));
    }

    #[test]
    fn restored_notices_become_items_not_user_messages() {
        let mut app = crush_app();
        app.restore_messages(vec![
            Message::user("hola"),
            Message::user("[Subagent review completed]\nready"),
            Message::user("[Job j1 failed] build\nboom"),
        ]);
        let user_lines: Vec<String> = app
            .transcript
            .iter()
            .filter(|l| l.kind == batten::state::LineKind::User)
            .map(|l| l.plain().to_string())
            .collect();
        assert_eq!(user_lines, vec!["hola".to_string()]);
        assert_eq!(
            heads(&app),
            vec![
                ("subagent  review".to_string(), ToolStatus::Success),
                ("job  build".to_string(), ToolStatus::Error),
            ]
        );
        let lines = texts(&app);
        let pos = |text: &str| lines.iter().position(|t| t == text).unwrap();
        assert!(pos("hola") < pos("subagent  review"));
        assert!(pos("subagent  review") < pos("job  build"));
        assert!(lines.iter().any(|t| t == "ready"));
        assert!(lines.iter().any(|t| t == "boom"));
    }

    #[test]
    fn restored_notices_with_the_same_label_each_create_an_item() {
        let mut app = crush_app();
        app.restore_messages(vec![
            Message::user("[Subagent review completed]\nuno"),
            Message::user("[Subagent review completed]\ndos"),
        ]);
        let lines = texts(&app);
        assert_eq!(heads(&app).len(), 2);
        assert!(lines.iter().any(|t| t == "uno"));
        assert!(lines.iter().any(|t| t == "dos"));
        assert!(!has_user_line(&app));
    }

    #[test]
    fn unparseable_notice_falls_back_to_a_plain_message() {
        let mut app = crush_app();
        app.apply(LiveEvent::MessageAdded(Message::user("[Job sin cerrar")));
        assert!(app
            .transcript
            .iter()
            .any(|l| l.kind == batten::state::LineKind::User && l.plain() == "[Job sin cerrar"));
        assert!(heads(&app).is_empty());
    }

    #[test]
    fn classic_look_keeps_notices_as_user_messages_and_hides_activity() {
        let mut app = App::new(); // tema Batten = Classic
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            SubagentStatus::Running,
            SessionId(1),
        )));
        app.apply(notice_for("[Subagent review completed]\nready"));
        let lines = texts(&app);
        assert!(lines.iter().any(|t| t == "[Subagent review completed]"));
        assert!(lines.iter().any(|t| t == "ready"));
        let user_count = app
            .transcript
            .iter()
            .filter(|l| l.kind == batten::state::LineKind::User)
            .count();
        assert_eq!(user_count, 2);
        assert!(heads(&app).is_empty());
    }

    #[test]
    fn switching_look_hides_and_restores_activity() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            SubagentStatus::Running,
            SessionId(1),
        )));
        assert_eq!(heads(&app).len(), 1);
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Batten);
        app.sync_all_tools();
        assert!(heads(&app).is_empty());
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.sync_all_tools();
        assert_eq!(
            heads(&app),
            vec![("subagent  review".to_string(), ToolStatus::Running)]
        );
    }

    /// Tarea de prueba con el contenido y el estado indicados.
    fn todo(
        content: &str,
        status: codeloom_core::todo::TodoStatus,
    ) -> codeloom_core::todo::TodoItem {
        codeloom_core::todo::TodoItem {
            content: content.into(),
            status,
        }
    }

    #[test]
    fn todos_event_creates_and_updates_one_element() {
        use codeloom_core::todo::TodoStatus::{Completed, Pending};
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![todo("uno", Pending), todo("dos", Pending)],
        });
        assert_eq!(app.transcript.entry_count(), 1);
        assert!(texts(&app).iter().any(|t| t == "todos  0/2"));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![todo("uno", Completed), todo("dos", Pending)],
        });
        assert_eq!(app.transcript.entry_count(), 1);
        let lines = texts(&app);
        assert!(lines.iter().any(|t| t == "todos  1/2"));
        assert!(!lines.iter().any(|t| t == "todos  0/2"));
    }

    #[test]
    fn foreign_session_todos_are_ignored() {
        use codeloom_core::todo::TodoStatus::Pending;
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(2),
            items: vec![todo("ajena", Pending)],
        });
        assert!(app.todos.is_empty());
        assert_eq!(app.transcript.entry_count(), 0);
    }

    #[test]
    fn classic_look_keeps_todos_out_of_the_transcript_but_stores_them() {
        use codeloom_core::todo::TodoStatus::InProgress;
        let mut app = App::new(); // tema Batten = Classic
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![todo("uno", InProgress)],
        });
        assert_eq!(app.todos.len(), 1);
        assert!(!texts(&app).iter().any(|t| t.starts_with("todos")));
    }

    #[test]
    fn switching_look_hides_and_restores_todos() {
        use codeloom_core::todo::TodoStatus::Pending;
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![todo("uno", Pending)],
        });
        assert!(texts(&app).iter().any(|t| t == "todos  0/1"));
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Batten);
        app.sync_all_tools();
        assert!(!texts(&app).iter().any(|t| t.starts_with("todos")));
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.sync_all_tools();
        assert!(texts(&app).iter().any(|t| t == "todos  0/1"));
    }

    #[test]
    fn todo_write_tool_has_no_transcript_card() {
        let mut app = crush_app();
        app.apply(LiveEvent::ToolStarted {
            call_id: "t1".into(),
            name: "todo_write".into(),
        });
        assert_eq!(app.tools.len(), 1);
        assert!(!texts(&app).iter().any(|t| t.starts_with("todo_write")));
        // Un cambio de look no debe devolver la tarjeta de la herramienta.
        app.sync_all_tools();
        assert!(!texts(&app).iter().any(|t| t.starts_with("todo_write")));
    }

    #[test]
    fn empty_list_clears_the_element() {
        use codeloom_core::todo::TodoStatus::Pending;
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![todo("uno", Pending), todo("dos", Pending)],
        });
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: Vec::new(),
        });
        assert!(app.todos.is_empty());
        assert!(!texts(&app).iter().any(|t| t.starts_with("todos")));
        assert_eq!(app.transcript.entry_count(), 1);
    }

    #[test]
    fn restore_rebuilds_todos_from_history() {
        let mut app = crush_app();
        app.restore_messages(vec![Message::assistant(vec![ContentPart::ToolCall {
            id: "t1".into(),
            name: "todo_write".into(),
            input: json!({"todos": [
                {"content": "a", "status": "completed"},
                {"content": "b", "status": "pending"}
            ]}),
        }])]);
        assert_eq!(app.todos.len(), 2);
        assert!(texts(&app).iter().any(|t| t == "todos  1/2"));
    }

    #[test]
    fn restore_keeps_only_the_last_todo_list() {
        let mut app = crush_app();
        app.restore_messages(vec![
            Message::assistant(vec![ContentPart::ToolCall {
                id: "t1".into(),
                name: "todo_write".into(),
                input: json!({"todos": [{"content": "a", "status": "pending"}]}),
            }]),
            Message::assistant(vec![ContentPart::ToolCall {
                id: "t2".into(),
                name: "todo_write".into(),
                input: json!({"todos": [
                    {"content": "a", "status": "completed"},
                    {"content": "b", "status": "in_progress"}
                ]}),
            }]),
        ]);
        assert_eq!(app.todos.len(), 2);
        let lines = texts(&app);
        assert_eq!(lines.iter().filter(|t| t.starts_with("todos")).count(), 1);
        assert!(lines.iter().any(|t| t == "todos  1/2"));
    }

    #[test]
    fn restore_ignores_invalid_todo_input_and_clears_previous_todos() {
        use codeloom_core::todo::TodoStatus::Pending;
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::TodosChanged {
            session: SessionId(1),
            items: vec![todo("viejo", Pending)],
        });
        app.restore_messages(vec![Message::assistant(vec![ContentPart::ToolCall {
            id: "t1".into(),
            name: "todo_write".into(),
            input: json!({"todos": "nope"}),
        }])]);
        assert!(app.todos.is_empty());
        assert!(!texts(&app).iter().any(|t| t.starts_with("todos")));
    }

    /// Pregunta de `ask_user` con opciones `labels`.
    fn ask_question(text: &str, labels: &[&str], multi_select: bool) -> Question {
        Question {
            header: "Stack".into(),
            question: text.into(),
            options: labels
                .iter()
                .map(|label| QuestionOption {
                    label: (*label).into(),
                    description: String::new(),
                })
                .collect(),
            multi_select,
        }
    }

    #[test]
    fn enqueue_questions_opens_the_first_and_queues_the_rest() {
        use tokio::sync::oneshot::error::TryRecvError;
        let mut app = App::new();
        let (tx1, mut rx1) = tokio::sync::oneshot::channel();
        let (tx2, mut rx2) = tokio::sync::oneshot::channel();
        app.enqueue_questions(vec![ask_question("Uno?", &["a", "b"], false)], tx1);
        app.enqueue_questions(vec![ask_question("Dos?", &["a", "b"], false)], tx2);
        let open = app.question.as_ref().expect("abre la primera pregunta");
        assert_eq!(open.questions[0].question, "Uno?");
        assert_eq!(open.dialog.len(), 1);
        assert_eq!(app.question_queue.len(), 1);
        assert!(matches!(rx1.try_recv(), Err(TryRecvError::Empty)));
        assert!(matches!(rx2.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn answering_sends_answers_and_opens_the_next() {
        use codeloom_core::question::Answer;
        let mut app = App::new();
        let (tx1, mut rx1) = tokio::sync::oneshot::channel();
        let (tx2, _rx2) = tokio::sync::oneshot::channel();
        app.enqueue_questions(vec![ask_question("Uno?", &["a", "b"], false)], tx1);
        app.enqueue_questions(vec![ask_question("Dos?", &["a", "b"], false)], tx2);
        app.question_key(batten::state::QuestionKey::Enter);
        assert_eq!(
            rx1.try_recv().unwrap(),
            Some(vec![Answer {
                question: "Uno?".into(),
                selected: vec!["a".into()],
            }])
        );
        let next = app.question.as_ref().expect("abre la siguiente");
        assert_eq!(next.questions[0].question, "Dos?");
    }

    #[test]
    fn esc_sends_none() {
        let mut app = App::new();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        app.enqueue_questions(vec![ask_question("Uno?", &["a", "b"], false)], tx);
        app.question_key(batten::state::QuestionKey::Esc);
        assert_eq!(rx.try_recv().unwrap(), None);
        assert!(app.question.is_none());
    }

    #[test]
    fn cancel_question_drains_everything_with_none() {
        let mut app = App::new();
        let (tx1, mut rx1) = tokio::sync::oneshot::channel();
        let (tx2, mut rx2) = tokio::sync::oneshot::channel();
        app.enqueue_questions(vec![ask_question("Uno?", &["a", "b"], false)], tx1);
        app.enqueue_questions(vec![ask_question("Dos?", &["a", "b"], false)], tx2);
        app.cancel_question();
        assert_eq!(rx1.try_recv().unwrap(), None);
        assert_eq!(rx2.try_recv().unwrap(), None);
        assert!(app.question.is_none());
        assert!(app.question_queue.is_empty());
    }

    #[test]
    fn questions_close_other_overlays() {
        let mut app = App::new();
        app.help_open = true;
        app.tools_open = true;
        app.queue_open = true;
        app.settings_open = true;
        let (tx, _rx) = tokio::sync::oneshot::channel();
        app.enqueue_questions(vec![ask_question("Uno?", &["a", "b"], false)], tx);
        assert!(!app.help_open);
        assert!(!app.tools_open);
        assert!(!app.queue_open);
        assert!(!app.settings_open);
        assert!(app.question.is_some());
    }

    #[test]
    fn empty_question_list_is_answered_with_none() {
        let mut app = App::new();
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        app.enqueue_questions(Vec::new(), tx);
        assert_eq!(rx.try_recv().unwrap(), None);
        assert!(app.question.is_none());
        assert!(app.question_queue.is_empty());
    }

    #[test]
    fn subagent_input_events_toggle_the_waiting_mark() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            SubagentStatus::Running,
            SessionId(1),
        )));

        // Preguntar marca al hijo, pero solo si su padre es la sesión abierta.
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(2),
            child: SessionId(9),
            label: "review".into(),
        });
        assert!(app.awaiting_input.is_empty());
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(1),
            child: SessionId(9),
            label: "review".into(),
        });
        assert!(app.awaiting_input.contains(&9));

        // Un hijo que sigue en marcha conserva la marca al cambiar de estado no terminal.
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            SubagentStatus::Running,
            SessionId(1),
        )));
        assert!(app.awaiting_input.contains(&9));

        // Responder retira la marca.
        app.apply(LiveEvent::SubagentInputResolved {
            parent: SessionId(1),
            child: SessionId(9),
        });
        assert!(!app.awaiting_input.contains(&9));

        // Un estado terminal retira la marca aunque no llegara la resolución.
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(1),
            child: SessionId(9),
            label: "review".into(),
        });
        assert!(app.awaiting_input.contains(&9));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            SubagentStatus::Completed,
            SessionId(1),
        )));
        assert!(!app.awaiting_input.contains(&9));
    }

    /// App en reposo con la sesión 1 abierta, que es el padre de los hijos de los tests.
    fn idle_parent_app() -> App {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app
    }

    /// Evento de cierre de un hijo (sesión 9) de la sesión 1 con el estado dado.
    fn settled(status: SubagentStatus) -> LiveEvent {
        LiveEvent::SubagentSettled {
            parent: SessionId(1),
            child: SessionId(9),
            status,
        }
    }

    #[test]
    fn wake_requested_on_child_question() {
        let mut app = idle_parent_app();
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(1),
            child: SessionId(9),
            label: "review".into(),
        });
        assert!(app.wake_pending);
        assert!(app.take_wake());
    }

    #[test]
    fn wake_requested_on_completed_or_failed_but_not_killed() {
        for status in [SubagentStatus::Completed, SubagentStatus::Failed] {
            let mut app = idle_parent_app();
            app.apply(settled(status));
            assert!(app.wake_pending, "{status:?} debe despertar al padre");
        }
        for status in [SubagentStatus::Killed, SubagentStatus::Interrupted] {
            let mut app = idle_parent_app();
            app.apply(settled(status));
            assert!(!app.wake_pending, "{status:?} no debe despertar al padre");
        }
    }

    #[test]
    fn no_wake_while_turn_in_flight_or_dialog_open_or_prompt_queued() {
        type Blocker = (&'static str, fn(&mut App));
        let blockers: [Blocker; 11] = [
            ("wake_in_flight", |app| app.wake_in_flight = true),
            ("turn_in_flight", |app| app.turn_in_flight = true),
            ("pending_prompts", |app| {
                app.pending_prompts.push_back("en cola".into())
            }),
            ("permission", |app| {
                let (reply, _rx) = tokio::sync::oneshot::channel();
                app.enqueue_permission(
                    PermissionRequest {
                        tool: "bash".into(),
                        action: "run".into(),
                        detail: "{}".into(),
                    },
                    reply,
                );
            }),
            ("question", |app| {
                let (reply, _rx) = tokio::sync::oneshot::channel();
                app.enqueue_questions(vec![ask_question("¿Cuál?", &["a", "b"], false)], reply);
            }),
            ("queue_open", |app| app.queue_open = true),
            ("queue_paused", |app| app.queue_paused = true),
            ("session_loading", |app| app.session_loading = true),
            ("session_picker_open", |app| app.session_picker_open = true),
            ("settings_open", |app| app.settings_open = true),
            ("settings_saving", |app| app.settings_saving = true),
        ];
        for (name, block) in blockers {
            let mut app = idle_parent_app();
            app.wake_pending = true;
            block(&mut app);
            assert!(!app.take_wake(), "{name} debe impedir el despertar");
            assert!(app.wake_pending, "{name}: el despertar queda pendiente");
        }
        let mut app = idle_parent_app();
        app.wake_pending = true;
        assert!(app.take_wake(), "sin bloqueos el despertar se pide");
    }

    #[test]
    fn wake_spends_budget_only_when_a_turn_starts() {
        let mut app = idle_parent_app();
        app.wake_pending = true;
        assert!(app.take_wake());
        app.apply(LiveEvent::TurnStarted);
        assert_eq!(app.auto_turns_left, MAX_AUTO_TURNS - 1);
        assert_eq!(app.status, "auto-turn");
        assert!(!app.wake_in_flight);
        app.apply(LiveEvent::TurnEnded);

        // Un turno iniciado por el usuario no gasta presupuesto ni se marca como automático.
        app.apply(LiveEvent::TurnStarted);
        assert_eq!(app.auto_turns_left, MAX_AUTO_TURNS - 1);
        assert_eq!(app.status, "working");
    }

    #[test]
    fn wake_idle_clears_in_flight_and_spends_nothing() {
        let mut app = idle_parent_app();
        app.wake_pending = true;
        assert!(app.take_wake());
        app.apply(LiveEvent::WakeIdle);
        assert!(!app.wake_in_flight);
        assert!(!app.turn_in_flight);
        assert_eq!(app.auto_turns_left, MAX_AUTO_TURNS);
    }

    #[test]
    fn wake_error_clears_in_flight() {
        let mut app = idle_parent_app();
        app.wake_pending = true;
        assert!(app.take_wake());
        app.apply(LiveEvent::Error {
            message: "proveedor caído".into(),
        });
        assert!(!app.wake_in_flight);
        assert!(!app.turn_in_flight);
        assert_eq!(app.auto_turns_left, MAX_AUTO_TURNS);
        // Tras el error, un nuevo aviso del hijo vuelve a poder despertar al padre.
        app.apply(settled(SubagentStatus::Completed));
        assert!(app.take_wake());
    }

    #[test]
    fn wake_ignores_other_sessions() {
        let mut app = idle_parent_app();
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(2),
            child: SessionId(9),
            label: "review".into(),
        });
        app.apply(LiveEvent::SubagentSettled {
            parent: SessionId(2),
            child: SessionId(9),
            status: SubagentStatus::Completed,
        });
        assert!(!app.wake_pending);

        let mut sin_sesion = App::new();
        sin_sesion.apply(settled(SubagentStatus::Completed));
        assert!(!sin_sesion.wake_pending);
    }

    #[test]
    fn opening_another_session_drops_the_pending_wake() {
        let mut app = idle_parent_app();
        app.apply(settled(SubagentStatus::Completed));
        assert!(app.wake_pending);
        app.load_session(SessionId(2), Vec::new());
        assert!(!app.wake_pending);
    }

    /// Evento de un hijo tal como lo reenvía el lanzador al bus del padre.
    fn child_event(parent: SessionId, child: i64, event: LiveEvent) -> LiveEvent {
        LiveEvent::ChildEvent {
            parent,
            child: SessionId(child),
            event: Box::new(event),
        }
    }

    /// Descriptor de un hijo con identificador y estado concretos.
    fn child_descriptor(
        child: i64,
        status: SubagentStatus,
        parent: SessionId,
    ) -> SubagentDescriptor {
        SubagentDescriptor {
            parent,
            child: SessionId(child),
            job_id: format!("subagent-{child}"),
            label: format!("hijo-{child}"),
            status,
            result: None,
            notice_delivered: false,
        }
    }

    #[test]
    fn child_events_feed_the_stats() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));

        app.apply(child_event(SessionId(1), 9, LiveEvent::TurnStarted));
        let stats = &app.agent_stats[&9];
        assert!(stats.started_at.is_some());
        assert!(stats.finished_at.is_none());

        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::StepStarted {
                step: 2,
                max_steps: 5,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::ToolStarted {
                call_id: "c1".into(),
                name: "glob".into(),
            },
        ));
        let stats = &app.agent_stats[&9];
        assert_eq!(stats.step, 2);
        assert_eq!(stats.max_steps, 5);
        assert_eq!(stats.tool.as_deref(), Some("glob"));

        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 100,
                output_tokens: 7,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::ToolFinished {
                call_id: "c1".into(),
                is_error: false,
            },
        ));
        let stats = &app.agent_stats[&9];
        assert_eq!(stats.tool, None);
        assert_eq!(stats.input_tokens, 100);
        assert_eq!(stats.output_tokens, 7);

        app.apply(child_event(SessionId(1), 9, LiveEvent::TurnEnded));
        let stats = &app.agent_stats[&9];
        assert!(stats.finished_at.is_some());
        assert_eq!(stats.tool, None);
    }

    #[test]
    fn child_stats_without_turn_start_get_a_start_time() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::StepStarted {
                step: 1,
                max_steps: 4,
            },
        ));
        let stats = &app.agent_stats[&9];
        assert!(stats.started_at.is_some());
        assert_eq!(stats.step, 1);
    }

    #[test]
    fn child_usage_accumulates_and_does_not_touch_the_parents_counters() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.turn_input_tokens = 40;
        app.turn_output_tokens = 3;

        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 10,
                output_tokens: 2,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 5,
                output_tokens: 1,
            },
        ));

        let stats = &app.agent_stats[&9];
        assert_eq!((stats.input_tokens, stats.output_tokens), (15, 3));
        assert_eq!(app.turn_input_tokens, 40);
        assert_eq!(app.turn_output_tokens, 3);
        assert_eq!(app.input_tokens, 0);
        assert_eq!(app.output_tokens, 0);
    }

    #[test]
    fn child_turn_restart_keeps_the_token_totals() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 10,
                output_tokens: 2,
            },
        ));
        app.apply(child_event(SessionId(1), 9, LiveEvent::TurnEnded));
        app.apply(child_event(SessionId(1), 9, LiveEvent::TurnStarted));

        let stats = &app.agent_stats[&9];
        assert_eq!((stats.input_tokens, stats.output_tokens), (10, 2));
        assert!(stats.started_at.is_some());
        assert!(stats.finished_at.is_none());
    }

    #[test]
    fn child_events_of_another_session_are_ignored() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(child_event(SessionId(2), 9, LiveEvent::TurnStarted));
        app.apply(child_event(
            SessionId(2),
            9,
            LiveEvent::Usage {
                input_tokens: 10,
                output_tokens: 2,
            },
        ));
        assert!(app.agent_stats.is_empty());
        assert_eq!(app.hierarchy_tokens(), (0, 0));
    }

    #[test]
    fn child_events_do_not_touch_transcript_or_status() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        // El turno del padre está en marcha cuando llegan los eventos de los hijos.
        app.apply(LiveEvent::TurnStarted);
        app.apply(LiveEvent::Delta {
            text: "respuesta".into(),
        });
        let status = app.status.clone();
        let in_flight = app.turn_in_flight;
        let entries = app.transcript.entry_count();
        let phase = app.phase.clone();

        app.apply(child_event(SessionId(1), 9, LiveEvent::TurnStarted));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::ToolStarted {
                call_id: "c1".into(),
                name: "glob".into(),
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::ToolFinished {
                call_id: "c1".into(),
                is_error: true,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Error {
                message: "fallo del hijo".into(),
            },
        ));
        app.apply(child_event(SessionId(1), 9, LiveEvent::TurnEnded));

        assert_eq!(app.status, status);
        assert_eq!(app.turn_in_flight, in_flight);
        assert_eq!(app.transcript.entry_count(), entries);
        assert_eq!(app.phase, phase);
    }

    #[test]
    fn hierarchy_tokens_sums_all_children() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 100,
                output_tokens: 10,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            10,
            LiveEvent::Usage {
                input_tokens: 20,
                output_tokens: 5,
            },
        ));
        // Un hijo de otra sesión no entra en el total.
        app.apply(child_event(
            SessionId(2),
            11,
            LiveEvent::Usage {
                input_tokens: 1000,
                output_tokens: 1000,
            },
        ));
        assert_eq!(app.hierarchy_tokens(), (120, 15));
    }

    #[test]
    fn active_agents_counts_only_running_children_of_the_open_session() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.subagents.insert(
            9,
            child_descriptor(9, SubagentStatus::Running, SessionId(1)),
        );
        app.subagents.insert(
            10,
            child_descriptor(10, SubagentStatus::Completed, SessionId(1)),
        );
        app.subagents.insert(
            11,
            child_descriptor(11, SubagentStatus::Running, SessionId(1)),
        );
        app.subagents.insert(
            12,
            child_descriptor(12, SubagentStatus::Running, SessionId(2)),
        );
        assert_eq!(app.active_agents(), 2);
    }

    #[test]
    fn stats_are_cleared_on_session_load() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 10,
                output_tokens: 2,
            },
        ));
        assert!(!app.agent_stats.is_empty());
        app.load_session(SessionId(2), Vec::new());
        assert!(app.agent_stats.is_empty());
    }

    #[test]
    fn set_theme_changes_the_look() {
        use batten::widgets::{Look, ThemePreset};
        let mut app = App::new();
        app.set_theme(ThemePreset::Crush);
        assert_eq!(app.theme_preset, ThemePreset::Crush);
        assert_eq!(app.theme.look, Look::crush());
        app.set_theme(ThemePreset::Batten);
        assert_eq!(app.theme.look, Look::classic());
    }

    #[test]
    fn set_prefs_path_applies_the_saved_theme_without_rewriting() {
        use batten::widgets::ThemePreset;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        // Espacios finales: si el fichero se reescribiera, desaparecerían.
        let original = "theme = \"crush\"   \n";
        std::fs::write(&path, original).unwrap();
        let mut app = App::new();
        app.set_prefs_path(path.clone());
        assert_eq!(app.theme_preset, ThemePreset::Crush);
        assert!(app.slash_feedback.is_none());
        assert_eq!(app.prefs_path.as_deref(), Some(path.as_path()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn set_theme_persists_when_a_path_is_set() {
        use batten::widgets::ThemePreset;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        let mut app = App::new();
        app.set_prefs_path(path.clone());
        app.set_theme(ThemePreset::MastraCode);
        assert_eq!(
            crate::prefs::load_theme(&path),
            Some(ThemePreset::MastraCode)
        );
    }

    #[test]
    fn a_save_error_does_not_break_the_theme_change() {
        use batten::widgets::ThemePreset;
        let dir = tempfile::tempdir().unwrap();
        // Un fichero normal como «directorio» padre: la escritura siempre falla.
        let blocker = dir.path().join("bloqueo");
        std::fs::write(&blocker, "x").unwrap();
        let mut app = App::new();
        app.set_prefs_path(blocker.join("ui.toml"));
        app.set_theme(ThemePreset::Crush);
        assert_eq!(app.theme_preset, ThemePreset::Crush);
        assert!(app
            .slash_feedback
            .as_deref()
            .unwrap()
            .starts_with("Theme not saved"));
    }
}
