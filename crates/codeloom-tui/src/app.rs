use crate::permission_flow::{dialog_for, spec_for, PermissionReply, QuestionReply};
use crate::rail::{agent_state, format_tokens};
use crate::tool_facts::tool_facts;
use batten::widgets::composer::hit_index;
use codeloom_core::catalog::{Catalog, CatalogModel, CatalogProvider};
use codeloom_core::config::{CodeLoomConfig, ProviderKind};
use codeloom_core::event::LiveEvent;
use codeloom_core::message::{ContentPart, Message, Role};
use codeloom_core::permission::{Decision, PermissionRequest};
use codeloom_core::question::{Answer, Question};
use codeloom_core::session::{SessionId, SessionInfo, UsageRecord};
use codeloom_core::task::{TaskState, TaskStatus};
use ratatui::layout::Rect;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Dónde va el teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Composer,
    Transcript,
    /// Panel lateral izquierdo (look Classic, si se pinta).
    Sidebar,
    /// Panel «Agent» de métricas; «Token usage» comparte esta ventana.
    Agent,
    /// Panel «Activity» de herramientas (look Classic).
    Activity,
    /// Rail derecho del look Crush.
    Rail,
}

impl Focus {
    /// Ventanas laterales: no escriben, y una tecla imprimible las devuelve al composer.
    pub fn is_side_pane(self) -> bool {
        matches!(
            self,
            Focus::Sidebar | Focus::Agent | Focus::Activity | Focus::Rail
        )
    }
}

/// `true` si la celda (x, y) cae dentro del rectángulo (bordes derecho e inferior excluidos).
fn rect_contains(area: Rect, x: u16, y: u16) -> bool {
    x >= area.x
        && y >= area.y
        && u32::from(x) < u32::from(area.x) + u32::from(area.width)
        && u32::from(y) < u32::from(area.y) + u32::from(area.height)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionChoice {
    Resume(SessionId),
    New,
}

/// Aviso que se muestra al cerrar `/settings` con cambios sin guardar.
const SETTINGS_DISCARD_WARNING: &str = "Unsaved changes · Esc again to discard";

/// Catálogo de muestra (formato de models.dev, recortado) para las pruebas del selector.
#[cfg(test)]
pub(crate) const PICKER_SAMPLE_JSON: &str = r#"{
    "openrouter": {"id": "openrouter", "name": "OpenRouter", "npm": "@openrouter/ai-sdk-provider",
        "api": "https://openrouter.ai/api/v1", "env": ["OPENROUTER_API_KEY"],
        "models": {"qwen": {"id": "qwen", "name": "Qwen Coder", "tool_call": true,
            "release_date": "2025-06-01", "limit": {"context": 262000, "output": 8000},
            "cost": {"input": 0.3, "output": 1.2}}}},
    "anthropic": {"id": "anthropic", "name": "Anthropic", "npm": "@ai-sdk/anthropic",
        "env": ["ANTHROPIC_API_KEY"],
        "models": {
            "claude-opus": {"id": "claude-opus", "name": "Claude Opus", "tool_call": true,
                "release_date": "2025-05-01", "limit": {"context": 200000, "output": 32000}},
            "claude-sonnet": {"id": "claude-sonnet", "name": "Claude Sonnet", "tool_call": true,
                "release_date": "2025-09-01", "limit": {"context": 200000, "output": 64000},
                "cost": {"input": 3, "output": 15}}}}
}"#;

/// Catálogo de muestra ya construido, listo para `App::set_catalog`.
#[cfg(test)]
pub(crate) fn picker_sample_catalog() -> Arc<Catalog> {
    Arc::new(Catalog::from_json(PICKER_SAMPLE_JSON).expect("el JSON de muestra es válido"))
}

/// Catálogo de muestra con modelos que razonan (uno admite tres niveles, otro dos) y
/// uno que no razona, para las pruebas del campo «Effort».
#[cfg(test)]
const REASONING_SAMPLE_JSON: &str = r#"{
    "anthropic": {"id": "anthropic", "name": "Anthropic", "npm": "@ai-sdk/anthropic",
        "api": "https://api.anthropic.com/v1", "env": ["ANTHROPIC_API_KEY"],
        "models": {
            "claude-think": {"id": "claude-think", "name": "Claude Think", "tool_call": true,
                "reasoning": true,
                "reasoning_options": [{"type": "effort", "values": ["low", "medium", "high"]}]},
            "claude-mid": {"id": "claude-mid", "name": "Claude Mid", "tool_call": true,
                "reasoning": true,
                "reasoning_options": [{"type": "effort", "values": ["low", "medium"]}]},
            "claude-plain": {"id": "claude-plain", "name": "Claude Plain", "tool_call": true}}}
}"#;

/// Catálogo de muestra con razonamiento ya construido, listo para `App::set_catalog`.
#[cfg(test)]
pub(crate) fn reasoning_sample_catalog() -> Arc<Catalog> {
    Arc::new(Catalog::from_json(REASONING_SAMPLE_JSON).expect("el JSON de muestra es válido"))
}

/// Catálogo de muestra con precios: `acme` publica su `api` con barra final; `priced` tiene
/// ambos precios, `half` solo el de entrada y `plain` ninguno.
#[cfg(test)]
const PRICED_SAMPLE_JSON: &str = r#"{
    "acme": {"id": "acme", "name": "Acme", "npm": "@ai-sdk/openai-compatible",
        "api": "https://acme.example/v1/", "env": ["ACME_KEY"],
        "models": {
            "priced": {"id": "priced", "name": "Priced", "tool_call": true,
                "cost": {"input": 3, "output": 15}},
            "half": {"id": "half", "name": "Half", "tool_call": true,
                "cost": {"input": 3}},
            "plain": {"id": "plain", "name": "Plain", "tool_call": true}}}
}"#;

/// Catálogo de muestra con precios ya construido, listo para `App::set_catalog`.
#[cfg(test)]
pub(crate) fn priced_sample_catalog() -> Arc<Catalog> {
    Arc::new(Catalog::from_json(PRICED_SAMPLE_JSON).expect("el JSON de muestra es válido"))
}

/// Posición del campo «Effort» en `/settings`. Solo existe si el modelo admite niveles;
/// sin él, «Save» ocupa esta misma posición.
pub const SETTINGS_EFFORT_FIELD: usize = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderSettings {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    pub api_key_env: String,
    /// Nivel de esfuerzo de razonamiento; `None` equivale a «default».
    pub effort: Option<String>,
}

/// Selector abierto sobre un campo de `/settings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPicker {
    /// Proveedores del catálogo.
    Provider,
    /// Modelos del proveedor del catálogo cuya URL base coincide con la actual.
    Model,
}

/// Fila de un selector: identificador que se guarda, etiqueta visible y detalle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub id: String,
    pub label: String,
    pub detail: String,
}

/// Forma comparable de una URL base: sin espacios, sin barras finales y en minúsculas.
fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_ascii_lowercase()
}

/// Resumen de un modelo para el selector: `ctx 200k · $3/$15` (coste por millón de tokens).
fn model_detail(model: &CatalogModel) -> String {
    let context = model.context.map_or_else(|| "-".to_string(), format_tokens);
    let cost = match (model.input_cost, model.output_cost) {
        (Some(input), Some(output)) => format!("${input}/${output}"),
        _ => "-".to_string(),
    };
    format!("ctx {context} · {cost}")
}

impl From<&CodeLoomConfig> for ProviderSettings {
    fn from(config: &CodeLoomConfig) -> Self {
        Self {
            kind: config.provider.kind,
            model: config.model.clone(),
            base_url: config.provider.base_url.clone(),
            api_key_env: config.provider.api_key_env.clone(),
            effort: config.effort.clone(),
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
fn subagent_tool_status(state: TaskState) -> batten::state::ToolStatus {
    use batten::state::ToolStatus;
    // Varios estados de A2A comparten aspecto: la cabecera nombra el estado exacto, para que
    // esta agrupación no lo oculte (ver `sync_subagent_activity`).
    match state {
        TaskState::Submitted
        | TaskState::Working
        | TaskState::InputRequired
        | TaskState::AuthRequired => ToolStatus::Running,
        TaskState::Completed => ToolStatus::Success,
        TaskState::Failed | TaskState::Rejected => ToolStatus::Error,
        TaskState::Canceled => ToolStatus::Canceled,
    }
}

/// Etiqueta del agente de un hijo: `[front]`; con origen de proyecto o remoto,
/// `[front·project]` o `[front·remote]`. Es política local de CodeLoom, no del protocolo A2A.
/// Devuelve `None` si el descriptor no conoce su agente, para no pintar corchetes vacíos.
pub(crate) fn agent_tag(subagent: &codeloom_core::session::SubagentDescriptor) -> Option<String> {
    let agente = subagent
        .agent
        .as_deref()
        .filter(|nombre| !nombre.is_empty())?;
    let nivel = match subagent.agent_origin.as_deref() {
        Some(origen) if origen.starts_with("project:") => "·project",
        Some(origen) if origen.starts_with("remote:") => "·remote",
        _ => "",
    };
    Some(format!("[{agente}{nivel}]"))
}

/// Formato de un recuento por estado A2A exacto: `working 1, input-required 2`.
/// Recibe el recuento ya filtrado (solo los estados con cuenta mayor que cero).
pub fn formato_recuento(recuento: &[(TaskState, usize)]) -> String {
    recuento
        .iter()
        .map(|(estado, cuenta)| format!("{} {cuenta}", agent_state(*estado).label()))
        .collect::<Vec<_>>()
        .join(", ")
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

/// Una fila del popup de agentes: lo que se muestra de un agente del registro.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    pub name: String,
    /// Origen en texto estable (`built-in`, `global:<ruta>`, `project:<ruta>`, `remote:<url>`).
    pub origin: String,
    pub approval: codeloom_core::registry::Approval,
    /// Modelo y esfuerzo propios del perfil, o la herencia.
    pub execution: String,
    pub description: String,
    /// Prompt de sistema del perfil: lo que la persona usuaria revisa antes de aprobar.
    pub body: String,
}

impl AgentRow {
    fn from_entry(
        entrada: &codeloom_core::registry::AgentEntry,
        instantanea: &codeloom_core::registry::RegistrySnapshot,
    ) -> Self {
        use codeloom_core::registry::Execution;
        let (execution, body) = match &entrada.execution {
            Execution::Local(local) => (
                format!(
                    "model {} / effort {}",
                    local.model.as_deref().unwrap_or("inherits"),
                    local.effort.as_deref().unwrap_or("inherits")
                ),
                local.system_prompt(),
            ),
            Execution::Remote => ("remote".to_string(), String::new()),
        };
        AgentRow {
            name: entrada.name.clone(),
            origin: entrada.origin.as_text(),
            approval: instantanea
                .approval(&entrada.name)
                .unwrap_or(codeloom_core::registry::Approval::Pending),
            execution,
            description: entrada.card.description.trim().to_string(),
            body,
        }
    }
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
    /// Captura del ratón (`/mouse`). Apagada, la terminal conserva su selección nativa.
    pub mouse_capture: bool,
    /// Hay un cambio de `mouse_capture` pendiente de aplicar al terminal.
    pub mouse_capture_dirty: bool,
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
    /// Métricas de los hijos de la sesión abierta, por id interno del hijo. Solo
    /// contiene hijos de esa sesión: se filtra al recibir y se limpia al cargarla.
    pub agent_stats: HashMap<i64, AgentStats>,
    /// Subagente resaltado en el rail (clave de `subagents`), si hay. Solo se mueve
    /// con el foco en el rail y se descarta si el hijo deja de existir.
    pub rail_selected: Option<i64>,
    /// Sesiones desde las que se entró en un subagente, de la más antigua a la más reciente.
    pub parent_stack: Vec<SessionId>,
    /// Navegación en curso (`open_child` o `go_parent`): la siguiente carga de sesión
    /// conserva la pila en vez de vaciarla.
    nav_keep_stack: bool,
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
    /// Ficheros editados por sesión (padre e hijos): sesión → ruta → (añadidas, quitadas).
    /// La última edición de cada ruta gana. Alimenta el rail y el panel de cambios al
    /// cargar una sesión; se vacía cuando la carga no es una navegación padre↔hijo.
    pub files_by_session: HashMap<i64, BTreeMap<String, (usize, usize)>>,
    /// Scroll del transcript (sigue el final salvo que el usuario suba).
    pub viewport: batten::state::Viewport,
    pub layout: batten::widgets::transcript::Layout,
    pub focus: Focus,
    /// Ventanas del último fotograma, en orden de pintado. `render` lo reescribe
    /// en cada frame reutilizando el mismo `Vec`.
    pub pane_rects: Vec<(Focus, Rect)>,
    /// Área del composer del último fotograma, si se pintó.
    pub composer_area: Option<Rect>,
    /// Ancla (en grafemas) del arrastre de ratón en curso dentro del composer.
    /// `None` si no hay arrastre; cualquier tecla o pegado lo anula.
    pub mouse_drag: Option<usize>,
    /// Líneas seleccionadas del transcript, si hay.
    pub selection: Option<batten::state::Selection>,
    /// Selección de texto por caracteres del transcript, si la hay. Excluyente con
    /// `selection` y con la del composer: crear una anula las otras.
    pub text_sel: Option<batten::state::TextSelection>,
    /// Ancho de texto del transcript cuando se creó `text_sel`. Si el diseño cambia
    /// de ancho, las columnas guardadas dejan de valer y la selección se descarta.
    pub text_sel_width: u16,
    /// Arrastre de ratón en curso sobre el transcript.
    pub text_drag: bool,
    /// Área en la que se pintó el texto del transcript en el último fotograma.
    pub transcript_area: Option<Rect>,
    /// Bloque activo del transcript: índice de entrada (`Transcript::entry`). Se
    /// navega con `[` y `]` y se fija con un clic; `None` si no hay ninguno.
    pub block: Option<usize>,
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
    /// Causas `(hijo, motivo)` de los despertares pendientes desde el último turno despertado.
    /// Motivos: `"completed"`, `"failed"` o `"asks input"`. Solo son memoria de la interfaz.
    pub wake_causes: Vec<(i64, &'static str)>,
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
    /// Catálogo de proveedores y modelos de models.dev. Vacío hasta que llega la carga en segundo plano.
    pub catalog: Arc<Catalog>,
    /// Registro de agentes, para `/agents`. `None` si el binario no lo cablea.
    pub agent_registry: Option<Arc<codeloom_core::registry::AgentRegistry>>,
    /// Apuntes de consumo de la sesión abierta: los persistidos al abrirla más los vivos.
    pub usage_records: Vec<UsageRecord>,
    /// Copia de los ajustes al abrir el diálogo y tras guardar con éxito.
    /// Sirve para saber si hay cambios sin guardar.
    pub settings_baseline: Option<ProviderSettings>,
    /// `true` tras avisar de cambios sin guardar; una segunda `Esc` descarta.
    pub settings_warned: bool,
    pub settings_open: bool,
    pub settings_field: usize,
    pub settings_editing: bool,
    pub settings_editor: batten::state::Composer,
    pub settings_save_requested: bool,
    pub settings_saving: bool,
    pub settings_feedback: Option<String>,
    /// Selector abierto sobre el campo de proveedor o de modelo, si lo hay.
    pub settings_picker: Option<SettingsPicker>,
    /// Texto con el que se filtra el selector abierto.
    pub picker_query: String,
    /// Índice de la fila resaltada dentro de `picker_items()`.
    pub picker_selected: usize,
    pub help_open: bool,
    /// Popup de `/agents` abierto, con el informe que mostraba el registro al abrirlo.
    pub agents_open: bool,
    pub agent_rows: Vec<AgentRow>,
    pub agents_selected: usize,
    /// Resultado de la última aprobación o revocación, para pintarlo en el popup.
    pub agents_status: Option<String>,
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
        // Se aplica sin marcar `mouse_capture_dirty`: el arranque ya usa este valor.
        if let Some(on) = crate::prefs::load_mouse(&path) {
            self.mouse_capture = on;
        }
        self.prefs_path = Some(path);
    }

    /// Fija la captura del ratón, la marca como pendiente de aplicar y la persiste
    /// si hay `prefs_path`. Un fallo al guardar no impide el cambio: se avisa en
    /// `slash_feedback`.
    pub fn set_mouse_capture(&mut self, on: bool) {
        self.mouse_capture = on;
        self.mouse_capture_dirty = true;
        self.slash_feedback = None;
        if let Some(path) = &self.prefs_path {
            if let Err(err) = crate::prefs::save_mouse(path, on) {
                self.slash_feedback = Some(format!("Mouse setting not saved: {err}"));
            }
        }
    }

    /// Invierte la captura del ratón (`/mouse`).
    pub fn toggle_mouse_capture(&mut self) {
        self.set_mouse_capture(!self.mouse_capture);
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
            mouse_capture: true,
            mouse_capture_dirty: false,
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
            agent_stats: HashMap::new(),
            rail_selected: None,
            parent_stack: Vec::new(),
            nav_keep_stack: false,
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
            files_by_session: HashMap::new(),
            viewport: batten::state::Viewport::new(),
            layout: batten::widgets::transcript::Layout::new(),
            focus: Focus::Composer,
            pane_rects: Vec::new(),
            composer_area: None,
            mouse_drag: None,
            selection: None,
            text_sel: None,
            text_sel_width: 0,
            text_drag: false,
            transcript_area: None,
            block: None,
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
            wake_causes: Vec::new(),
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
            catalog: Arc::new(Catalog::default()),
            agent_registry: None,
            usage_records: Vec::new(),
            settings_baseline: None,
            settings_warned: false,
            settings_open: false,
            settings_field: 0,
            settings_editing: false,
            settings_editor: batten::state::Composer::new(),
            settings_save_requested: false,
            settings_saving: false,
            settings_feedback: None,
            settings_picker: None,
            picker_query: String::new(),
            picker_selected: 0,
            help_open: false,
            agents_open: false,
            agent_rows: Vec::new(),
            agents_selected: 0,
            agents_status: None,
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
            || self.agents_open
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
            // Las causas descartadas no deben contarse en el próximo despertar.
            self.wake_causes.clear();
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
        self.wake_causes.clear();
    }

    /// Línea de interfaz que explica el último despertar del padre. Con varias causas
    /// pendientes muestra la más reciente y cuántas más quedan. Un hijo que no se conoce
    /// se nombra por su identificador de agente.
    fn wake_cause_line(&self) -> Option<String> {
        let (child, motivo) = *self.wake_causes.last()?;
        let etiqueta = self
            .subagents
            .get(&child)
            .map(|subagent| subagent.label.clone())
            .unwrap_or_else(|| format!("agent #{child}"));
        let mas = self.wake_causes.len() - 1;
        let sufijo = if mas > 0 {
            format!(" (+{mas} more)")
        } else {
            String::new()
        };
        Some(format!(
            "⟳ auto-turn · woken by «{etiqueta}» ({motivo}){sufijo}"
        ))
    }

    pub fn set_provider_settings(&mut self, settings: ProviderSettings) {
        self.model = settings.model.clone();
        self.settings_baseline = Some(settings.clone());
        self.provider_settings = Some(settings);
    }

    /// Sustituye el catálogo por otro recién llegado de la carga en segundo plano.
    pub fn set_catalog(&mut self, catalog: Arc<Catalog>) {
        self.catalog = catalog;
    }

    /// Entrega a la interfaz el registro de agentes que consulta `/agents`.
    pub fn set_agent_registry(&mut self, registry: Arc<codeloom_core::registry::AgentRegistry>) {
        self.agent_registry = Some(registry);
    }

    /// Abre el popup de `/agents` con los agentes de la instantánea vigente del registro.
    pub fn show_agents(&mut self) {
        self.agents_selected = 0;
        self.agents_status = None;
        self.refresh_agent_rows();
        self.agents_open = true;
    }

    /// Reconstruye las filas desde la instantánea vigente, conservando la selección acotada.
    /// El estado de aprobación se consulta en vivo, así que aprobar se refleja sin recargar.
    fn refresh_agent_rows(&mut self) {
        self.agent_rows = match &self.agent_registry {
            Some(registro) => {
                let instantanea = registro.snapshot();
                instantanea
                    .entries()
                    .into_iter()
                    .map(|entrada| AgentRow::from_entry(entrada, &instantanea))
                    .collect()
            }
            None => Vec::new(),
        };
        self.agents_selected = self
            .agents_selected
            .min(self.agent_rows.len().saturating_sub(1));
    }

    /// Mueve la selección del popup de agentes (`delta` negativo sube), sin dar la vuelta.
    pub fn agents_move(&mut self, delta: isize) {
        let tope = self.agent_rows.len().saturating_sub(1);
        self.agents_selected = self.agents_selected.saturating_add_signed(delta).min(tope);
    }

    /// Aprueba (`aprobar`) o revoca el agente seleccionado y anota el resultado.
    pub fn agents_decide(&mut self, aprobar: bool) {
        let (Some(registro), Some(fila)) = (
            self.agent_registry.clone(),
            self.agent_rows.get(self.agents_selected),
        ) else {
            return;
        };
        let nombre = fila.name.clone();
        let resultado = if aprobar {
            registro.approve(&nombre)
        } else {
            registro.revoke(&nombre)
        };
        self.agents_status = Some(match resultado {
            Ok(()) if aprobar => format!("Approved `{nombre}`"),
            Ok(()) => format!("Revoked `{nombre}`"),
            Err(error) => format!("Not changed: {error}"),
        });
        self.refresh_agent_rows();
    }

    pub fn open_settings(&mut self) {
        if self.provider_settings.is_some() {
            self.settings_open = true;
            self.settings_feedback = None;
            self.settings_warned = false;
            self.close_picker();
            self.settings_baseline = self.provider_settings.clone();
        }
    }

    /// `true` si los ajustes en pantalla difieren de la copia guardada.
    pub fn settings_dirty(&self) -> bool {
        match (&self.provider_settings, &self.settings_baseline) {
            (Some(current), Some(baseline)) => current != baseline,
            _ => false,
        }
    }

    /// `true` si el campo `index` (0 proveedor, 1 modelo, 2 URL, 3 variable de clave,
    /// 4 esfuerzo) difiere de la copia guardada. El esfuerzo solo cuenta mientras el
    /// campo existe: un cambio que el modelo actual no puede mostrar no se marca.
    pub fn settings_field_changed(&self, index: usize) -> bool {
        match (&self.provider_settings, &self.settings_baseline) {
            (Some(current), Some(baseline)) => match index {
                0 => current.kind != baseline.kind,
                1 => current.model != baseline.model,
                2 => current.base_url != baseline.base_url,
                3 => current.api_key_env != baseline.api_key_env,
                SETTINGS_EFFORT_FIELD => {
                    self.settings_effort_visible() && current.effort != baseline.effort
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Número de campos con cambios sin guardar.
    pub fn settings_change_count(&self) -> usize {
        (0..=SETTINGS_EFFORT_FIELD)
            .filter(|&index| self.settings_field_changed(index))
            .count()
    }

    /// Niveles de esfuerzo que admite el modelo de los ajustes, según el catálogo (proveedor
    /// con la misma URL base y modelo con el mismo identificador). Vacío si no hay catálogo,
    /// si el modelo no razona o si no publica niveles con nombre.
    pub fn effort_options(&self) -> Vec<String> {
        let Some(settings) = &self.provider_settings else {
            return Vec::new();
        };
        let Some(provider) = self.model_catalog_provider() else {
            return Vec::new();
        };
        provider
            .models
            .iter()
            .find(|model| model.id == settings.model)
            .and_then(|model| model.reasoning.as_ref())
            .map(|reasoning| reasoning.efforts.clone())
            .unwrap_or_default()
    }

    /// `true` si `/settings` muestra el campo «Effort» (el modelo admite niveles).
    pub fn settings_effort_visible(&self) -> bool {
        !self.effort_options().is_empty()
    }

    /// Posición de «Save»: justo tras «Effort» si ese campo existe, o en su lugar si no.
    pub fn settings_save_field(&self) -> usize {
        if self.settings_effort_visible() {
            SETTINGS_EFFORT_FIELD + 1
        } else {
            SETTINGS_EFFORT_FIELD
        }
    }

    /// Avanza (o retrocede) el nivel de esfuerzo entre «default» y los niveles del modelo,
    /// dando la vuelta al final. Sin niveles disponibles no hace nada.
    pub fn cycle_effort(&mut self, forward: bool) {
        let options = self.effort_options();
        if options.is_empty() {
            return;
        }
        let Some(settings) = self.provider_settings.as_mut() else {
            return;
        };
        // La posición 0 es «default»; la i + 1 corresponde al nivel `options[i]`.
        let len = options.len() + 1;
        let current = settings
            .effort
            .as_ref()
            .and_then(|level| options.iter().position(|option| option == level))
            .map_or(0, |index| index + 1);
        let next = if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
        settings.effort = next.checked_sub(1).map(|index| options[index].clone());
    }

    /// Tras cambiar el modelo o la URL, descarta el nivel si el modelo nuevo no lo admite
    /// y lo deja anotado en `settings_feedback`.
    fn reconcile_effort(&mut self) {
        let options = self.effort_options();
        let unsupported = self.provider_settings.as_ref().is_some_and(|settings| {
            settings
                .effort
                .as_ref()
                .is_some_and(|level| !options.contains(level))
        });
        if !unsupported {
            return;
        }
        if let Some(settings) = self.provider_settings.as_mut() {
            settings.effort = None;
        }
        self.settings_feedback = Some("Effort reset: not supported by this model".into());
    }

    /// Proveedor del catálogo cuya URL base coincide con la de los ajustes actuales.
    fn model_catalog_provider(&self) -> Option<&CatalogProvider> {
        let current = self.provider_settings.as_ref()?;
        let wanted = normalize_base_url(&current.base_url);
        self.catalog
            .providers
            .iter()
            .find(|provider| normalize_base_url(&provider.base_url) == wanted)
    }

    /// Abre el selector del campo indicado. Sin catálogo, o sin proveedor del catálogo
    /// que coincida con la URL actual, no lo abre y deja el aviso en `settings_feedback`.
    pub fn open_picker(&mut self, picker: SettingsPicker) {
        if self.catalog.is_empty() {
            self.settings_feedback = Some("Catalog not loaded yet; type it by hand".into());
            return;
        }
        if picker == SettingsPicker::Model && self.model_catalog_provider().is_none() {
            self.settings_feedback = Some("No catalog provider matches the current URL".into());
            return;
        }
        self.settings_picker = Some(picker);
        self.picker_query.clear();
        self.picker_selected = 0;
        self.settings_feedback = None;
    }

    /// Cierra el selector sin tocar los valores ni el diálogo.
    pub fn close_picker(&mut self) {
        self.settings_picker = None;
        self.picker_query.clear();
        self.picker_selected = 0;
    }

    /// Filas del selector abierto, filtradas por `picker_query` (sin distinguir mayúsculas,
    /// sobre el identificador y el nombre). Sin selector abierto, lista vacía.
    pub fn picker_items(&self) -> Vec<PickerItem> {
        let query = self.picker_query.trim().to_lowercase();
        let all: Vec<PickerItem> = match self.settings_picker {
            None => return Vec::new(),
            Some(SettingsPicker::Provider) => self
                .catalog
                .providers
                .iter()
                .map(|provider| PickerItem {
                    id: provider.id.clone(),
                    label: provider.name.clone(),
                    detail: provider.base_url.clone(),
                })
                .collect(),
            Some(SettingsPicker::Model) => self
                .model_catalog_provider()
                .map(|provider| {
                    provider
                        .models
                        .iter()
                        .map(|model| PickerItem {
                            id: model.id.clone(),
                            label: if model.name.is_empty() {
                                model.id.clone()
                            } else {
                                model.name.clone()
                            },
                            detail: model_detail(model),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        all.into_iter()
            .filter(|item| {
                query.is_empty()
                    || item.id.to_lowercase().contains(&query)
                    || item.label.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Mueve la fila resaltada del selector, sin salirse de la lista filtrada.
    pub fn picker_move(&mut self, down: bool) {
        let last = self.picker_items().len().saturating_sub(1);
        self.picker_selected = if down {
            (self.picker_selected + 1).min(last)
        } else {
            self.picker_selected.saturating_sub(1)
        };
    }

    /// Aplica la fila resaltada y cierra el selector. Elegir un proveedor rellena tipo,
    /// URL base y variable de clave (si el catálogo la trae; si no, se conserva la
    /// actual). Elegir un modelo rellena el identificador. Lo rellenado cuenta como
    /// cambio sin guardar.
    pub fn picker_choose(&mut self) {
        let Some(picker) = self.settings_picker else {
            return;
        };
        let items = self.picker_items();
        // Un filtro sin coincidencias en la lista de modelos se acepta tal cual:
        // permite fijar a mano un modelo que el catálogo aún no conoce.
        if picker == SettingsPicker::Model && items.is_empty() && !self.picker_query.is_empty() {
            let typed = self.picker_query.trim().to_string();
            if let Some(settings) = self.provider_settings.as_mut() {
                settings.model = typed;
            }
            self.close_picker();
            self.settings_feedback = None;
            self.reconcile_effort();
            return;
        }
        if let (Some(item), Some(settings)) = (
            items.get(self.picker_selected),
            self.provider_settings.as_mut(),
        ) {
            match picker {
                SettingsPicker::Provider => {
                    if let Some(provider) = self.catalog.provider(&item.id) {
                        settings.kind = provider.kind;
                        settings.base_url = provider.base_url.clone();
                        if !provider.api_key_env.is_empty() {
                            settings.api_key_env = provider.api_key_env.clone();
                        }
                    }
                }
                SettingsPicker::Model => settings.model = item.id.clone(),
            }
        }
        self.close_picker();
        self.settings_feedback = None;
        self.reconcile_effort();
        // Tras elegir proveedor se pasa directamente a sus modelos.
        if picker == SettingsPicker::Provider {
            self.settings_field = 1;
            self.open_picker(SettingsPicker::Model);
        }
    }

    /// Cierra `/settings`. Con cambios sin guardar avisa primero: devuelve `false`
    /// y deja el aviso en `settings_feedback`. Con el aviso ya dado descarta los
    /// cambios, restaura la copia guardada y devuelve `true`.
    pub fn close_settings(&mut self) -> bool {
        if self.settings_dirty() && !self.settings_warned {
            self.settings_warned = true;
            self.settings_feedback = Some(SETTINGS_DISCARD_WARNING.into());
            return false;
        }
        if self.settings_dirty() {
            self.provider_settings = self.settings_baseline.clone();
        }
        self.close_picker();
        self.settings_open = false;
        self.settings_warned = false;
        self.settings_feedback = None;
        true
    }

    /// Anula el aviso de «cambios sin guardar» tras cualquier acción distinta de `Esc`.
    pub fn settings_disarm(&mut self) {
        if self.settings_warned {
            self.settings_warned = false;
            if self.settings_feedback.as_deref() == Some(SETTINGS_DISCARD_WARNING) {
                self.settings_feedback = None;
            }
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
        self.settings_disarm();
        let value = self.settings_editor.text().to_string();
        if let Some(settings) = self.provider_settings.as_mut() {
            match self.settings_field {
                1 => settings.model = value,
                2 => settings.base_url = value,
                3 => settings.api_key_env = value,
                _ => {}
            }
        }
        if matches!(self.settings_field, 1 | 2) {
            self.reconcile_effort();
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
                    self.settings_baseline = Some(settings.clone());
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
                                let facts = tool_facts(&name, &input);
                                let tool = self.tool_mut(&id, &name);
                                tool.set_summary(input.to_string());
                                tool.facts = facts;
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
            self.wake_causes.clear();
        }
        self.session_id = Some(session);
        // Una navegación con `open_child` o `go_parent` conserva la pila de padres;
        // cualquier otra carga (p. ej. el selector de sesiones) la vacía.
        let navegacion = std::mem::take(&mut self.nav_keep_stack);
        if !navegacion {
            self.parent_stack.clear();
            self.files_by_session.clear();
        }
        self.rail_selected = None;
        self.subagents.clear();
        self.agent_stats.clear();
        // Los apuntes son de la sesión anterior; la carga posterior repone los de esta.
        self.usage_records.clear();
        self.session_selected = 0;
        self.session_query.clear();
        self.session_searching = false;
        self.session_choice = None;
        self.session_loading = false;
        self.session_picker_open = false;
        self.session_picker_return_to_chat = false;
        // Una selección del transcript pertenece a la sesión anterior.
        self.text_sel = None;
        self.text_drag = false;
        // El bloque activo pertenece también a la sesión anterior.
        self.block = None;
        self.restore_messages(messages);
        // `restore_messages` vacía el panel de cambios; se repone con los ficheros de esta sesión.
        if let Some(files) = self.files_by_session.get(&session.0) {
            for (path, &(added, removed)) in files {
                self.changes.upsert(batten::state::FileChange {
                    path: path.clone(),
                    added,
                    removed,
                });
            }
        }
    }

    /// Sustituye los apuntes de la sesión abierta por los que devuelve la base de datos. Una
    /// carga de otra sesión se ignora, para no mezclar totales.
    pub fn load_usage(&mut self, session: SessionId, records: Vec<UsageRecord>) {
        if self.session_id == Some(session) {
            self.usage_records = records;
        }
    }

    /// Añade un apunte vivo con el modelo y los precios vigentes en este instante, atribuido a
    /// `session` (el padre para su turno, el hijo para lo suyo). Sin sesión abierta se usa
    /// `SessionId(0)` como marcador: la recarga de la sesión sustituye esta lista por la de la
    /// base de datos, así que no duplica ni deja marcadores persistidos.
    fn record_live_usage(&mut self, session: SessionId, input_tokens: u32, output_tokens: u32) {
        let (input_price, output_price) = self.model_prices().unzip();
        let created_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0);
        self.usage_records.push(UsageRecord {
            session,
            model: self.model.clone(),
            input_tokens: u64::from(input_tokens),
            output_tokens: u64::from(output_tokens),
            input_price,
            output_price,
            created_at_ms,
        });
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
            // Un hijo que ya no existe no puede seguir marcado en el rail.
            if self
                .rail_selected
                .is_some_and(|selected| !self.subagents.contains_key(&selected))
            {
                self.rail_selected = None;
            }
        }
    }

    /// Mueve la selección del rail entre los hijos de la sesión abierta, sin dar la
    /// vuelta. Sin hijos la quita; sin selección (o con una que ya no existe) elige el primero.
    pub fn rail_move(&mut self, delta: isize) {
        let children: Vec<i64> = self.subagents.keys().copied().collect();
        let Some(&first) = children.first() else {
            self.rail_selected = None;
            return;
        };
        let current = self
            .rail_selected
            .and_then(|selected| children.iter().position(|&child| child == selected));
        self.rail_selected = Some(match current {
            None => first,
            Some(index) => {
                let last = children.len() - 1;
                children[index.saturating_add_signed(delta).min(last)]
            }
        });
    }

    /// Entra en el hijo `child` del rail: apila la sesión actual y pide cargar el hijo.
    /// No tiene efecto (y devuelve `false`) con un turno en curso, sin sesión abierta
    /// o si `child` no es hijo de la sesión abierta.
    pub fn open_child(&mut self, child: i64) -> bool {
        let Some(current) = self.session_id else {
            return false;
        };
        if self.turn_in_flight || !self.subagents.contains_key(&child) {
            return false;
        }
        self.parent_stack.push(current);
        self.nav_keep_stack = true;
        self.session_choice = Some(SessionChoice::Resume(SessionId(child)));
        true
    }

    /// Vuelve a la sesión desde la que se entró en el subagente actual. No tiene efecto
    /// (y devuelve `false`) con un turno en curso o con la pila de padres vacía.
    pub fn go_parent(&mut self) -> bool {
        if self.turn_in_flight {
            return false;
        }
        let Some(parent) = self.parent_stack.pop() else {
            return false;
        };
        self.nav_keep_stack = true;
        self.session_choice = Some(SessionChoice::Resume(parent));
        true
    }

    pub fn session_load_failed(&mut self) {
        self.session_loading = false;
        // Una navegación fallida no debe dejar la pila «encendida» para la siguiente carga.
        self.nav_keep_stack = false;
        self.session_picker_open = !self.sessions.is_empty();
        self.session_picker_return_to_chat = false;
    }

    /// Alterna el foco entre composer y transcript. Desde un panel lateral, Tab
    /// devuelve el foco al composer.
    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Composer => Focus::Transcript,
            Focus::Transcript => Focus::Composer,
            _ => Focus::Composer,
        };
        if self.focus == Focus::Transcript {
            self.ensure_block();
        }
    }

    /// Con el chrome `Flat`, si el transcript no tiene bloque activo y hay entradas,
    /// activa el último. Así siempre hay un bloque marcado al entrar en él. Con
    /// Classic, o sin entradas, no hace nada.
    fn ensure_block(&mut self) {
        if self.theme.look.chrome != batten::widgets::ChromeStyle::Flat || self.block.is_some() {
            return;
        }
        if let Some(last) = self.transcript.entry_count().checked_sub(1) {
            self.block = Some(last);
        }
    }

    /// Ventana bajo la celda (x, y) del último fotograma. El composer tiene
    /// prioridad sobre el transcript, cuya área lo contiene.
    pub fn pane_at(&self, x: u16, y: u16) -> Option<Focus> {
        if self
            .composer_area
            .is_some_and(|area| rect_contains(area, x, y))
        {
            return Some(Focus::Composer);
        }
        self.pane_rects
            .iter()
            .find(|(_, area)| rect_contains(*area, x, y))
            .map(|(pane, _)| *pane)
    }

    /// Copia la selección del composer al portapapeles (lo vuelca el bucle con
    /// OSC 52) y avisa del número de caracteres. Devuelve `false` sin selección.
    pub fn copy_composer_selection(&mut self) -> bool {
        let Some(text) = self.composer.selected_text() else {
            return false;
        };
        let chars = text.chars().count();
        self.clipboard = Some(text.to_owned());
        self.slash_feedback = Some(format!("Copied {chars} chars"));
        true
    }

    /// Pulsación del botón izquierdo. Dirige el foco a la ventana pulsada; en el
    /// composer coloca el cursor y queda listo para arrastrar. Fuera de cualquier
    /// ventana no hace nada, y en otra ventana anula el arrastre en curso.
    pub fn mouse_down(&mut self, x: u16, y: u16) {
        self.text_drag = false;
        let Some(pane) = self.pane_at(x, y) else {
            self.mouse_drag = None;
            return;
        };
        self.focus = pane;
        if pane == Focus::Transcript {
            self.mouse_drag = None;
            self.transcript_mouse_down(x, y);
            self.ensure_block();
            return;
        }
        let area = self.composer_area.filter(|_| pane == Focus::Composer);
        let Some(area) = area else {
            self.mouse_drag = None;
            return;
        };
        // El composer y el transcript no comparten selección.
        self.text_sel = None;
        match hit_index(&self.theme, &self.composer, area, x, y) {
            Some(index) => {
                self.composer.set_cursor(index);
                self.mouse_drag = Some(index);
            }
            None => self.mouse_drag = None,
        }
    }

    /// Arrastre con el botón izquierdo: selecciona desde el ancla hasta la celda,
    /// acotada al área del composer. Sin arrastre en curso, no hace nada.
    pub fn mouse_drag_to(&mut self, x: u16, y: u16) {
        if self.text_drag {
            self.transcript_mouse_drag_to(x, y);
            return;
        }
        let (Some(anchor), Some(area)) = (self.mouse_drag, self.composer_area) else {
            return;
        };
        if area.is_empty() {
            return;
        }
        let x = x.clamp(area.x, area.right() - 1);
        let y = y.clamp(area.y, area.bottom() - 1);
        if let Some(head) = hit_index(&self.theme, &self.composer, area, x, y) {
            self.composer.select_range(anchor, head);
        }
    }

    /// Soltar el botón. Solo si hubo un arrastre que empezó en el composer se
    /// copia la selección: un clic simple no copia nada.
    pub fn mouse_up(&mut self) {
        if self.text_drag {
            self.transcript_mouse_up();
            return;
        }
        if self.mouse_drag.take().is_some() {
            self.copy_composer_selection();
        }
    }

    /// Pulsación dentro del área de texto del transcript: enfoca el transcript y
    /// empieza una selección de texto en la celda pulsada. Fuera de esa área solo
    /// enfoca. Anula la selección de líneas y la del composer.
    pub fn transcript_mouse_down(&mut self, x: u16, y: u16) {
        self.text_sel = None;
        self.text_drag = false;
        self.selection = None;
        self.composer.clear_selection();
        let Some(area) = self
            .transcript_area
            .filter(|area| rect_contains(*area, x, y))
        else {
            return;
        };
        let Some(at) =
            batten::widgets::transcript::hit_cell(&self.layout, &self.viewport, area, x, y)
        else {
            return;
        };
        self.text_sel = Some(batten::state::TextSelection::new(at));
        self.text_sel_width = self.layout.width();
        self.text_drag = true;
        // El clic activa el bloque pulsado; la selección de texto no cambia.
        self.block = self.entry_at_row(at.row);
    }

    /// Entrada del transcript que ocupa la fila de documento `row`, si la hay.
    fn entry_at_row(&self, row: usize) -> Option<usize> {
        let (line, _) = self.layout.line_at_row(row)?;
        self.transcript.locate(line).map(|(entry, _)| entry)
    }

    /// Bloque activo acotado a las entradas existentes: `None` sin transcript.
    pub fn block_index(&self) -> Option<usize> {
        let count = self.transcript.entry_count();
        self.block.filter(|_| count > 0).map(|b| b.min(count - 1))
    }

    /// Bloque anterior (`[`). Sin bloque activo, activa el último.
    pub fn block_prev(&mut self) {
        let Some(last) = self.transcript.entry_count().checked_sub(1) else {
            self.block = None;
            return;
        };
        let target = self
            .block_index()
            .map_or(last, |current| current.saturating_sub(1));
        self.focus_block(target, true);
    }

    /// Bloque siguiente (`]`). Sin bloque activo, activa el último.
    pub fn block_next(&mut self) {
        let Some(last) = self.transcript.entry_count().checked_sub(1) else {
            self.block = None;
            return;
        };
        let target = self
            .block_index()
            .map_or(last, |current| (current + 1).min(last));
        self.focus_block(target, false);
    }

    /// Activa la entrada `entry` y desplaza lo mínimo para verla entera; si no cabe,
    /// deja visible su comienzo. Al retroceder suelta el seguimiento del final.
    fn focus_block(&mut self, entry: usize, backwards: bool) {
        self.block = Some(entry);
        if backwards {
            // `scroll_up(0)` solo suelta el seguimiento del final sin mover la vista.
            self.viewport.scroll_up(0);
        }
        let first = self.transcript.entry_start(entry);
        let start = self.layout.row_of(first);
        let height = self
            .layout
            .row_of(first + self.transcript.entry_lines(entry).len())
            .saturating_sub(start);
        let view = self
            .transcript_area
            .map_or(1, |area| usize::from(area.height).max(1));
        self.viewport
            .ensure_visible(start, start + height.min(view));
    }

    /// Texto plano del bloque activo: las líneas de la entrada unidas por `\n`.
    pub fn block_text(&self) -> Option<String> {
        let entry = self.block_index()?;
        let text = self
            .transcript
            .entry_lines(entry)
            .iter()
            .map(|line| line.plain())
            .collect::<Vec<_>>()
            .join("\n");
        Some(text)
    }

    /// Copia el bloque activo al portapapeles y avisa del número de caracteres.
    /// Devuelve `false` si no hay bloque activo.
    pub fn copy_block(&mut self) -> bool {
        let Some(text) = self.block_text() else {
            return false;
        };
        let chars = text.chars().count();
        self.clipboard = Some(text);
        self.slash_feedback = Some(format!("Copied {chars} chars"));
        true
    }

    /// Arrastre sobre el transcript: mueve la cabeza de la selección. Si el puntero
    /// sale por arriba o por abajo del área, el transcript se desplaza una fila por
    /// evento hacia ese lado y la cabeza queda acotada a la fila visible.
    pub fn transcript_mouse_drag_to(&mut self, x: u16, y: u16) {
        if !self.text_drag {
            return;
        }
        let (Some(area), Some(mut sel)) = (
            self.transcript_area.filter(|area| !area.is_empty()),
            self.text_sel,
        ) else {
            return;
        };
        if y <= area.y {
            self.viewport.scroll_up(1);
        } else if y.saturating_add(1) >= area.bottom() {
            self.viewport.scroll_down(1);
        }
        if let Some(head) =
            batten::widgets::transcript::hit_cell(&self.layout, &self.viewport, area, x, y)
        {
            sel.set_head(head);
            self.text_sel = Some(sel);
        }
    }

    /// Soltar tras un arrastre sobre el transcript: copia si la selección no está
    /// vacía. Un clic simple (selección vacía) la descarta. La selección queda
    /// resaltada tras copiar.
    pub fn transcript_mouse_up(&mut self) {
        self.text_drag = false;
        if self.text_sel.is_some_and(|sel| sel.is_empty()) {
            self.text_sel = None;
            return;
        }
        self.copy_transcript_selection();
    }

    /// Copia la selección de texto del transcript al portapapeles y avisa del número
    /// de caracteres. Devuelve `false` si no hay selección no vacía.
    pub fn copy_transcript_selection(&mut self) -> bool {
        let Some(sel) = self.text_sel.filter(|sel| !sel.is_empty()) else {
            return false;
        };
        let text = batten::widgets::transcript::selected_text(
            &self.transcript,
            &self.layout,
            &self.theme,
            &sel,
        );
        let chars = text.chars().count();
        self.clipboard = Some(text);
        self.slash_feedback = Some(format!("Copied {chars} chars"));
        true
    }

    /// Encola una petición; si no hay diálogo abierto, la muestra ya.
    pub fn enqueue_permission(&mut self, req: PermissionRequest, reply: PermissionReply) {
        self.tools_open = false;
        self.queue_open = false;
        self.queue_editing = false;
        self.help_open = false;
        self.agents_open = false;
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
        self.agents_open = false;
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

    /// Precios `(entrada, salida)` por millón de tokens del modelo actual, buscados en el
    /// catálogo. `None` si falta el catálogo, el proveedor, el modelo o alguno de los dos precios.
    pub fn model_prices(&self) -> Option<(f64, f64)> {
        let provider = self.model_catalog_provider()?;
        let model = provider
            .models
            .iter()
            .find(|model| model.id == self.model)?;
        Some((model.input_cost?, model.output_cost?))
    }

    /// Coste de la sesión abierta: suma cada apunte del libro (propios e hijos) a su precio
    /// congelado en el momento. Devuelve `(total, parcial)`, donde `parcial` indica que algún
    /// apunte no tenía precio y el total queda por debajo de lo real. `None` sin apuntes.
    pub fn estimated_cost(&self) -> Option<(f64, bool)> {
        if self.usage_records.is_empty() {
            return None;
        }
        let mut total = 0.0;
        let mut partial = false;
        for record in &self.usage_records {
            match record.cost() {
                Some(cost) => total += cost,
                None => partial = true,
            }
        }
        Some((total, partial))
    }

    /// Coste de un hijo: suma los apuntes que llevan su sesión, con el mismo criterio que
    /// `estimated_cost` (devuelve `(total, parcial)`). `None` si no tiene apuntes o si ninguno
    /// lleva precio, para no mostrar un `≈$0` que no informa de nada.
    pub fn child_cost(&self, child: SessionId) -> Option<(f64, bool)> {
        let mut total = 0.0;
        let mut priced = false;
        let mut partial = false;
        for record in self.usage_records.iter().filter(|r| r.session == child) {
            match record.cost() {
                Some(cost) => {
                    total += cost;
                    priced = true;
                }
                None => partial = true,
            }
        }
        priced.then_some((total, partial))
    }

    /// Recuento de subagentes de la sesión abierta por estado A2A exacto, en el orden de
    /// `TaskState::ALL`. Solo aparecen los estados con cuenta mayor que cero.
    pub fn subagent_state_counts(&self) -> Vec<(TaskState, usize)> {
        TaskState::ALL
            .into_iter()
            .filter_map(|estado| {
                let cuenta = self
                    .subagents
                    .values()
                    .filter(|s| Some(s.parent) == self.session_id && s.status.state == estado)
                    .count();
                (cuenta > 0).then_some((estado, cuenta))
            })
            .collect()
    }

    /// Resumen de los subagentes en curso. Sin ninguno, `0 subagents running` sin paréntesis
    /// (no hay nada que ocultar); con alguno, el desglose por estado A2A entre paréntesis.
    pub fn running_summary(&self) -> String {
        let activos: Vec<(TaskState, usize)> = self
            .subagent_state_counts()
            .into_iter()
            .filter(|(estado, _)| !estado.is_terminal())
            .collect();
        let total = self.active_agents();
        if total == 0 {
            "0 subagents running".to_owned()
        } else {
            format!("{total} subagents running ({})", formato_recuento(&activos))
        }
    }

    /// Sufijo de los subagentes terminados, ` (subagents: completed 2, failed 1)`; vacío si
    /// no hay ninguno.
    pub fn finished_summary(&self) -> String {
        let terminados: Vec<(TaskState, usize)> = self
            .subagent_state_counts()
            .into_iter()
            .filter(|(estado, _)| estado.is_terminal())
            .collect();
        if terminados.is_empty() {
            String::new()
        } else {
            format!(" (subagents: {})", formato_recuento(&terminados))
        }
    }

    /// Número de subagentes de la sesión abierta que siguen en marcha, es decir, los que
    /// no están en un estado terminal de A2A.
    pub fn active_agents(&self) -> usize {
        self.subagents
            .values()
            .filter(|subagent| {
                Some(subagent.parent) == self.session_id && !subagent.status.state.is_terminal()
            })
            .count()
    }

    /// Número de ficheros distintos que ha editado `child` en la sesión abierta.
    pub fn child_file_count(&self, child: SessionId) -> usize {
        self.files_by_session.get(&child.0).map_or(0, BTreeMap::len)
    }

    /// Anota una edición en `files_by_session` para `session`. La última edición de una
    /// ruta sustituye a la anterior, igual que `changes.upsert`.
    fn record_file(&mut self, session: i64, path: &str, added: usize, removed: usize) {
        self.files_by_session
            .entry(session)
            .or_default()
            .insert(path.to_string(), (added, removed));
    }

    /// Actualiza las métricas de un hijo de la sesión abierta. Solo toca `agent_stats`:
    /// el turno, el transcript y los contadores del padre quedan intactos.
    fn apply_child_event(&mut self, child: SessionId, event: LiveEvent) {
        // Los eventos que no se reenvían no crean métricas.
        if !event.is_child_relevant() {
            return;
        }
        // Un fichero editado por el hijo se anota en su sesión, sin tocar el panel del padre
        // ni crear métricas nuevas para el hijo.
        if let LiveEvent::FileDiff {
            path,
            added,
            removed,
        } = event
        {
            self.record_file(child.0, &path, added, removed);
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
                    // La causa del despertar se apunta en el transcript como línea de interfaz;
                    // no pasa al historial del modelo ni se persiste.
                    if let Some(line) = self.wake_cause_line() {
                        self.transcript.push_entry(
                            batten::state::LineKind::System,
                            batten::state::Format::Plain,
                            line,
                        );
                    }
                    self.wake_causes.clear();
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
                let session = self.session_id.unwrap_or(SessionId(0));
                self.record_live_usage(session, input_tokens, output_tokens);
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
            } => {
                if let Some(session) = self.session_id {
                    self.record_file(session.0, &path, added, removed);
                }
                self.changes.upsert(batten::state::FileChange {
                    path,
                    added,
                    removed,
                });
            }
            LiveEvent::JobChanged(snapshot) => {
                if snapshot.kind == "subagent" {
                    if let Some(subagent) = self
                        .subagents
                        .values_mut()
                        .find(|subagent| subagent.job_id == snapshot.id.0)
                    {
                        // Solo el cierre del job fija el estado: mientras sigue en marcha, la tarea
                        // conserva su estado real (`working`, `input-required`, `auth-required`...).
                        // Si el cierre no cambia el estado, se conserva el `TaskStatus` tal cual.
                        let state = TaskState::from(snapshot.status);
                        if state.is_terminal() && subagent.status.state != state {
                            subagent.status = TaskStatus::new(state);
                        }
                    }
                }
                self.sync_job_activity(&snapshot);
                self.jobs.insert(snapshot.id.clone(), snapshot);
            }
            LiveEvent::SubagentChanged(subagent) => {
                if Some(subagent.parent) == self.session_id {
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
                // La espera la fija el estado de la tarea (`SubagentChanged`, `input-required`);
                // aquí solo se despierta al padre.
                if Some(parent) == self.session_id {
                    // Una pregunta del hijo pide que el padre lea el aviso.
                    self.wake_pending = true;
                    self.wake_causes.push((child.0, "asks input"));
                }
            }
            // La vuelta a `working` llega como `SubagentChanged`: no hay nada que mantener aquí.
            LiveEvent::SubagentInputResolved { .. } => {}
            LiveEvent::WakeIdle => self.wake_in_flight = false,
            LiveEvent::ChildEvent {
                parent,
                child,
                event,
            } => {
                if Some(parent) == self.session_id {
                    // El consumo de un hijo se atribuye al modelo actual de la interfaz, pero
                    // con la sesión del hijo para que su coste se pueda separar.
                    if let LiveEvent::Usage {
                        input_tokens,
                        output_tokens,
                    } = event.as_ref()
                    {
                        self.record_live_usage(child, *input_tokens, *output_tokens);
                    }
                    self.apply_child_event(child, *event);
                }
            }
            LiveEvent::SubagentSettled {
                parent,
                child,
                status,
            } => {
                // Un hijo que termina bien o con fallo despierta al padre; uno matado
                // por el usuario o sin terminar no.
                let motivo = match status {
                    codeloom_core::task::TaskState::Completed => Some("completed"),
                    codeloom_core::task::TaskState::Failed => Some("failed"),
                    _ => None,
                };
                if let Some(motivo) = motivo.filter(|_| Some(parent) == self.session_id) {
                    self.wake_pending = true;
                    self.wake_causes.push((child.0, motivo));
                }
            }
            // El registro de agentes cambió. La interfaz aún no lo muestra: mostrarlo no es
            // objetivo del registro (spec del registro de agentes, «Eventos y observabilidad»).
            LiveEvent::AgentRegistryChanged { .. } => {}
        }
    }

    fn apply_message_added(&mut self, message: Message) {
        if message.role == Role::User {
            self.user_message(&message.text());
        } else {
            for part in message.parts {
                match part {
                    ContentPart::ToolCall { id, name, input } => {
                        let facts = tool_facts(&name, &input);
                        let tool = self.tool_mut(&id, &name);
                        tool.set_summary(input.to_string());
                        tool.facts = facts;
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

    /// Líneas que una herramienta o un elemento de actividad deja en el transcript. Con el
    /// look `Inline`, todas; con el compacto, solo la cabecera. El transcript es el diario de
    /// toda iteración: ningún look puede dejar una llamada sin rastro.
    fn shown_lines(&self, lines: &[batten::state::Line]) -> Vec<batten::state::Line> {
        if self.tool_inline() {
            lines.to_vec()
        } else {
            lines.iter().take(1).cloned().collect()
        }
    }

    /// Sincroniza las líneas de una herramienta con su entrada del transcript.
    /// Solo reemplaza la entrada si sus líneas cambiaron: no se llama desde el
    /// camino de los deltas, que se ejecuta por cada token.
    pub fn sync_tool(&mut self, call_id: &str) {
        let Some(item) = self.tools.iter().find(|tool| tool.call_id == call_id) else {
            return;
        };
        let lines = batten::state::tool_lines_with(item, &item.facts);
        let todo = item.name == TODO_TOOL;
        // La lista de tareas tiene su propia entrada: la llamada que la escribe no se repite.
        let shown = if todo {
            Vec::new()
        } else {
            self.shown_lines(&lines)
        };
        match self.tool_entries.get_mut(call_id) {
            Some((id, cache)) => {
                if *cache == lines {
                    return;
                }
                *cache = lines;
                self.transcript.replace(*id, shown);
            }
            // La entrada se crea siempre, en su sitio cronológico. Si deja alguna línea visible,
            // corta el texto del asistente: lo que diga después va en una entrada nueva, debajo.
            None => {
                let visible = !shown.is_empty();
                let id = self.transcript.push_lines(shown);
                self.tool_entries.insert(call_id.to_string(), (id, lines));
                if todo {
                    self.todo_tool_calls.insert(call_id.to_string());
                }
                if visible {
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
            let shown = if self.todo_tool_calls.contains(call_id) {
                Vec::new()
            } else {
                self.shown_lines(lines)
            };
            self.transcript.replace(*id, shown);
        }
        for entry in &self.activity_entries {
            let shown = self.shown_lines(&entry.lines);
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
        let shown = self.shown_lines(&lines);
        if let Some(entry) = self.activity_entries.iter_mut().find(|e| e.key == key) {
            if entry.lines == lines {
                return;
            }
            entry.label = label.to_string();
            entry.lines = lines;
            self.transcript.replace(entry.id, shown);
            return;
        }
        // Como con las herramientas, la entrada se crea en su sitio cronológico y, al dejar
        // una línea visible, corta el texto del asistente.
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
        self.assistant = None;
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
        let state = subagent.status.state;
        let status = subagent_tool_status(state);
        // El cuerpo es el texto de los artefactos del resultado, el mismo que antes daba `result`.
        let resultado = if status == batten::state::ToolStatus::Running {
            None
        } else {
            subagent.result_text()
        };
        let body = resultado.as_deref();
        // Regla de trazabilidad: el estado A2A exacto va entre paréntesis tras la etiqueta, de modo
        // que el aspecto agrupado de `ToolStatus` nunca sustituya al estado real.
        // La etiqueta del agente va tras el nombre; el estado A2A sigue siendo lo último.
        let titulo = match agent_tag(subagent) {
            Some(etiqueta) => format!(
                "{} {etiqueta} ({})",
                subagent.label,
                agent_state(state).label()
            ),
            None => format!("{} ({})", subagent.label, agent_state(state).label()),
        };
        let view = batten::state::ActivityView {
            kind: "subagent",
            label: &titulo,
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
                    // El aviso ya dice que el hijo terminó: el estado exacto es `completed`.
                    None => self.push_notice_entry(
                        "subagent",
                        batten::state::ToolStatus::Success,
                        &format!("{label} (completed)"),
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
        let mut flock = vec![
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
        ];
        // Regla de trazabilidad: el resumen de subagentes lleva su desglose por estado A2A. La
        // barra lateral es estrecha, así que el resumen se parte en líneas de como mucho 20 columnas.
        flock.extend(crate::rail::partir_por_palabras(
            &self.running_summary(),
            20,
        ));
        sb.push_section("FLOCK", flock);
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
    use codeloom_core::session::SubagentDescriptor;
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
                status: TaskStatus::new(TaskState::Failed),
                artifacts: vec![codeloom_core::a2a::Artifact::text_artifact(
                    "partial output",
                )],
                notice_delivered: false,
                agent: None,
                agent_origin: None,
            }],
        );
        assert_eq!(
            app.subagents.get(&child.0).unwrap().status.state,
            TaskState::Failed
        );

        app.load_session(SessionId(3), Vec::new());
        assert!(app.subagents.is_empty());
    }

    fn rail_child(parent: SessionId, child: i64) -> SubagentDescriptor {
        SubagentDescriptor {
            parent,
            child: SessionId(child),
            job_id: format!("subagent-{child}"),
            label: "review".into(),
            status: TaskStatus::new(TaskState::Working),
            artifacts: Vec::new(),
            notice_delivered: false,
            agent: None,
            agent_origin: None,
        }
    }

    /// Sesión `parent` abierta con los hijos `children` ya cargados en el rail.
    fn app_with_children(parent: SessionId, children: &[i64]) -> App {
        let mut app = App::new();
        app.load_session(parent, Vec::new());
        app.load_subagents(
            parent,
            children
                .iter()
                .map(|&child| rail_child(parent, child))
                .collect(),
        );
        app
    }

    #[test]
    fn open_child_pushes_parent_and_requests_resume() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[7]);
        assert!(app.open_child(7));
        assert_eq!(app.parent_stack, vec![parent]);
        assert_eq!(
            app.session_choice,
            Some(SessionChoice::Resume(SessionId(7)))
        );
    }

    #[test]
    fn open_child_refuses_while_turn_in_flight() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[7]);
        app.turn_in_flight = true;
        assert!(!app.open_child(7));
        assert!(app.parent_stack.is_empty());
        assert_eq!(app.session_choice, None);
    }

    #[test]
    fn open_child_refuses_unknown_child() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[7]);
        assert!(!app.open_child(99));
        assert!(app.parent_stack.is_empty());
        assert_eq!(app.session_choice, None);

        // Sin sesión abierta tampoco hay a dónde volver: se niega.
        assert!(!App::new().open_child(7));
    }

    #[test]
    fn go_parent_pops_and_requests_resume() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[7]);
        assert!(app.open_child(7));
        // La selección ya se despachó: el worker carga el hijo.
        app.session_choice = None;
        app.load_session(SessionId(7), Vec::new());

        assert!(app.go_parent());
        assert!(app.parent_stack.is_empty());
        assert_eq!(app.session_choice, Some(SessionChoice::Resume(parent)));
    }

    #[test]
    fn go_parent_refuses_with_empty_stack() {
        let mut app = App::new();
        assert!(!app.go_parent());
        assert_eq!(app.session_choice, None);
    }

    #[test]
    fn go_parent_refuses_while_turn_in_flight() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[7]);
        assert!(app.open_child(7));
        app.session_choice = None;
        app.load_session(SessionId(7), Vec::new());
        app.turn_in_flight = true;

        assert!(!app.go_parent());
        assert_eq!(app.parent_stack, vec![parent]);
        assert_eq!(app.session_choice, None);
    }

    #[test]
    fn normal_load_session_clears_parent_stack() {
        let mut app = App::new();
        app.parent_stack.push(SessionId(1));
        app.rail_selected = Some(3);
        app.load_session(SessionId(5), Vec::new());
        assert!(app.parent_stack.is_empty());
        assert_eq!(app.rail_selected, None);
    }

    #[test]
    fn navigation_load_keeps_parent_stack() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[7]);
        assert!(app.open_child(7));
        app.session_choice = None;

        // La carga que pide `open_child` conserva la pila...
        app.load_session(SessionId(7), Vec::new());
        assert_eq!(app.parent_stack, vec![parent]);

        // ...pero cualquier otra carga posterior la vacía.
        app.load_session(SessionId(8), Vec::new());
        assert!(app.parent_stack.is_empty());
    }

    #[test]
    fn rail_move_walks_children_without_wrapping() {
        let mut app = app_with_children(SessionId(1), &[3, 5, 9]);
        assert_eq!(app.rail_selected, None);

        app.rail_move(1);
        assert_eq!(app.rail_selected, Some(3));
        app.rail_move(1);
        assert_eq!(app.rail_selected, Some(5));
        app.rail_move(1);
        assert_eq!(app.rail_selected, Some(9));
        // Al final no da la vuelta.
        app.rail_move(1);
        assert_eq!(app.rail_selected, Some(9));

        app.rail_move(-1);
        assert_eq!(app.rail_selected, Some(5));
        app.rail_move(-1);
        assert_eq!(app.rail_selected, Some(3));
        app.rail_move(-1);
        assert_eq!(app.rail_selected, Some(3));
    }

    #[test]
    fn rail_move_without_children_clears_selection() {
        let mut app = App::new();
        app.rail_selected = Some(3);
        app.rail_move(1);
        assert_eq!(app.rail_selected, None);
    }

    #[test]
    fn rail_selection_dropped_when_child_disappears() {
        let parent = SessionId(1);
        let mut app = app_with_children(parent, &[3, 5]);
        app.rail_selected = Some(5);
        app.load_subagents(parent, vec![rail_child(parent, 3)]);
        assert_eq!(app.rail_selected, None);

        app.rail_selected = Some(3);
        app.load_subagents(parent, vec![rail_child(parent, 3)]);
        assert_eq!(app.rail_selected, Some(3));
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

        // Con el look clásico la llamada deja solo su cabecera, entre los dos mensajes: el
        // transcript es el diario de toda iteración.
        assert_eq!(app.transcript.len(), 3);
        assert_eq!(app.transcript.line(0).unwrap().plain(), "hola");
        assert_eq!(app.transcript.line(1).unwrap().plain(), "respuesta");
        assert!(app
            .transcript
            .line(2)
            .unwrap()
            .plain()
            .to_lowercase()
            .contains("view"));
        assert_eq!(app.history.prev(""), Some("hola"));
        assert_eq!(app.tools.len(), 1);
        assert_eq!(app.tools[0].output.as_deref(), Some("contenido"));
        assert_eq!(app.tools[0].status(), batten::state::ToolStatus::Success);
        assert_eq!((app.turn_input_tokens, app.turn_output_tokens), (0, 0));
    }

    #[test]
    fn classic_look_shows_the_subagent_call_with_its_agent_in_the_conversation() {
        let mut app = App::new(); // tema Batten = Classic
        app.restore_messages(vec![
            Message::user("lee un fichero"),
            Message::assistant(vec![ContentPart::ToolCall {
                id: "c1".into(),
                name: "subagent".into(),
                input: serde_json::json!({
                    "prompt": "lee rustfmt.toml",
                    "label": "leer fichero",
                    "agent": "rust"
                }),
            }]),
        ]);
        let texts: Vec<String> = app
            .transcript
            .iter()
            .map(|l| l.plain().to_string())
            .collect();
        assert!(
            texts
                .iter()
                .any(|t| t.contains("subagent") && t.contains("[rust] leer fichero")),
            "la llamada a subagent debe verse con su agente: {texts:?}"
        );
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
    fn show_agents_lists_the_registry_and_approves_a_project_agent() {
        use codeloom_core::registry::{
            AgentRegistry, Approval, BuiltInSource, FileSource, ProfileLevel, TrustStore,
        };
        let mut app = App::new();
        app.show_agents();
        assert!(app.agents_open);
        assert!(app.agent_rows.is_empty());

        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        std::fs::write(
            proyecto.path().join("front.md"),
            "---
name: front
description: Interfaz
---

Cuerpo de front.
",
        )
        .unwrap();
        let registro = AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(
                proyecto.path().to_path_buf(),
                ProfileLevel::Project,
            )),
        ])
        .with_trust(
            TrustStore::open(datos.path().to_path_buf()).unwrap(),
            proyecto.path().to_path_buf(),
            false,
        );
        app.set_agent_registry(Arc::new(registro));
        app.show_agents();
        assert_eq!(app.agent_rows.len(), 2);
        assert_eq!(app.agent_rows[0].name, "default");
        assert_eq!(app.agent_rows[1].name, "front");
        assert_eq!(app.agent_rows[1].approval, Approval::Pending);
        assert!(app.agent_rows[1].body.contains("Cuerpo de front."));

        app.agents_move(1);
        app.agents_move(5);
        assert_eq!(app.agents_selected, 1, "la selección no sale de la lista");
        app.agents_decide(true);
        assert_eq!(app.agent_rows[1].approval, Approval::Approved);
        assert_eq!(app.agents_status.as_deref(), Some("Approved `front`"));
        app.agents_decide(false);
        assert_eq!(app.agent_rows[1].approval, Approval::Pending);

        // `default` no es de proyecto: no se puede aprobar y se explica por qué.
        app.agents_move(-1);
        app.agents_decide(true);
        assert!(app
            .agents_status
            .as_deref()
            .is_some_and(|s| s.starts_with("Not changed:")));
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
    fn switching_look_keeps_the_tool_header_and_restores_the_body() {
        let mut app = crush_app();
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        app.apply_message_added(tool_result_message(
            "c1",
            "linea 1
linea 2",
            false,
        ));
        let con_cuerpo = app
            .transcript
            .iter()
            .filter(|l| l.plain() == "linea 1")
            .count();
        assert_eq!(con_cuerpo, 1, "el look Crush muestra el cuerpo");
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Batten);
        app.sync_all_tools();
        assert_eq!(
            app.transcript
                .iter()
                .filter(|l| l.plain().starts_with("View"))
                .count(),
            1,
            "la cabecera nunca desaparece"
        );
        assert!(!app.transcript.iter().any(|l| l.plain() == "linea 1"));
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.sync_all_tools();
        assert!(app.transcript.iter().any(|l| l.plain() == "linea 1"));
    }

    #[test]
    fn classic_look_leaves_one_header_line_per_tool_call_and_folds_the_output() {
        let mut app = App::new(); // tema Batten = Classic
        app.apply(LiveEvent::ToolStarted {
            call_id: "c1".into(),
            name: "View".into(),
        });
        assert_eq!(
            app.transcript
                .iter()
                .filter(|l| l.plain().starts_with("View"))
                .count(),
            1,
            "toda llamada deja su cabecera en el transcript"
        );
        assert_eq!(app.tools.len(), 1);
        app.apply_message_added(tool_result_message(
            "c1",
            "linea 1
linea 2",
            false,
        ));
        assert!(
            !app.transcript.iter().any(|l| l.plain() == "linea 1"),
            "el cuerpo sigue en la tarjeta fija y en el inspector, no en el transcript"
        );
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
    fn classic_look_orders_the_assistant_text_around_a_tool_chronologically() {
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
        let uno = texts.iter().position(|t| t == "uno").unwrap();
        let herramienta = texts.iter().position(|t| t.starts_with("View")).unwrap();
        let dos = texts.iter().position(|t| t.contains("dos")).unwrap();
        assert!(
            uno < herramienta && herramienta < dos,
            "el texto posterior a la llamada va debajo de ella: {texts:?}"
        );
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
    fn sub(label: &str, state: TaskState, parent: SessionId) -> SubagentDescriptor {
        SubagentDescriptor {
            parent,
            child: SessionId(9),
            job_id: "subagent-1".into(),
            label: label.into(),
            status: TaskStatus::new(state),
            artifacts: Vec::new(),
            notice_delivered: false,
            agent: None,
            agent_origin: None,
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
            TaskState::Working,
            SessionId(1),
        )));
        assert_eq!(
            heads(&app),
            vec![(
                "subagent  review (working)".to_string(),
                ToolStatus::Running
            )]
        );

        let mut done = sub("review", TaskState::Completed, SessionId(1));
        done.artifacts = vec![codeloom_core::a2a::Artifact::text_artifact("listo")];
        app.apply(LiveEvent::SubagentChanged(done));
        assert_eq!(app.transcript.entry_count(), 1);
        assert_eq!(
            heads(&app),
            vec![(
                "subagent  review (completed)".to_string(),
                ToolStatus::Success
            )]
        );
        assert!(texts(&app).iter().any(|t| t == "listo"));
    }

    #[test]
    fn tui_shows_the_same_result_text_as_before() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        let mut done = sub("review", TaskState::Completed, SessionId(1));
        done.artifacts = vec![
            codeloom_core::a2a::Artifact::text_artifact("primera"),
            codeloom_core::a2a::Artifact::text_artifact("segunda"),
        ];
        app.apply(LiveEvent::SubagentChanged(done));
        // Antes el cuerpo era `result` = "primera\nsegunda"; ahora sale de los artefactos y debe igualarlo.
        let lines = texts(&app);
        assert!(lines.iter().any(|t| t == "primera"), "{lines:?}");
        assert!(lines.iter().any(|t| t == "segunda"), "{lines:?}");
    }

    #[test]
    fn activity_status_covers_all_eight_states() {
        let esperados = [
            (TaskState::Submitted, ToolStatus::Running),
            (TaskState::Working, ToolStatus::Running),
            (TaskState::InputRequired, ToolStatus::Running),
            (TaskState::AuthRequired, ToolStatus::Running),
            (TaskState::Completed, ToolStatus::Success),
            (TaskState::Failed, ToolStatus::Error),
            (TaskState::Rejected, ToolStatus::Error),
            (TaskState::Canceled, ToolStatus::Canceled),
        ];
        assert_eq!(esperados.len(), TaskState::ALL.len());
        for (state, esperado) in esperados {
            assert_eq!(subagent_tool_status(state), esperado, "{state:?}");
        }
    }

    #[test]
    fn activity_header_names_the_exact_a2a_state_in_parentheses() {
        for state in TaskState::ALL {
            let mut app = crush_app();
            app.session_id = Some(SessionId(1));
            app.apply(LiveEvent::SubagentChanged(sub(
                "review",
                state,
                SessionId(1),
            )));
            let nombre = agent_state(state).label();
            assert_eq!(
                heads(&app),
                vec![(
                    format!("subagent  review ({nombre})"),
                    subagent_tool_status(state)
                )],
                "{state:?}"
            );
        }
    }

    /// Hijo de la sesión 1 con el agente y el origen indicados.
    fn con_agente(agent: Option<&str>, origin: Option<&str>) -> SubagentDescriptor {
        let mut hijo = sub("review", TaskState::Working, SessionId(1));
        hijo.agent = agent.map(str::to_owned);
        hijo.agent_origin = origin.map(str::to_owned);
        hijo
    }

    #[test]
    fn agent_tag_names_the_agent_and_flags_project_and_remote_origins() {
        assert_eq!(agent_tag(&con_agente(None, None)), None);
        assert_eq!(
            agent_tag(&con_agente(Some("default"), Some("built-in"))).as_deref(),
            Some("[default]")
        );
        assert_eq!(
            agent_tag(&con_agente(Some("rust"), Some("global:C:/agentes/rust.md"))).as_deref(),
            Some("[rust]")
        );
        assert_eq!(
            agent_tag(&con_agente(
                Some("front"),
                Some("project:.codeloom/agents/front.md")
            ))
            .as_deref(),
            Some("[front·project]")
        );
        assert_eq!(
            agent_tag(&con_agente(
                Some("ext"),
                Some("remote:https://ext.example/a2a")
            ))
            .as_deref(),
            Some("[ext·remote]")
        );
    }

    #[test]
    fn activity_header_keeps_the_a2a_state_last() {
        for state in TaskState::ALL {
            let mut app = crush_app();
            app.session_id = Some(SessionId(1));
            let mut hijo = sub("review", state, SessionId(1));
            hijo.agent = Some("front".into());
            app.apply(LiveEvent::SubagentChanged(hijo));
            let nombre = agent_state(state).label();
            assert_eq!(
                heads(&app),
                vec![(
                    format!("subagent  review [front] ({nombre})"),
                    subagent_tool_status(state)
                )],
                "{state:?}"
            );
        }
    }

    #[test]
    fn state_counts_are_exact_and_ordered() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        for (child, state) in [
            (10, TaskState::Failed),
            (11, TaskState::InputRequired),
            (12, TaskState::Working),
            (13, TaskState::Working),
        ] {
            app.subagents
                .insert(child, child_descriptor(child, state, SessionId(1)));
        }
        // Un subagente de otra sesión no entra en el recuento.
        app.subagents
            .insert(20, child_descriptor(20, TaskState::Working, SessionId(2)));
        assert_eq!(
            app.subagent_state_counts(),
            vec![
                (TaskState::Working, 2),
                (TaskState::InputRequired, 1),
                (TaskState::Failed, 1),
            ]
        );
        assert_eq!(
            app.running_summary(),
            "3 subagents running (working 2, input-required 1)"
        );
        assert_eq!(app.finished_summary(), " (subagents: failed 1)");
        assert_eq!(
            formato_recuento(&[(TaskState::Completed, 2), (TaskState::Canceled, 1)]),
            "completed 2, canceled 1"
        );

        // Sin subagentes en curso no hay paréntesis que ocultar en el resumen en curso.
        let mut terminados = App::new();
        terminados.session_id = Some(SessionId(1));
        terminados
            .subagents
            .insert(10, child_descriptor(10, TaskState::Completed, SessionId(1)));
        assert_eq!(terminados.running_summary(), "0 subagents running");
        assert_eq!(terminados.finished_summary(), " (subagents: completed 1)");
        assert_eq!(App::new().finished_summary(), "");
    }

    #[test]
    fn active_agents_counts_only_non_terminal() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        for (indice, state) in TaskState::ALL.into_iter().enumerate() {
            let child = 10 + indice as i64;
            app.subagents
                .insert(child, child_descriptor(child, state, SessionId(1)));
        }
        // Un subagente en marcha de otra sesión no cuenta.
        app.subagents
            .insert(99, child_descriptor(99, TaskState::Working, SessionId(2)));
        assert_eq!(app.active_agents(), 4);
    }

    #[test]
    fn rejected_canceled_and_auth_required_do_not_wake_the_parent() {
        for state in [
            TaskState::Rejected,
            TaskState::Canceled,
            TaskState::AuthRequired,
        ] {
            let mut app = App::new();
            app.session_id = Some(SessionId(1));
            app.apply(LiveEvent::SubagentChanged(child_descriptor(
                9,
                state,
                SessionId(1),
            )));
            app.apply(LiveEvent::SubagentSettled {
                parent: SessionId(1),
                child: SessionId(9),
                status: state,
            });
            assert!(!app.wake_pending, "{state:?} no debe despertar al padre");
        }
    }

    #[test]
    fn input_required_still_wakes_the_parent() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(1),
            child: SessionId(9),
            label: "obrero".into(),
        });
        assert!(app.wake_pending);
    }

    #[test]
    fn job_snapshot_keeps_the_waiting_state_of_the_child() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::InputRequired,
            SessionId(1),
        )));
        // El job sigue en marcha: su snapshot no debe devolver al hijo a `working`.
        let mut en_marcha = job("subagent-1", JobStatus::Running, SessionId(1));
        en_marcha.kind = "subagent".into();
        app.apply(LiveEvent::JobChanged(en_marcha));
        assert_eq!(
            app.subagents[&9].status.state,
            TaskState::InputRequired,
            "el estado de espera es de la tarea, no del job"
        );
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
        let mut done = sub("review", TaskState::Completed, SessionId(1));
        done.artifacts = vec![codeloom_core::a2a::Artifact::text_artifact("listo")];
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
        let mut done = sub("review", TaskState::Completed, SessionId(1));
        done.artifacts = vec![codeloom_core::a2a::Artifact::text_artifact("listo")];
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
                (
                    "subagent  review (completed)".to_string(),
                    ToolStatus::Success
                ),
                ("job  build".to_string(), ToolStatus::Error),
            ]
        );
        let lines = texts(&app);
        let pos = |text: &str| lines.iter().position(|t| t == text).unwrap();
        assert!(pos("hola") < pos("subagent  review (completed)"));
        assert!(pos("subagent  review (completed)") < pos("job  build"));
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
    fn classic_look_keeps_notices_as_user_messages_and_shows_the_activity_header() {
        let mut app = App::new(); // tema Batten = Classic
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Working,
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
        assert_eq!(
            heads(&app).len(),
            1,
            "la actividad del subagente deja su cabecera"
        );
        assert!(heads(&app)[0].0.starts_with("subagent  review"));
    }

    #[test]
    fn switching_look_keeps_the_activity_header() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Working,
            SessionId(1),
        )));
        assert_eq!(heads(&app).len(), 1);
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Batten);
        app.sync_all_tools();
        let esperado = vec![(
            "subagent  review (working)".to_string(),
            ToolStatus::Running,
        )];
        assert_eq!(
            heads(&app),
            esperado,
            "la cabecera sobrevive al look clásico"
        );
        app.theme = batten::widgets::Theme::preset(batten::widgets::ThemePreset::Crush);
        app.sync_all_tools();
        assert_eq!(heads(&app), esperado);
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
    fn subagent_input_events_drive_the_waiting_state() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Working,
            SessionId(1),
        )));

        // Un cambio de un hijo de otro padre no se refleja en esta sesión.
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::InputRequired,
            SessionId(2),
        )));
        assert_eq!(app.subagents[&9].status.state, TaskState::Working);

        // La espera es el estado de la tarea; la pregunta solo despierta al padre.
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::InputRequired,
            SessionId(1),
        )));
        assert_eq!(app.subagents[&9].status.state, TaskState::InputRequired);
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(1),
            child: SessionId(9),
            label: "review".into(),
        });
        assert!(app.wake_pending);

        // Responder devuelve la tarea a `working`, y el estado lo dice el propio cambio.
        app.apply(LiveEvent::SubagentInputResolved {
            parent: SessionId(1),
            child: SessionId(9),
        });
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Working,
            SessionId(1),
        )));
        assert_eq!(app.subagents[&9].status.state, TaskState::Working);

        // Un estado terminal cierra la espera.
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Completed,
            SessionId(1),
        )));
        assert_eq!(app.subagents[&9].status.state, TaskState::Completed);
    }

    /// App en reposo con la sesión 1 abierta, que es el padre de los hijos de los tests.
    fn idle_parent_app() -> App {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app
    }

    /// Evento de cierre de un hijo (sesión 9) de la sesión 1 con el estado dado.
    fn settled(status: TaskState) -> LiveEvent {
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
    fn wake_requested_on_completed_or_failed_but_not_canceled_or_rejected() {
        for status in [TaskState::Completed, TaskState::Failed] {
            let mut app = idle_parent_app();
            app.apply(settled(status));
            assert!(app.wake_pending, "{status:?} debe despertar al padre");
        }
        for status in [TaskState::Canceled, TaskState::Rejected] {
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
        app.apply(settled(TaskState::Completed));
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
            status: TaskState::Completed,
        });
        assert!(!app.wake_pending);

        let mut sin_sesion = App::new();
        sin_sesion.apply(settled(TaskState::Completed));
        assert!(!sin_sesion.wake_pending);
    }

    #[test]
    fn opening_another_session_drops_the_pending_wake() {
        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Completed));
        assert!(app.wake_pending);
        app.load_session(SessionId(2), Vec::new());
        assert!(!app.wake_pending);
        assert!(app.wake_causes.is_empty());
    }

    /// Lista de textos de las líneas de sistema del transcript.
    fn system_lines(app: &App) -> Vec<String> {
        app.transcript
            .iter()
            .filter(|l| l.kind == batten::state::LineKind::System)
            .map(|l| l.plain().to_string())
            .collect()
    }

    #[test]
    fn settled_child_records_wake_cause() {
        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Completed));
        assert_eq!(app.wake_causes, vec![(9, "completed")]);

        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Failed));
        assert_eq!(app.wake_causes, vec![(9, "failed")]);
    }

    #[test]
    fn input_request_records_wake_cause() {
        let mut app = idle_parent_app();
        app.apply(LiveEvent::SubagentInputRequested {
            parent: SessionId(1),
            child: SessionId(9),
            label: "review".into(),
        });
        assert_eq!(app.wake_causes, vec![(9, "asks input")]);
    }

    #[test]
    fn killed_child_records_no_cause() {
        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Canceled));
        assert!(app.wake_causes.is_empty());
    }

    #[test]
    fn woken_turn_emits_cause_line_and_clears_causes() {
        let mut app = idle_parent_app();
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Working,
            SessionId(1),
        )));
        app.apply(settled(TaskState::Completed));
        assert!(app.take_wake());
        app.apply(LiveEvent::TurnStarted);
        assert_eq!(
            system_lines(&app),
            vec!["⟳ auto-turn · woken by «review» (completed)".to_string()]
        );
        assert!(app.wake_causes.is_empty());
    }

    #[test]
    fn user_turn_emits_no_cause_line() {
        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Completed));
        // Turno del usuario: no hay `wake_in_flight`, así que no hay línea ni se toca la causa.
        app.apply(LiveEvent::TurnStarted);
        assert!(system_lines(&app).is_empty());
        assert_eq!(app.wake_causes, vec![(9, "completed")]);
    }

    #[test]
    fn cause_line_with_several_causes_shows_latest_and_count() {
        let mut app = idle_parent_app();
        app.apply(LiveEvent::SubagentChanged(sub(
            "review",
            TaskState::Working,
            SessionId(1),
        )));
        app.wake_causes = vec![(9, "completed"), (9, "failed"), (9, "asks input")];
        assert_eq!(
            app.wake_cause_line().as_deref(),
            Some("⟳ auto-turn · woken by «review» (asks input) (+2 more)")
        );
    }

    #[test]
    fn cause_line_uses_agent_id_for_unknown_child() {
        let mut app = idle_parent_app();
        app.wake_causes = vec![(42, "completed")];
        assert_eq!(
            app.wake_cause_line().as_deref(),
            Some("⟳ auto-turn · woken by «agent #42» (completed)")
        );
        assert_eq!(App::new().wake_cause_line(), None);
    }

    #[test]
    fn exhausted_budget_drops_wake_causes() {
        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Completed));
        app.auto_turns_left = 0;
        assert!(!app.take_wake());
        assert!(app.wake_causes.is_empty());
    }

    #[test]
    fn escape_clears_wake_causes() {
        let mut app = idle_parent_app();
        app.apply(settled(TaskState::Completed));
        assert!(!app.wake_causes.is_empty());
        app.request_cancel();
        assert!(app.wake_causes.is_empty());
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
    fn child_descriptor(child: i64, state: TaskState, parent: SessionId) -> SubagentDescriptor {
        SubagentDescriptor {
            parent,
            child: SessionId(child),
            job_id: format!("subagent-{child}"),
            label: format!("hijo-{child}"),
            status: TaskStatus::new(state),
            artifacts: Vec::new(),
            notice_delivered: false,
            agent: None,
            agent_origin: None,
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

    /// Ficheros de un hijo tal como los ve la interfaz: ruta → (añadidas, quitadas).
    fn child_files(app: &App, child: i64) -> Vec<(String, (usize, usize))> {
        app.files_by_session
            .get(&child)
            .map(|files| files.iter().map(|(p, c)| (p.clone(), *c)).collect())
            .unwrap_or_default()
    }

    #[test]
    fn child_file_diff_accumulates_by_child_and_keeps_last_per_path() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));

        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::FileDiff {
                path: "a.rs".into(),
                added: 1,
                removed: 0,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::FileDiff {
                path: "b.rs".into(),
                added: 1,
                removed: 1,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::FileDiff {
                path: "a.rs".into(),
                added: 4,
                removed: 2,
            },
        ));
        app.apply(child_event(
            SessionId(1),
            10,
            LiveEvent::FileDiff {
                path: "a.rs".into(),
                added: 7,
                removed: 7,
            },
        ));

        assert_eq!(
            child_files(&app, 9),
            vec![("a.rs".to_string(), (4, 2)), ("b.rs".to_string(), (1, 1))],
            "la última edición de cada ruta gana y las rutas se ordenan"
        );
        assert_eq!(child_files(&app, 10), vec![("a.rs".to_string(), (7, 7))]);
        assert_eq!(app.child_file_count(SessionId(9)), 2);
        assert_eq!(app.child_file_count(SessionId(42)), 0);
    }

    #[test]
    fn child_file_diff_does_not_touch_parent_changes() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::FileDiff {
            path: "propio.rs".into(),
            added: 3,
            removed: 1,
        });
        let antes = app.changes.totals();

        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::FileDiff {
                path: "del_hijo.rs".into(),
                added: 5,
                removed: 5,
            },
        ));

        assert_eq!(app.changes.len(), 1, "el panel del padre no cambia");
        assert_eq!(app.changes.totals(), antes);
        assert_eq!(app.changes.iter().next().unwrap().path, "propio.rs");
    }

    #[test]
    fn own_file_diff_is_recorded_for_the_open_session() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::FileDiff {
            path: "a.rs".into(),
            added: 1,
            removed: 0,
        });
        app.apply(LiveEvent::FileDiff {
            path: "a.rs".into(),
            added: 2,
            removed: 3,
        });

        assert_eq!(
            app.files_by_session
                .get(&1)
                .and_then(|files| files.get("a.rs")),
            Some(&(2, 3)),
            "el fichero propio queda anotado en la sesión abierta"
        );
        assert_eq!(app.changes.len(), 1);
    }

    #[test]
    fn parent_files_return_to_changes_panel_after_child_roundtrip() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.subagents
            .insert(9, child_descriptor(9, TaskState::Working, SessionId(1)));
        app.apply(LiveEvent::FileDiff {
            path: "a.rs".into(),
            added: 1,
            removed: 0,
        });

        // Entrar en el hijo: su panel empieza vacío, sin los ficheros del padre.
        assert!(app.open_child(9));
        app.load_session(SessionId(9), Vec::new());
        assert!(
            app.changes.is_empty(),
            "el hijo no hereda los ficheros del padre"
        );

        // Mientras el hijo está abierto, sus propias ediciones llegan directas.
        app.apply(LiveEvent::FileDiff {
            path: "b.rs".into(),
            added: 2,
            removed: 0,
        });
        assert_eq!(app.changes.len(), 1);

        // Volver al padre: sus ficheros reaparecen y los del hijo no.
        assert!(app.go_parent());
        app.load_session(SessionId(1), Vec::new());
        let rutas: Vec<&str> = app.changes.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(rutas, ["a.rs"], "el panel del padre recupera sus ficheros");
        assert!(
            app.files_by_session.contains_key(&9),
            "la navegación conserva al hijo"
        );
    }

    #[test]
    fn loading_an_unrelated_session_clears_files_by_session() {
        let mut app = crush_app();
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::FileDiff {
            path: "a.rs".into(),
            added: 1,
            removed: 0,
        });
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::FileDiff {
                path: "b.rs".into(),
                added: 1,
                removed: 0,
            },
        ));

        app.load_session(SessionId(7), Vec::new());

        assert!(
            app.files_by_session.is_empty(),
            "la carga ajena vacía el mapa"
        );
        assert!(app.changes.is_empty());
    }

    /// App con el catálogo de precios y unos ajustes que apuntan a `acme` con el modelo dado.
    fn priced_app(model: &str, base_url: &str) -> App {
        let mut app = App::new();
        app.set_catalog(priced_sample_catalog());
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: model.into(),
            base_url: base_url.into(),
            api_key_env: "ACME_KEY".into(),
            effort: None,
        });
        app
    }

    #[test]
    fn model_prices_match_provider_by_url_ignoring_trailing_slash() {
        // La `api` del catálogo lleva barra final; los ajustes, no. Debe casar en ambos sentidos.
        assert_eq!(
            priced_app("priced", "https://acme.example/v1").model_prices(),
            Some((3.0, 15.0))
        );
        assert_eq!(
            priced_app("priced", "https://acme.example/v1/").model_prices(),
            Some((3.0, 15.0))
        );
    }

    #[test]
    fn model_prices_none_without_catalog() {
        let mut app = App::new();
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "priced".into(),
            base_url: "https://acme.example/v1".into(),
            api_key_env: "ACME_KEY".into(),
            effort: None,
        });
        assert_eq!(app.model_prices(), None, "sin catálogo no hay precio");
    }

    #[test]
    fn model_prices_none_when_a_price_is_missing() {
        let url = "https://acme.example/v1";
        assert_eq!(priced_app("half", url).model_prices(), None);
        assert_eq!(priced_app("plain", url).model_prices(), None);
        assert_eq!(priced_app("ausente", url).model_prices(), None);
    }

    /// Apunte de prueba con precios conocidos (3 y 15 por millón), como los que guarda el libro.
    fn usage_record(model: &str, input: u64, output: u64) -> UsageRecord {
        UsageRecord {
            session: SessionId(1),
            model: model.into(),
            input_tokens: input,
            output_tokens: output,
            input_price: Some(3.0),
            output_price: Some(15.0),
            created_at_ms: 1,
        }
    }

    #[test]
    fn load_usage_replaces_records_for_the_open_session() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.usage_records = vec![usage_record("viejo", 1, 1)];
        let stored = vec![usage_record("priced", 10, 2), usage_record("priced", 20, 4)];
        app.load_usage(SessionId(1), stored.clone());
        assert_eq!(
            app.usage_records, stored,
            "la lista de la base de datos sustituye a la anterior"
        );
    }

    #[test]
    fn load_usage_ignores_other_sessions() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        let mine = vec![usage_record("priced", 10, 2)];
        app.load_usage(SessionId(1), mine.clone());
        app.load_usage(SessionId(2), vec![usage_record("ajena", 99, 99)]);
        assert_eq!(
            app.usage_records, mine,
            "una carga de otra sesión no se mezcla"
        );
    }

    #[test]
    fn live_usage_appends_a_priced_record() {
        let mut app = priced_app("priced", "https://acme.example/v1");
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::Usage {
            input_tokens: 100,
            output_tokens: 10,
        });
        assert_eq!(app.usage_records.len(), 1);
        let record = &app.usage_records[0];
        assert_eq!(record.model, "priced");
        assert_eq!((record.input_tokens, record.output_tokens), (100, 10));
        assert_eq!(
            (record.input_price, record.output_price),
            (Some(3.0), Some(15.0))
        );
        assert!(record.created_at_ms > 0, "el apunte lleva marca de tiempo");
    }

    #[test]
    fn live_child_usage_appends_a_record() {
        let mut app = priced_app("priced", "https://acme.example/v1");
        app.session_id = Some(SessionId(1));
        app.apply(child_event(
            SessionId(1),
            7,
            LiveEvent::Usage {
                input_tokens: 50,
                output_tokens: 5,
            },
        ));
        assert_eq!(app.usage_records.len(), 1);
        assert_eq!(app.usage_records[0].model, "priced");
        assert_eq!(app.usage_records[0].input_tokens, 50);
        // Un hijo de otra sesión no entra en la sesión abierta.
        app.apply(child_event(
            SessionId(9),
            8,
            LiveEvent::Usage {
                input_tokens: 1,
                output_tokens: 1,
            },
        ));
        assert_eq!(
            app.usage_records.len(),
            1,
            "solo los hijos de la sesión abierta"
        );
    }

    #[test]
    fn session_load_clears_usage_records() {
        let mut app = priced_app("priced", "https://acme.example/v1");
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::Usage {
            input_tokens: 1,
            output_tokens: 1,
        });
        app.load_session(SessionId(2), Vec::new());
        assert!(
            app.usage_records.is_empty(),
            "la sesión anterior no deja apuntes"
        );
    }

    #[test]
    fn reload_does_not_duplicate_live_records() {
        let mut app = priced_app("priced", "https://acme.example/v1");
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::Usage {
            input_tokens: 100,
            output_tokens: 10,
        });
        // La base de datos ya contiene ese apunte: la recarga lo sustituye, no lo suma.
        let persisted = app.usage_records.clone();
        app.load_usage(SessionId(1), persisted.clone());
        assert_eq!(app.usage_records, persisted);
        assert_eq!(app.usage_records.len(), 1);
        // Un apunte vivo nuevo y una recarga posterior que ya lo incluye: sigue habiendo dos.
        app.apply(LiveEvent::Usage {
            input_tokens: 20,
            output_tokens: 2,
        });
        let persisted = app.usage_records.clone();
        app.load_usage(SessionId(1), persisted);
        assert_eq!(app.usage_records.len(), 2);
    }

    /// Apunte con precios `(entrada, salida)` por millón, o sin precio si `prices` es `None`.
    fn priced_record(input: u64, output: u64, prices: Option<(f64, f64)>) -> UsageRecord {
        UsageRecord {
            session: SessionId(1),
            model: "m".into(),
            input_tokens: input,
            output_tokens: output,
            input_price: prices.map(|(price_in, _)| price_in),
            output_price: prices.map(|(_, price_out)| price_out),
            created_at_ms: 1,
        }
    }

    /// Apunte de un hijo concreto, con precios `(entrada, salida)` o sin ellos.
    fn child_record(
        child: i64,
        input: u64,
        output: u64,
        prices: Option<(f64, f64)>,
    ) -> UsageRecord {
        UsageRecord {
            session: SessionId(child),
            ..priced_record(input, output, prices)
        }
    }

    #[test]
    fn child_cost_sums_only_that_childs_records() {
        let mut app = App::new();
        app.usage_records = vec![
            child_record(9, 1_000_000, 0, Some((3.0, 15.0))),
            child_record(9, 0, 100_000, Some((3.0, 15.0))),
            child_record(10, 1_000_000, 0, Some((3.0, 15.0))),
        ];
        let (cost, partial) = app
            .child_cost(SessionId(9))
            .expect("el hijo 9 tiene apuntes con precio");
        // 3,00 de entrada más 1,50 de salida. Los apuntes del hijo 10 no entran.
        assert!((cost - 4.5).abs() < 1e-9, "coste inesperado: {cost}");
        assert!(!partial, "todos los apuntes del hijo tienen precio");
        assert_eq!(
            app.child_cost(SessionId(11)),
            None,
            "un hijo sin apuntes no tiene coste"
        );
    }

    #[test]
    fn child_cost_none_without_prices() {
        let mut app = App::new();
        app.usage_records = vec![child_record(9, 1_000, 1_000, None)];
        assert_eq!(
            app.child_cost(SessionId(9)),
            None,
            "ningún apunte del hijo tiene precio"
        );
    }

    #[test]
    fn child_cost_partial_marks_plus() {
        let mut app = App::new();
        app.usage_records = vec![
            child_record(9, 1_000_000, 0, Some((3.0, 15.0))),
            child_record(9, 1_000_000, 1_000_000, None),
        ];
        let (cost, partial) = app
            .child_cost(SessionId(9))
            .expect("hay un apunte con precio");
        assert!(
            (cost - 3.0).abs() < 1e-9,
            "solo suma el apunte con precio: {cost}"
        );
        assert!(
            partial,
            "un apunte sin precio marca el coste del hijo como parcial"
        );
    }

    #[test]
    fn child_costs_plus_parent_equal_total() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.usage_records = vec![
            child_record(1, 1_000_000, 0, Some((3.0, 15.0))),
            child_record(9, 0, 100_000, Some((3.0, 15.0))),
            child_record(10, 1_000_000, 1_000_000, None),
            child_record(9, 1_000_000, 0, Some((2.0, 10.0))),
        ];
        let (total, partial) = app.estimated_cost().expect("hay apuntes");
        let mut suma = 0.0;
        for child in [9, 10] {
            if let Some((cost, _)) = app.child_cost(SessionId(child)) {
                suma += cost;
            }
        }
        suma += app
            .usage_records
            .iter()
            .filter(|record| record.session == SessionId(1))
            .filter_map(|record| record.cost())
            .sum::<f64>();
        assert!(
            (total - suma).abs() < 1e-9,
            "hijos más padre deben dar el total: {suma} frente a {total}"
        );
        assert!(partial, "el apunte sin precio del hijo 10 marca el total");
    }

    #[test]
    fn live_child_usage_is_attributed_to_the_child() {
        let mut app = App::new();
        app.session_id = Some(SessionId(1));
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 1_000,
                output_tokens: 10,
            },
        ));
        app.apply(LiveEvent::Usage {
            input_tokens: 2_000,
            output_tokens: 20,
        });
        let sesiones: Vec<SessionId> = app
            .usage_records
            .iter()
            .map(|record| record.session)
            .collect();
        assert_eq!(
            sesiones,
            vec![SessionId(9), SessionId(1)],
            "cada apunte vivo lleva la sesión que lo originó"
        );
    }

    #[test]
    fn estimated_cost_sums_each_record_at_its_own_price() {
        // Cambio de modelo a mitad de sesión: cada apunte conserva su precio.
        let mut app = App::new();
        app.usage_records = vec![
            priced_record(1_000_000, 0, Some((3.0, 15.0))),
            priced_record(0, 100_000, Some((2.0, 10.0))),
        ];
        // Primero: 1.000.000 a 3 $/M = 3. Segundo: 100.000 a 10 $/M = 1.
        let (cost, partial) = app.estimated_cost().expect("hay apuntes");
        assert!((cost - 4.0).abs() < 1e-9, "coste inesperado: {cost}");
        assert!(!partial, "todos los apuntes tienen precio");
    }

    #[test]
    fn estimated_cost_marks_partial_when_a_record_has_no_price() {
        let mut app = App::new();
        app.usage_records = vec![
            priced_record(1_000_000, 0, Some((3.0, 15.0))),
            priced_record(1_000_000, 1_000_000, None),
        ];
        let (cost, partial) = app.estimated_cost().expect("hay apuntes");
        assert!(
            (cost - 3.0).abs() < 1e-9,
            "solo suma los apuntes con precio: {cost}"
        );
        assert!(partial, "un apunte sin precio marca el total como parcial");
    }

    #[test]
    fn estimated_cost_none_without_records() {
        let app = priced_app("priced", "https://acme.example/v1");
        assert_eq!(app.estimated_cost(), None, "sin apuntes no hay estimación");
    }

    #[test]
    fn estimated_cost_adds_children() {
        let mut app = priced_app("priced", "https://acme.example/v1");
        app.session_id = Some(SessionId(1));
        app.apply(LiveEvent::Usage {
            input_tokens: 1_000_000,
            output_tokens: 100_000,
        });
        app.apply(child_event(
            SessionId(1),
            9,
            LiveEvent::Usage {
                input_tokens: 1_000_000,
                output_tokens: 0,
            },
        ));
        // Entrada: 2.000.000 a 3 $/M = 6. Salida: 100.000 a 15 $/M = 1,5.
        let (cost, partial) = app.estimated_cost().expect("hay apuntes");
        assert!((cost - 7.5).abs() < 1e-9, "coste inesperado: {cost}");
        assert!(!partial);
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
        app.subagents
            .insert(9, child_descriptor(9, TaskState::Working, SessionId(1)));
        app.subagents
            .insert(10, child_descriptor(10, TaskState::Completed, SessionId(1)));
        app.subagents
            .insert(11, child_descriptor(11, TaskState::Working, SessionId(1)));
        app.subagents
            .insert(12, child_descriptor(12, TaskState::Working, SessionId(2)));
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

    #[test]
    fn toggle_mouse_capture_marks_dirty_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        let mut app = App::new();
        assert!(app.mouse_capture);
        assert!(!app.mouse_capture_dirty);
        app.set_prefs_path(path.clone());
        app.toggle_mouse_capture();
        assert!(!app.mouse_capture);
        assert!(app.mouse_capture_dirty);
        assert!(app.slash_feedback.is_none());
        assert_eq!(crate::prefs::load_mouse(&path), Some(false));
        app.toggle_mouse_capture();
        assert!(app.mouse_capture);
        assert_eq!(crate::prefs::load_mouse(&path), Some(true));
    }

    #[test]
    fn set_prefs_path_loads_mouse_without_marking_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ui.toml");
        // Espacios finales: si el fichero se reescribiera, desaparecerían.
        let original = "mouse = false   \n";
        std::fs::write(&path, original).unwrap();
        let mut app = App::new();
        app.set_prefs_path(path.clone());
        assert!(!app.mouse_capture);
        assert!(!app.mouse_capture_dirty);
        assert!(app.slash_feedback.is_none());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn mouse_save_error_does_not_block_the_change() {
        let dir = tempfile::tempdir().unwrap();
        // Un fichero normal como «directorio» padre: la escritura siempre falla.
        let blocker = dir.path().join("bloqueo");
        std::fs::write(&blocker, "x").unwrap();
        let mut app = App::new();
        app.set_prefs_path(blocker.join("ui.toml"));
        app.toggle_mouse_capture();
        assert!(!app.mouse_capture);
        assert!(app.mouse_capture_dirty);
        assert!(app
            .slash_feedback
            .as_deref()
            .unwrap()
            .starts_with("Mouse setting not saved"));
    }

    #[test]
    fn pane_at_prefers_the_composer_over_the_transcript() {
        let mut app = App::new();
        let transcript = Rect::new(0, 0, 40, 20);
        let composer = Rect::new(0, 15, 40, 5);
        // El transcript va primero a propósito: el composer debe ganar igualmente.
        app.pane_rects = vec![(Focus::Transcript, transcript), (Focus::Composer, composer)];
        app.composer_area = Some(composer);
        assert_eq!(app.pane_at(5, 16), Some(Focus::Composer));
        assert_eq!(app.pane_at(5, 3), Some(Focus::Transcript));
    }

    #[test]
    fn pane_at_returns_none_outside_any_pane() {
        let mut app = App::new();
        app.pane_rects = vec![(Focus::Transcript, Rect::new(0, 0, 40, 20))];
        assert_eq!(app.pane_at(50, 3), None);
        // Los bordes derecho e inferior quedan fuera del rectángulo.
        assert_eq!(app.pane_at(40, 0), None);
        assert_eq!(app.pane_at(3, 20), None);
    }

    fn settings_fixture() -> ProviderSettings {
        ProviderSettings {
            kind: codeloom_core::config::ProviderKind::OpenAiCompat,
            model: "model-a".into(),
            base_url: "https://example.test/v1".into(),
            api_key_env: "API_KEY".into(),
            effort: None,
        }
    }

    fn app_with_settings_open() -> App {
        let mut app = App::new();
        app.set_provider_settings(settings_fixture());
        app.open_settings();
        app
    }

    #[test]
    fn settings_not_dirty_when_opened() {
        let app = app_with_settings_open();
        assert!(app.settings_open);
        assert!(!app.settings_dirty());
    }

    #[test]
    fn editing_a_field_makes_settings_dirty() {
        let mut app = app_with_settings_open();
        app.provider_settings.as_mut().unwrap().model = "model-b".into();
        assert!(app.settings_dirty());
    }

    #[test]
    fn esc_with_unsaved_changes_warns_first() {
        let mut app = app_with_settings_open();
        app.provider_settings.as_mut().unwrap().model = "model-b".into();
        assert!(!app.close_settings());
        assert!(app.settings_open);
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("Unsaved changes · Esc again to discard")
        );
        assert_eq!(app.provider_settings.as_ref().unwrap().model, "model-b");
    }

    #[test]
    fn second_esc_discards_and_restores() {
        let mut app = app_with_settings_open();
        app.provider_settings.as_mut().unwrap().model = "model-b".into();
        assert!(!app.close_settings());
        assert!(app.close_settings());
        assert!(!app.settings_open);
        assert!(!app.settings_dirty());
        assert_eq!(app.provider_settings.as_ref().unwrap().model, "model-a");
        assert_eq!(app.model, "model-a");
    }

    #[test]
    fn esc_without_changes_closes_at_once() {
        let mut app = app_with_settings_open();
        assert!(app.close_settings());
        assert!(!app.settings_open);
    }

    #[test]
    fn successful_save_clears_dirty() {
        let mut app = app_with_settings_open();
        app.provider_settings.as_mut().unwrap().model = "model-b".into();
        app.settings_saving = true;
        app.settings_saved(Ok(()));
        assert!(!app.settings_dirty());
        assert!(app.close_settings());
    }

    #[test]
    fn editing_after_warning_cancels_the_warning() {
        let mut app = app_with_settings_open();
        app.provider_settings.as_mut().unwrap().model = "model-b".into();
        assert!(!app.close_settings());
        app.settings_field = 1;
        app.edit_settings_value();
        app.settings_editor.set_text("model-c");
        app.commit_settings_value();
        assert_eq!(app.settings_feedback, None);
        assert!(!app.close_settings());
        assert!(app.settings_open);
    }

    #[test]
    fn set_catalog_replaces_the_catalog() {
        use codeloom_core::catalog::Catalog;

        let sample = r#"{"anthropic": {"id": "anthropic", "name": "Anthropic",
            "npm": "@ai-sdk/anthropic", "env": ["ANTHROPIC_API_KEY"],
            "models": {"m": {"id": "m", "tool_call": true}}}}"#;

        let mut app = App::new();
        assert!(app.catalog.is_empty(), "por defecto el catálogo está vacío");

        app.set_catalog(std::sync::Arc::new(Catalog::from_json(sample).unwrap()));
        assert_eq!(app.catalog.providers.len(), 1);
        assert_eq!(app.catalog.providers[0].id, "anthropic");

        app.set_catalog(std::sync::Arc::new(Catalog::default()));
        assert!(
            app.catalog.is_empty(),
            "el catálogo nuevo sustituye al anterior"
        );
    }

    fn anthropic_settings_open() -> App {
        let mut app = App::new();
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::Anthropic,
            model: "model-a".into(),
            base_url: "https://api.anthropic.com".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: None,
        });
        app.set_catalog(picker_sample_catalog());
        app.open_settings();
        app
    }

    #[test]
    fn picker_filters_by_query() {
        let mut app = app_with_settings_open();
        app.set_catalog(picker_sample_catalog());
        app.open_picker(SettingsPicker::Provider);
        assert_eq!(app.picker_items().len(), 2);
        app.picker_query = "ANTH".into();
        let items = app.picker_items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "anthropic");
        app.picker_query = "nada".into();
        assert!(app.picker_items().is_empty());
    }

    #[test]
    fn choosing_a_provider_fills_kind_url_and_env() {
        let mut app = app_with_settings_open();
        app.set_catalog(picker_sample_catalog());
        app.open_picker(SettingsPicker::Provider);
        app.picker_query = "anthropic".into();
        app.picker_choose();
        let settings = app.provider_settings.as_ref().unwrap();
        assert_eq!(
            settings.kind,
            codeloom_core::config::ProviderKind::Anthropic
        );
        assert_eq!(settings.base_url, "https://api.anthropic.com");
        assert_eq!(settings.api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(
            settings.model, "model-a",
            "elegir proveedor no toca el modelo"
        );
        // Tras elegir proveedor se pasa a la lista de sus modelos.
        assert_eq!(app.settings_picker, Some(SettingsPicker::Model));
        assert_eq!(app.settings_field, 1);
    }

    #[test]
    fn unknown_model_typed_in_the_picker_is_accepted() {
        let mut app = app_with_settings_open();
        app.set_catalog(picker_sample_catalog());
        app.settings_field = 1;
        app.open_picker(SettingsPicker::Provider);
        app.picker_query = "anthropic".into();
        app.picker_choose();
        app.picker_query = "mi-modelo-nuevo".into();
        app.picker_choose();
        assert_eq!(
            app.provider_settings.as_ref().unwrap().model,
            "mi-modelo-nuevo"
        );
        assert!(app.settings_picker.is_none());
    }

    #[test]
    fn provider_without_key_variable_keeps_the_current_one() {
        use codeloom_core::catalog::{Catalog, CatalogProvider};

        let mut app = app_with_settings_open();
        app.set_catalog(Arc::new(Catalog {
            providers: vec![CatalogProvider {
                id: "local".into(),
                name: "Local".into(),
                kind: codeloom_core::config::ProviderKind::OpenAiCompat,
                base_url: "http://localhost:11434/v1".into(),
                api_key_env: String::new(),
                models: Vec::new(),
            }],
        }));
        app.open_picker(SettingsPicker::Provider);
        app.picker_choose();
        let settings = app.provider_settings.as_ref().unwrap();
        assert_eq!(settings.base_url, "http://localhost:11434/v1");
        assert_eq!(settings.api_key_env, "API_KEY");
    }

    #[test]
    fn choosing_a_model_fills_the_model() {
        let mut app = anthropic_settings_open();
        app.open_picker(SettingsPicker::Model);
        let ids: Vec<String> = app.picker_items().into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["claude-sonnet", "claude-opus"]);
        app.picker_selected = 1;
        app.picker_choose();
        assert_eq!(app.provider_settings.as_ref().unwrap().model, "claude-opus");
    }

    #[test]
    fn picker_with_empty_catalog_sets_feedback() {
        let mut app = app_with_settings_open();
        app.open_picker(SettingsPicker::Provider);
        assert!(app.settings_picker.is_none());
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("Catalog not loaded yet; type it by hand")
        );
    }

    #[test]
    fn model_picker_without_matching_provider_sets_feedback() {
        // La URL de `settings_fixture` (example.test) no está en el catálogo de muestra.
        let mut app = app_with_settings_open();
        app.set_catalog(picker_sample_catalog());
        app.open_picker(SettingsPicker::Model);
        assert!(app.settings_picker.is_none());
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("No catalog provider matches the current URL")
        );
        assert!(!app.settings_dirty());
    }

    #[test]
    fn picker_changes_count_as_unsaved() {
        let mut app = anthropic_settings_open();
        app.open_picker(SettingsPicker::Model);
        app.picker_selected = 1;
        app.picker_choose();
        assert!(app.settings_dirty());
        assert_eq!(app.settings_change_count(), 1);
        assert!(
            !app.close_settings(),
            "elegir del selector cuenta como cambio sin guardar"
        );
        assert!(app.settings_open);
    }

    #[test]
    fn manual_edit_still_works_after_picking() {
        let mut app = anthropic_settings_open();
        app.open_picker(SettingsPicker::Model);
        app.picker_choose();
        app.settings_field = 1;
        app.edit_settings_value();
        app.settings_editor.set_text("model-manual");
        app.commit_settings_value();
        assert_eq!(
            app.provider_settings.as_ref().unwrap().model,
            "model-manual"
        );
    }

    /// Diálogo abierto sobre `model` del catálogo de razonamiento, sin esfuerzo.
    fn reasoning_app(model: &str) -> App {
        let mut app = App::new();
        app.set_catalog(reasoning_sample_catalog());
        app.set_provider_settings(ProviderSettings {
            kind: codeloom_core::config::ProviderKind::Anthropic,
            model: model.into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: None,
        });
        app.open_settings();
        app
    }

    fn effort_of(app: &App) -> Option<String> {
        app.provider_settings.as_ref().unwrap().effort.clone()
    }

    #[test]
    fn effort_options_follow_the_catalog_model() {
        let mut app = reasoning_app("claude-think");
        assert_eq!(app.effort_options(), ["low", "medium", "high"]);
        app.provider_settings.as_mut().unwrap().model = "claude-plain".into();
        assert!(app.effort_options().is_empty());
    }

    #[test]
    fn cycle_effort_walks_default_and_values() {
        let mut app = reasoning_app("claude-think");
        app.cycle_effort(true);
        assert_eq!(effort_of(&app).as_deref(), Some("low"));
        app.cycle_effort(true);
        app.cycle_effort(true);
        assert_eq!(effort_of(&app).as_deref(), Some("high"));
        app.cycle_effort(true);
        assert_eq!(effort_of(&app), None, "tras el último vuelve a «default»");
        app.cycle_effort(false);
        assert_eq!(effort_of(&app).as_deref(), Some("high"));
    }

    #[test]
    fn cycle_effort_does_nothing_without_options() {
        let mut app = reasoning_app("claude-plain");
        app.cycle_effort(true);
        assert_eq!(effort_of(&app), None);
        assert!(!app.settings_effort_visible());
    }

    #[test]
    fn effort_field_hidden_for_non_reasoning_model() {
        let mut app = reasoning_app("claude-plain");
        assert_eq!(app.settings_save_field(), SETTINGS_EFFORT_FIELD);
        app.provider_settings.as_mut().unwrap().model = "claude-think".into();
        assert!(app.settings_effort_visible());
        assert_eq!(app.settings_save_field(), SETTINGS_EFFORT_FIELD + 1);
    }

    #[test]
    fn changing_model_drops_unsupported_effort() {
        let mut app = reasoning_app("claude-think");
        app.provider_settings.as_mut().unwrap().effort = Some("high".into());
        app.open_picker(SettingsPicker::Model);
        app.picker_query = "mid".into();
        app.picker_choose();
        assert_eq!(effort_of(&app), None);
        assert_eq!(
            app.settings_feedback.as_deref(),
            Some("Effort reset: not supported by this model")
        );

        // Un nivel que el modelo nuevo sí admite se conserva.
        app.provider_settings.as_mut().unwrap().effort = Some("medium".into());
        app.open_picker(SettingsPicker::Model);
        app.picker_query = "think".into();
        app.picker_choose();
        assert_eq!(effort_of(&app).as_deref(), Some("medium"));
        assert_eq!(app.settings_feedback, None);

        // La edición manual del modelo también descarta el nivel no admitido.
        app.provider_settings.as_mut().unwrap().effort = Some("high".into());
        app.settings_field = 1;
        app.edit_settings_value();
        app.settings_editor.set_text("claude-mid");
        app.commit_settings_value();
        assert_eq!(effort_of(&app), None);
    }

    #[test]
    fn effort_change_counts_as_unsaved() {
        let mut app = reasoning_app("claude-think");
        app.cycle_effort(true);
        assert!(app.settings_dirty());
        assert!(app.settings_field_changed(SETTINGS_EFFORT_FIELD));
        assert_eq!(app.settings_change_count(), 1);
        assert!(
            !app.close_settings(),
            "el esfuerzo cuenta como cambio sin guardar"
        );
    }

    #[test]
    fn initial_settings_take_effort_from_config() {
        let config = CodeLoomConfig {
            provider: codeloom_core::config::ProviderConfig {
                kind: ProviderKind::Anthropic,
                base_url: "https://api.anthropic.com/v1".into(),
                api_key_env: "ANTHROPIC_API_KEY".into(),
            },
            model: "claude-think".into(),
            max_steps: 8,
            effort: Some("low".into()),
        };
        let settings = ProviderSettings::from(&config);
        assert_eq!(settings.effort.as_deref(), Some("low"));
    }
}
