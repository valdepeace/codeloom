pub mod background;
pub mod changes;
mod clear;
pub mod compact;
pub mod dispatch;
pub mod stop;

use crate::budget::{budget_crossings, BudgetConfig};
use crate::catalog::Catalog;
use crate::config::{CodeLoomConfig, ConfirmDelegation};
use crate::error::Result;
use crate::event::{Activation, EventBus, LiveEvent};
use crate::message::{ContentPart, FinishReason, Message, Role, Usage};
use crate::permission::PermissionService;
use crate::provider::{Provider, ProviderRequest, StreamChunk};
use crate::session::{SessionId, SessionStore, UsageRecord};
use crate::tools::spill::ToolSpill;
use crate::tools::{limit_bytes, ToolCtx, ToolRegistry, MAX_NOTICE_BYTES, MAX_TOOL_OUTPUT_BYTES};
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio_util::sync::CancellationToken;

/// Tope de la sección «Available agents» del prompt de sistema, que crece con el número de
/// agentes del registro y viaja en cada petición.
const MAX_AGENT_SECTION_BYTES: usize = 8_000;

/// Tope del prompt propio de un agente (el cuerpo de su perfil), que se suma al contexto.
const MAX_SYSTEM_EXTRA_BYTES: usize = 16_000;

/// Contenido que recibe el modelo para el resultado de una herramienta. Si supera el tope y hay
/// volcado, guarda el original y añade su ruta tras el recorte; la reserva de 256 bytes deja
/// margen al marcador para que el total siga dentro de `MAX_TOOL_OUTPUT_BYTES`. Si no hay
/// volcado o no se pudo escribir, se recorta sin ruta, como antes.
fn contenido_de_herramienta(spill: Option<&ToolSpill>, call_id: &str, content: &str) -> String {
    if content.len() > MAX_TOOL_OUTPUT_BYTES {
        if let Some(ruta) = spill.and_then(|spill| spill.save(call_id, content)) {
            let recorte = limit_bytes(content, MAX_TOOL_OUTPUT_BYTES - 256);
            return format!("{recorte}\n[full output saved to: {}]", ruta.display());
        }
    }
    crate::tools::limit_output(content).into_owned()
}

/// Instrucción final de la petición de resumen, después de los mensajes que se resumen.
const INSTRUCCION_DE_RESUMEN: &str =
    "Summarize the conversation above following your instructions.";

/// Bytes que se reservan para el marcador de omisión que `limit_bytes` añade al recortar.
const MARCADOR_DE_RECORTE: usize = 64;

pub struct TurnOutcome {
    pub session: SessionId,
    pub final_text: String,
    pub steps: usize,
}

/// Registrador del consumo de una respuesta. Lleva copias compartidas del almacén, la
/// configuración, el catálogo y el bus, de modo que es `'static` y puede moverse a una tarea
/// `tokio::spawn`. Su comportamiento es el del antiguo `AgentLoop::record_usage`.
#[derive(Clone)]
struct UsageRecorder {
    store: Arc<dyn SessionStore>,
    config: Arc<RwLock<CodeLoomConfig>>,
    catalog: Arc<RwLock<Catalog>>,
    budget: BudgetConfig,
    /// Sesión raíz cuya familia (ella y sus hijos) suma al presupuesto; `None` si es la propia.
    family: Option<SessionId>,
    bus: EventBus,
}

impl UsageRecorder {
    /// Guarda el apunte de consumo de una respuesta, con el modelo y la URL de la configuración
    /// vigente y los precios del catálogo en ese instante. Un fallo se registra y no corta el turno.
    async fn record(&self, session: SessionId, usage: &crate::message::Usage) {
        let (model, base_url) = {
            let config = self.config.read().unwrap();
            (config.model.clone(), config.provider.base_url.clone())
        };
        let prices = self
            .catalog
            .read()
            .ok()
            .and_then(|catalog| catalog.prices(&base_url, &model));
        let record = UsageRecord {
            session,
            model,
            input_tokens: u64::from(usage.input_tokens),
            output_tokens: u64::from(usage.output_tokens),
            input_price: prices.map(|(input, _)| input),
            output_price: prices.map(|(_, output)| output),
            created_at_ms: crate::message::now_ms(),
        };
        // Con presupuesto, el acumulado previo se lee antes de guardar el apunte para comparar
        // antes y después. Sin presupuesto no se toca el almacén más allá del apunte.
        let anteriores = if self.budget.is_empty() {
            None
        } else {
            match self
                .store
                .session_usage(self.family.unwrap_or(session))
                .await
            {
                Ok(registros) => Some(registros),
                Err(error) => {
                    tracing::warn!(error = %error, "no se pudo leer el consumo previo del presupuesto");
                    None
                }
            }
        };
        if let Err(error) = self.store.record_usage(session, &record).await {
            tracing::warn!(error = %error, "no se pudo guardar el apunte de consumo");
            return;
        }
        if let Some(anteriores) = anteriores {
            for aviso in budget_crossings(&self.budget, &anteriores, &record) {
                self.bus.publish(LiveEvent::BudgetWarning {
                    metric: aviso.metric,
                    level_percent: aviso.level_percent,
                    used: aviso.used,
                    limit: aviso.limit,
                    partial: aviso.partial,
                });
            }
        }
    }
}

pub struct AgentLoop {
    provider: Arc<RwLock<Arc<dyn Provider>>>,
    store: Arc<dyn SessionStore>,
    tools: Arc<ToolRegistry>,
    bus: EventBus,
    permission: Arc<PermissionService>,
    config: Arc<RwLock<CodeLoomConfig>>,
    cwd: PathBuf,
    changes: crate::agent::changes::ChangeTracker,
    jobs: Arc<dyn crate::job::JobRegistry>,
    subagents: Option<Arc<dyn crate::subagent::SubagentLauncher>>,
    /// Catálogo compartido de modelos y precios; vacío hasta que `with_catalog` lo sustituye.
    catalog: Arc<RwLock<Catalog>>,
    /// Prompt de sistema propio del agente (perfil). Se añade tras el contexto del proyecto;
    /// `None` o vacío no añade nada.
    system_extra: Option<String>,
    /// Registro de agentes compartido con los subagentes; `None` deja solo el agente `default`.
    registry: Option<Arc<crate::registry::AgentRegistry>>,
    /// Modo de confirmación al delegar; viaja a `RegistrySnapshot::prompt_section` en cada turno.
    delegation_confirm: ConfirmDelegation,
    /// Volcado a disco de las salidas recortadas; `None` no guarda nada y no añade ruta.
    spill: Option<ToolSpill>,
    remote_clients: Option<Arc<dyn crate::remote_task::RemoteClientFactory>>,
    /// Presupuesto de gasto de la sesión; vacío no avisa ni lee el consumo acumulado. Los
    /// subagentes lo reciben y suman al mismo total (véase `budget_family`).
    budget: BudgetConfig,
    /// Sesión raíz del presupuesto compartido. En un hijo, la del padre: su consumo y el de sus
    /// hermanos suman al mismo acumulado. `None` en el agente raíz, que cuenta con su propia sesión.
    budget_family: Option<SessionId>,
    /// Configuración de la compactación (`[compaction]`). Por defecto, sin segundo plano. Solo
    /// afecta a este agente: el lanzador de subagentes no lo hereda.
    compaction: compact::CompactionConfig,
    /// Estado por sesión de la compactación en segundo plano. Es privado: los subagentes no lo
    /// comparten.
    background: background::BackgroundSlots,
}

/// Genera en segundo plano el resumen del tramo antiguo y lo entrega a `finisher`. Es una tarea
/// propia con copias de lo que necesita: no toma prestado el agente ni el token de cancelación del
/// turno, así que cancelar el turno no mata el resumen. El consumo se apunta y se publica antes de
/// entregar, también si el resumen se descarta.
async fn resumir_en_segundo_plano(
    finisher: background::Finisher,
    provider: Arc<dyn Provider>,
    model: String,
    historial: Vec<Message>,
    recorder: UsageRecorder,
    bus: EventBus,
    session: SessionId,
) {
    let token = finisher.cancel_token();
    let resultado = generate_summary(provider, model, &historial, &token).await;
    // Solo un resumen entero y de una tarea no cancelada queda listo; todo lo demás se descarta.
    let utilizable = matches!(&resultado, Ok((Some(_), _))) && !token.is_cancelled();
    if let Ok((resumen, uso)) = &resultado {
        if let Some(uso) = uso {
            bus.publish(LiveEvent::CompactionUsage {
                input_tokens: uso.input_tokens,
                output_tokens: uso.output_tokens,
            });
            recorder.record(session, uso).await;
        }
        // Un resumen de una tarea cancelada no se anuncia: su slot ya se ha vaciado.
        if let (Some(resumen), false) = (resumen, token.is_cancelled()) {
            bus.publish(LiveEvent::BackgroundCompactionReady {
                summary_bytes: resumen.text.len(),
            });
        }
    }
    finisher.finish(resultado);
    if !utilizable {
        // Sin este aviso la interfaz seguiría mostrando «resumiendo…» de una tarea ya terminada.
        bus.publish(LiveEvent::BackgroundCompactionDiscarded);
    }
}

impl AgentLoop {
    pub fn new(
        provider: Arc<dyn Provider>,
        store: Arc<dyn SessionStore>,
        tools: Arc<ToolRegistry>,
        bus: EventBus,
        permission: Arc<PermissionService>,
        config: CodeLoomConfig,
        cwd: PathBuf,
    ) -> Self {
        let provider = Arc::new(RwLock::new(provider));
        let config = Arc::new(RwLock::new(config));
        let jobs: Arc<dyn crate::job::JobRegistry> =
            Arc::new(crate::job::LocalJobRegistry::default());
        let has_subagents = tools.get("subagent").is_some();
        let mut agent = Self {
            provider,
            store,
            tools,
            bus,
            permission,
            config,
            changes: crate::agent::changes::ChangeTracker::detect(cwd.clone()),
            jobs,
            subagents: None,
            catalog: Arc::new(RwLock::new(Catalog::default())),
            system_extra: None,
            registry: None,
            delegation_confirm: ConfirmDelegation::default(),
            spill: None,
            remote_clients: None,
            budget: BudgetConfig::default(),
            budget_family: None,
            compaction: compact::CompactionConfig::default(),
            background: background::BackgroundSlots::default(),
            cwd,
        };
        if has_subagents {
            agent.subagents = Some(agent.subagent_launcher());
        }
        agent
    }

    pub fn with_jobs(mut self, jobs: Arc<dyn crate::job::JobRegistry>) -> Self {
        self.jobs = jobs;
        self
    }

    /// Comparte el catálogo de modelos con el agente y con sus subagentes, que apuntan el
    /// precio vigente de cada respuesta. Sin catálogo, los apuntes se guardan con precio `NULL`.
    pub fn with_catalog(mut self, catalog: Arc<RwLock<Catalog>>) -> Self {
        self.catalog = catalog;
        // El lanzador guarda su propia copia del `Arc`: si ya existía, se reconstruye con el nuevo.
        if self.subagents.is_some() {
            self.subagents = Some(self.subagent_launcher());
        }
        self
    }

    /// Añade un prompt de sistema propio (el del perfil de un subagente) tras el contexto del
    /// proyecto. `None` deja el sistema tal cual, como antes de existir esta opción.
    pub fn with_system_extra(mut self, texto: Option<String>) -> Self {
        self.system_extra = texto;
        self
    }

    /// Comparte el registro de agentes con los subagentes que este agente lance. Como
    /// `with_catalog`, reconstruye el lanzador si ya existía.
    pub fn with_agent_registry(mut self, registry: Arc<crate::registry::AgentRegistry>) -> Self {
        self.registry = Some(registry);
        if self.subagents.is_some() {
            self.subagents = Some(self.subagent_launcher());
        }
        self
    }

    /// Fija cuándo pregunta la persona usuaria antes de delegar (`[agents] confirm_delegation`).
    /// Por defecto, `Doubt`. Solo cambia la regla del prompt; no garantiza el comportamiento.
    pub fn with_delegation_confirm(mut self, confirm: ConfirmDelegation) -> Self {
        self.delegation_confirm = confirm;
        self
    }

    /// Activa el volcado a disco de las salidas de herramienta que superan el tope. Sin él, el
    /// recorte no lleva ruta.
    pub fn with_spill(mut self, spill: ToolSpill) -> Self {
        self.spill = Some(spill);
        // Igual que `with_catalog`: si el lanzador ya existía, se reconstruye con el volcado.
        if self.subagents.is_some() {
            self.subagents = Some(self.subagent_launcher());
        }
        self
    }

    /// Fija la fábrica de clientes de agentes remotos (A8a). Sin ella, el lanzador rechaza los
    /// agentes remotos con un error claro. Los subagentes que lance la heredan.
    pub fn with_remote_clients(
        mut self,
        fabrica: Arc<dyn crate::remote_task::RemoteClientFactory>,
    ) -> Self {
        self.remote_clients = Some(fabrica);
        // Igual que `with_spill`: si el lanzador ya existía, se reconstruye con la fábrica.
        if self.subagents.is_some() {
            self.subagents = Some(self.subagent_launcher());
        }
        self
    }

    /// Fija el presupuesto de gasto de la sesión (`[budget]`). Sin él, o vacío, no hay avisos.
    /// Los subagentes que lance heredan el mismo presupuesto y suman al total de la familia.
    pub fn with_budget(mut self, budget: BudgetConfig) -> Self {
        self.budget = budget;
        // Igual que `with_catalog`: el lanzador guarda su copia y, si ya existía, se reconstruye.
        if self.subagents.is_some() {
            self.subagents = Some(self.subagent_launcher());
        }
        self
    }

    /// Hace que el consumo de este agente cuente en el acumulado de la familia de `root`: es lo que
    /// usa el lanzador para que un hijo y sus hermanos sumen al presupuesto del padre.
    pub fn with_budget_family(mut self, root: SessionId) -> Self {
        self.budget_family = Some(root);
        self
    }

    /// Fija la configuración de la compactación (`[compaction]`). Solo afecta a este agente: el
    /// lanzador de subagentes no lo hereda, así que un hijo usa siempre la vía síncrona.
    pub fn with_compaction(mut self, compaction: compact::CompactionConfig) -> Self {
        self.compaction = compaction;
        self
    }

    /// Prompt de sistema vigente: contexto del proyecto, lista de agentes y prompt propio, cada
    /// parte con su tope. Se calcula en cada turno, de modo que la recarga en caliente del registro
    /// también le llega al modelo.
    fn system_prompt(&self) -> String {
        let mut system = crate::config::context::load_context_files(&self.cwd).join("\n\n");
        // Con la herramienta `subagent` y un registro, el modelo recibe la lista vigente de agentes:
        // se lee en cada turno, de modo que la recarga en caliente también le llega.
        if let (Some(_), Some(registro)) = (&self.subagents, &self.registry) {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            let seccion = registro.snapshot().prompt_section(self.delegation_confirm);
            system.push_str(&limit_bytes(&seccion, MAX_AGENT_SECTION_BYTES));
        }
        // El prompt propio del agente va detrás del contexto del proyecto, sin sustituirlo.
        if let Some(extra) = self
            .system_extra
            .as_deref()
            .filter(|t| !t.trim().is_empty())
        {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(&limit_bytes(extra, MAX_SYSTEM_EXTRA_BYTES));
        }
        system
    }

    /// Resume ahora el historial antiguo de `session`, sin esperar al umbral (`/compact`). Usa la
    /// misma vía que la compactación automática: activa un resumen en segundo plano si lo hay y,
    /// si no, lo genera con el modelo. Publica siempre `ManualCompactionEnded`, también si falla,
    /// y no devuelve error: un fallo es un resultado, no una excepción. No debe llamarse con un
    /// turno en curso sobre la misma sesión; quien llama (la interfaz) lo garantiza.
    pub async fn compact_now(
        &self,
        session: SessionId,
        cancel: CancellationToken,
    ) -> crate::event::ManualCompaction {
        use crate::event::ManualCompaction;
        let outcome = match self.store.messages(session).await {
            Err(error) => {
                tracing::warn!(error = %error, "no se pudo leer el historial para compactar");
                ManualCompaction::Failed
            }
            Ok(history) if compact::choose_cut(&history).is_none() => {
                ManualCompaction::NothingToSummarize
            }
            Ok(history) => {
                let tokens_before =
                    compact::estimate_context_tokens(&history, self.system_prompt().len()) as u64;
                match self
                    .compact_or_activate(session, &history, tokens_before, &cancel)
                    .await
                {
                    Ok(true) => ManualCompaction::Compacted,
                    Ok(false) => ManualCompaction::Failed,
                    Err(error) => {
                        tracing::warn!(error = %error, "no se pudo resumir el historial");
                        ManualCompaction::Failed
                    }
                }
            }
        };
        self.bus
            .publish(LiveEvent::ManualCompactionEnded { outcome });
        outcome
    }

    /// Lanzador de subagentes con el estado actual de este agente, catálogo y registro incluidos.
    fn subagent_launcher(&self) -> Arc<dyn crate::subagent::SubagentLauncher> {
        let lanzador = crate::subagent::InProcessSubagentLauncher::new(
            self.provider.clone(),
            self.store.clone(),
            self.tools.clone(),
            self.permission.clone(),
            self.config.clone(),
            self.cwd.clone(),
            self.jobs.clone(),
            self.bus.clone(),
        )
        .with_catalog(self.catalog.clone())
        .with_budget(self.budget.clone());
        let lanzador = match &self.registry {
            Some(registro) => lanzador.with_registry(registro.clone()),
            None => lanzador,
        };
        let lanzador = match &self.spill {
            Some(spill) => lanzador.with_spill(spill.clone()),
            None => lanzador,
        };
        let lanzador = match &self.remote_clients {
            Some(fabrica) => lanzador.with_remote_clients(fabrica.clone()),
            None => lanzador,
        };
        Arc::new(lanzador)
    }

    /// Registrador de consumo con el estado actual de este agente. Es clonable y no toma prestado
    /// el agente, así que una tarea en segundo plano puede llevarlo consigo.
    fn recorder(&self) -> UsageRecorder {
        UsageRecorder {
            store: self.store.clone(),
            config: self.config.clone(),
            catalog: self.catalog.clone(),
            budget: self.budget.clone(),
            family: self.budget_family,
            bus: self.bus.clone(),
        }
    }

    /// Guarda el apunte de consumo de una respuesta. Véase `UsageRecorder::record`.
    async fn record_usage(&self, session: SessionId, usage: &crate::message::Usage) {
        self.recorder().record(session, usage).await;
    }

    pub fn jobs(&self) -> Arc<dyn crate::job::JobRegistry> {
        self.jobs.clone()
    }

    pub fn update_provider(&self, provider: Arc<dyn Provider>, config: CodeLoomConfig) {
        *self.provider.write().unwrap() = provider;
        *self.config.write().unwrap() = config;
    }

    /// Turno iniciado por un mensaje del usuario.
    pub async fn run(
        &self,
        session: SessionId,
        user_prompt: String,
        cancel: CancellationToken,
    ) -> Result<TurnOutcome> {
        self.bus.publish(LiveEvent::TurnStarted);
        let result = match self.flush_subagent_notices(session).await {
            Ok(_) => self.run_turn(session, Some(user_prompt), cancel).await,
            Err(error) => Err(error),
        };
        self.publish_turn_result(&result);
        result
    }

    /// Despertar del padre ocioso: entrega los avisos pendientes y, solo si había
    /// alguno, ejecuta un turno sin mensaje de usuario. Sin avisos devuelve `None`
    /// y publica `WakeIdle`, sin turno ni gasto alguno.
    pub async fn run_woken(
        &self,
        session: SessionId,
        cancel: CancellationToken,
    ) -> Result<Option<TurnOutcome>> {
        let delivered = match self.flush_subagent_notices(session).await {
            Ok(delivered) => delivered,
            Err(error) => {
                self.bus.publish(LiveEvent::Error {
                    message: error.to_string(),
                });
                return Err(error);
            }
        };
        if delivered == 0 {
            self.bus.publish(LiveEvent::WakeIdle);
            return Ok(None);
        }
        self.bus.publish(LiveEvent::TurnStarted);
        let result = self.run_turn(session, None, cancel).await;
        self.publish_turn_result(&result);
        result.map(Some)
    }

    /// Publica el desenlace de un turno ya ejecutado (fin o error).
    fn publish_turn_result(&self, result: &Result<TurnOutcome>) {
        match result {
            Ok(_) => self.bus.publish(LiveEvent::TurnEnded),
            Err(error) => self.bus.publish(LiveEvent::Error {
                message: error.to_string(),
            }),
        }
    }

    /// Ejecuta el turno. Los avisos pendientes los entrega el llamante antes de
    /// llamar aquí; el bucle solo los vuelve a vaciar entre pasos.
    async fn run_turn(
        &self,
        session: SessionId,
        user_prompt: Option<String>,
        cancel: CancellationToken,
    ) -> Result<TurnOutcome> {
        let provider = self.provider.read().unwrap().clone();
        let config = self.config.read().unwrap().clone();
        let mut history = self.store.messages(session).await?;
        if let Some(prompt) = user_prompt {
            let user = Message::user(prompt);
            self.store.append_message(session, &user).await?;
            self.bus.publish(LiveEvent::MessageAdded(user.clone()));
            history.push(user);
        }

        let system = self.system_prompt();
        let ctx = ToolCtx {
            cwd: self.cwd.clone(),
            permission: self.permission.clone(),
            session,
            jobs: self.jobs.clone(),
            bus: self.bus.clone(),
            subagents: self.subagents.clone(),
        };
        let mut steps = 0usize;
        let mut final_text = String::new();
        let mut compactions = 0usize;
        let mut compaction_failed = false;

        loop {
            if cancel.is_cancelled() {
                break;
            }
            self.flush_subagent_notices(session).await?;
            history = self.store.messages(session).await?;
            if crate::agent::stop::should_stop(&history, steps, config.max_steps) {
                break;
            }
            // Si el contexto estimado alcanza la fracción de la ventana, se resume lo antiguo antes
            // de construir la petición. Sin ventana conocida no se compacta. Un fallo no aborta el
            // turno ni se reintenta en él.
            let window = self
                .catalog
                .read()
                .unwrap()
                .context_window(&config.provider.base_url, &config.model);
            if let Some(window) = window {
                // Con segundo plano, al alcanzar el búfer se lanza el resumen y el turno sigue sin
                // esperarlo. Solo si la sesión no tiene ya una tarea en curso o un resumen listo.
                if self.compaction.background
                    && compactions < compact::MAX_COMPACTIONS_PER_TURN
                    && !compaction_failed
                    && background::should_start_background(&history, system.len(), window)
                {
                    let tokens_before =
                        compact::estimate_context_tokens(&history, system.len()) as u64;
                    if let Some(finisher) = self
                        .background
                        .try_start(session, background::current_base(&history))
                    {
                        self.bus
                            .publish(LiveEvent::BackgroundCompactionStarted { tokens_before });
                        tokio::spawn(resumir_en_segundo_plano(
                            finisher,
                            provider.clone(),
                            config.model.clone(),
                            history.clone(),
                            self.recorder(),
                            self.bus.clone(),
                            session,
                        ));
                    }
                }
                if compactions < compact::MAX_COMPACTIONS_PER_TURN
                    && !compaction_failed
                    && compact::should_compact(&history, system.len(), window)
                    // Sin corte posible aún no hay nada que resumir: no es un fallo y no debe
                    // apagar la compactación, porque más adelante en el turno sí puede haberlo.
                    && compact::choose_cut(&history).is_some()
                {
                    let tokens_before =
                        compact::estimate_context_tokens(&history, system.len()) as u64;
                    match self
                        .compact_or_activate(session, &history, tokens_before, &cancel)
                        .await
                    {
                        Ok(true) => {
                            compactions += 1;
                            history = self.store.messages(session).await?;
                        }
                        Ok(false) => compaction_failed = true,
                        Err(error) => {
                            tracing::warn!(error = %error, "no se pudo resumir el historial; el turno continúa sin compactar");
                            compaction_failed = true;
                        }
                    }
                    if cancel.is_cancelled() {
                        break;
                    }
                }
            }
            steps += 1;
            self.bus.publish(LiveEvent::StepStarted {
                step: steps,
                max_steps: config.max_steps,
            });
            let req = ProviderRequest {
                model: config.model.clone(),
                system: system.clone(),
                messages: clear::clear_old_tool_results(
                    &compact::effective_history(&history),
                    clear::KEEP_TOOL_RESULTS,
                ),
                tools: self.tools.specs(),
                temperature: 0.0,
                max_tokens: 4096,
                effort: config.effort.clone(),
            };
            let abierto = tokio::select! {
                _ = cancel.cancelled() => break,
                result = provider.stream(req, cancel.clone()) => result,
            };
            let mut stream = match abierto {
                Ok(stream) => stream,
                // Red de seguridad de la compactación: la estimación local pudo quedarse corta y el
                // proveedor rechaza la petición por exceder la ventana. Se resume por fuerza y se
                // reintenta, con el mismo tope de compactaciones por turno que la vía normal, de
                // modo que el bucle no puede ser infinito. Si no hay corte posible, o el resumen
                // falla, se devuelve el error original.
                Err(error)
                    if compact::is_context_overflow(&error)
                        && compactions < compact::MAX_COMPACTIONS_PER_TURN
                        && !compaction_failed
                        && compact::choose_cut(&history).is_some() =>
                {
                    tracing::warn!(error = %error, "el proveedor rechazó el contexto por exceso; se compacta y se reintenta");
                    let tokens_before =
                        compact::estimate_context_tokens(&history, system.len()) as u64;
                    match self
                        .compact_or_activate(session, &history, tokens_before, &cancel)
                        .await
                    {
                        Ok(true) => {
                            compactions += 1;
                            continue;
                        }
                        Ok(false) if cancel.is_cancelled() => break,
                        Ok(false) | Err(_) => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            };
            let mut text_buf = String::new();
            let mut tool_calls: Vec<(String, String, serde_json::Value)> = Vec::new();
            let mut usage: Option<Usage> = None;
            let mut finish = FinishReason::Stop;

            while let Some(chunk) = tokio::select! {
                _ = cancel.cancelled() => None,
                chunk = stream.next() => chunk,
            } {
                match chunk {
                    StreamChunk::TextDelta(t) => {
                        text_buf.push_str(&t);
                        self.bus.publish(LiveEvent::Delta { text: t });
                    }
                    StreamChunk::ReasoningDelta(r) => {
                        self.bus.publish(LiveEvent::ReasoningDelta { text: r });
                    }
                    StreamChunk::Usage(chunk) => {
                        let total = usage.get_or_insert(Usage {
                            input_tokens: 0,
                            output_tokens: 0,
                        });
                        total.input_tokens = total.input_tokens.saturating_add(chunk.input_tokens);
                        total.output_tokens =
                            total.output_tokens.saturating_add(chunk.output_tokens);
                        self.bus.publish(LiveEvent::Usage {
                            input_tokens: chunk.input_tokens,
                            output_tokens: chunk.output_tokens,
                        });
                    }
                    StreamChunk::ToolCallComplete { id, name, input } => {
                        tool_calls.push((id, name, input));
                    }
                    StreamChunk::Done(r) => finish = r,
                    _ => {}
                }
            }

            // Un apunte por respuesta: el proveedor puede repartir su uso en varios fragmentos,
            // así que se guarda el total acumulado al cerrar el flujo.
            if let Some(total) = &usage {
                self.record_usage(session, total).await;
            }

            if cancel.is_cancelled() {
                finish = FinishReason::Error;
            }

            let mut parts = Vec::new();
            if !text_buf.is_empty() {
                parts.push(ContentPart::Text {
                    text: text_buf.clone(),
                });
            }
            for (id, name, input) in &tool_calls {
                parts.push(ContentPart::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            parts.push(ContentPart::Finish { reason: finish });
            let mut assistant = Message::assistant(parts);
            assistant.usage = usage;
            self.store.append_message(session, &assistant).await?;
            self.bus.publish(LiveEvent::MessageAdded(assistant.clone()));
            history.push(assistant);
            final_text = text_buf;

            if cancel.is_cancelled() {
                break;
            }

            if tool_calls.is_empty() {
                break;
            }

            let mut allowed = Vec::new();
            for (id, name, input) in tool_calls {
                let req = crate::permission::PermissionRequest {
                    tool: name.clone(),
                    action: "run".into(),
                    detail: input.to_string(),
                };
                match self.permission.check(req).await {
                    crate::permission::Decision::Allow
                    | crate::permission::Decision::AllowSession => allowed.push((id, name, input)),
                    crate::permission::Decision::Deny => {
                        let tr = Message::new(
                            Role::Tool,
                            vec![ContentPart::ToolResult {
                                call_id: id,
                                content: "permission denied".into(),
                                is_error: true,
                            }],
                        );
                        self.store.append_message(session, &tr).await?;
                        self.bus.publish(LiveEvent::MessageAdded(tr.clone()));
                        history.push(tr);
                    }
                }
            }
            let mut by_id: std::collections::HashMap<String, (String, serde_json::Value)> =
                std::collections::HashMap::new();
            for (id, name, input) in &allowed {
                by_id.insert(id.clone(), (name.clone(), input.clone()));
            }

            let results = dispatch::execute_tools(
                &self.tools,
                ctx.clone(),
                allowed,
                cancel.clone(),
                self.bus.clone(),
            )
            .await;
            for (id, out) in results {
                if !out.is_error {
                    if let Some((name, input)) = by_id.get(&id) {
                        if name == "edit" {
                            let path = input["path"].as_str().unwrap_or_default();
                            let old = input["old"].as_str().unwrap_or_default();
                            let new = input["new"].as_str().unwrap_or_default();
                            let (added, removed) = self.changes.numbers(path, old, new);
                            self.bus.publish(LiveEvent::FileDiff {
                                path: path.to_string(),
                                added,
                                removed,
                            });
                        }
                    }
                }
                let contenido = contenido_de_herramienta(self.spill.as_ref(), &id, &out.content);
                let tr = Message::new(
                    Role::Tool,
                    vec![ContentPart::ToolResult {
                        call_id: id,
                        content: contenido,
                        is_error: out.is_error,
                    }],
                );
                self.store.append_message(session, &tr).await?;
                self.bus.publish(LiveEvent::MessageAdded(tr.clone()));
                history.push(tr);
            }
        }

        Ok(TurnOutcome {
            session,
            final_text,
            steps,
        })
    }

    /// Resume lo antiguo del historial de `session` y lo persiste como mensaje de resumen. Devuelve
    /// `Ok(true)` si compactó, y `Ok(false)` si no había nada que resumir, si el modelo devolvió un
    /// resumen vacío o si se canceló (en cuyo caso no se persiste nada). Un fallo del proveedor se
    /// propaga como `Err`. El consumo de la llamada de resumen se apunta y se publica como
    /// `CompactionUsage` aunque el resumen se descarte.
    async fn compact(
        &self,
        session: SessionId,
        history: &[Message],
        tokens_before: u64,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        let provider = self.provider.read().unwrap().clone();
        let model = self.config.read().unwrap().model.clone();
        let (resumen, uso) = generate_summary(provider, model, history, cancel).await?;

        // La llamada de resumen cuesta lo mismo que cualquier otra, se acepte el resumen o no:
        // el consumo se apunta siempre para que el coste de compactar sea visible. Se publica como
        // `CompactionUsage` para que no mueva el medidor de contexto.
        if let Some(uso) = &uso {
            self.bus.publish(LiveEvent::CompactionUsage {
                input_tokens: uso.input_tokens,
                output_tokens: uso.output_tokens,
            });
            self.record_usage(session, uso).await;
        }
        let Some(resumen) = resumen else {
            return Ok(false);
        };

        self.activate(session, &resumen, tokens_before, Activation::Sync)
            .await?;
        Ok(true)
    }

    /// Compacta el historial, o activa el resumen en segundo plano si lo hay. Sin
    /// `[compaction] background` es exactamente `compact`. Con él: un resumen listo y vigente se
    /// activa sin llamar al modelo; uno en curso se espera como mucho `BACKGROUND_WAIT_SECS`, y
    /// si no llega a tiempo, o no sirve, se cae a la vía síncrona. Devuelve `Ok(true)` si el
    /// historial cambió y `Ok(false)` si no se compactó porque no hubo resumen o se canceló el
    /// turno. Cancelar el turno no aborta la tarea en curso: sigue para el próximo turno.
    async fn compact_or_activate(
        &self,
        session: SessionId,
        history: &[Message],
        tokens_before: u64,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        if !self.compaction.background {
            return self.compact(session, history, tokens_before, cancel).await;
        }
        match self.background.state(session) {
            background::SlotState::Ready => {
                let vigente = self
                    .background
                    .take_ready(session, background::current_base(history));
                if let Some(pendiente) = vigente {
                    self.activate(
                        session,
                        &pendiente.summary,
                        tokens_before,
                        Activation::Background,
                    )
                    .await?;
                    return Ok(true);
                }
                // Resumen listo pero caduco: se tira y la interfaz deja de marcarlo como pendiente.
                self.bus.publish(LiveEvent::BackgroundCompactionDiscarded);
            }
            background::SlotState::Running => {
                let t0 = tokio::time::Instant::now();
                let espera = tokio::select! {
                    _ = cancel.cancelled() => return Ok(false),
                    resultado = self.background.wait(
                        session,
                        std::time::Duration::from_secs(background::BACKGROUND_WAIT_SECS),
                    ) => resultado,
                };
                if let background::WaitOutcome::Ready(pendiente) = espera {
                    if pendiente.base == background::current_base(history) {
                        let waited_ms = t0.elapsed().as_millis() as u64;
                        self.activate(
                            session,
                            &pendiente.summary,
                            tokens_before,
                            Activation::BackgroundAwaited { waited_ms },
                        )
                        .await?;
                        return Ok(true);
                    }
                    // Esperado, pero de otra base: caduco, se descarta igual que arriba.
                    self.bus.publish(LiveEvent::BackgroundCompactionDiscarded);
                }
            }
            background::SlotState::Idle => {}
        }
        // Sin resumen utilizable: la vía síncrona de siempre.
        self.compact(session, history, tokens_before, cancel).await
    }

    /// Activa un resumen ya generado: lo persiste como mensaje de resumen y publica `MessageAdded`
    /// y `ContextCompacted`. No hace llamadas al modelo. `activation` indica cómo entra en efecto.
    async fn activate(
        &self,
        session: SessionId,
        resumen: &GeneratedSummary,
        tokens_before: u64,
        activation: Activation,
    ) -> Result<()> {
        let mensaje = compact::summary_message(resumen.covers, &resumen.text);
        self.store.append_message(session, &mensaje).await?;
        self.bus.publish(LiveEvent::MessageAdded(mensaje));
        self.bus.publish(LiveEvent::ContextCompacted {
            replaced: resumen.covers,
            tokens_before,
            summary_bytes: resumen.text.len(),
            activation,
        });
        Ok(())
    }

    /// Entrega los avisos pendientes de jobs y subagentes. Devuelve cuántos se
    /// entregaron en esta llamada; cada aviso sale una sola vez.
    async fn flush_subagent_notices(&self, session: SessionId) -> Result<usize> {
        let mut delivered = 0usize;
        for job in self.jobs.pending_settled_notices(session) {
            let status = match job.status {
                crate::job::JobStatus::Completed => "completed",
                crate::job::JobStatus::Killed => "killed",
                crate::job::JobStatus::Failed => "failed",
                _ => continue,
            };
            let detail = job
                .detail
                .or_else(|| job.result.map(|result| result.content));
            let message = Message::user(format!(
                "[Job {} {status}] {}{}",
                job.id.0,
                job.label,
                detail
                    .map(|text| format!("\n{}", limit_bytes(&text, MAX_NOTICE_BYTES)))
                    .unwrap_or_default()
            ));
            self.store.append_message(session, &message).await?;
            self.jobs.mark_notice_delivered(session, &job.id)?;
            self.bus
                .publish(LiveEvent::SessionMessageAdded { session, message });
            delivered += 1;
        }
        if let Some(subagents) = &self.subagents {
            for message in subagents.deliver_notices(session).await? {
                self.bus
                    .publish(LiveEvent::SessionMessageAdded { session, message });
                delivered += 1;
            }
            // Las preguntas de los hijos se guardan aquí, a diferencia de los avisos
            // de finalización, que el propio lanzador persiste al entregarlos.
            for message in subagents.pending_input_notices(session).await? {
                self.store.append_message(session, &message).await?;
                self.bus
                    .publish(LiveEvent::SessionMessageAdded { session, message });
                delivered += 1;
            }
        }
        Ok(delivered)
    }
}

/// Resumen del tramo antiguo del historial, aún sin activar. `covers` es el corte: los mensajes
/// anteriores a él quedan cubiertos por `text`. `usage` es el consumo de la llamada de resumen.
#[derive(Debug, Clone)]
pub struct GeneratedSummary {
    pub text: String,
    pub usage: Option<Usage>,
    pub covers: usize,
}

/// Pide al modelo el resumen del tramo antiguo de `history`. No persiste nada ni publica eventos:
/// el llamador decide qué hacer con el texto y con el consumo.
///
/// Devuelve `(resumen, consumo)`. El consumo va aparte porque se apunta siempre, también cuando el
/// resumen se descarta. Por eso `resumen` es `None` si no hay corte, si se cancela, si el flujo no
/// cierra bien o si el texto queda vacío, mientras que `consumo` es `Some` en cuanto la llamada
/// llegó a consumir tokens.
pub async fn generate_summary(
    provider: Arc<dyn Provider>,
    model: String,
    history: &[Message],
    cancel: &CancellationToken,
) -> Result<(Option<GeneratedSummary>, Option<Usage>)> {
    let Some(cut) = compact::choose_cut(history) else {
        return Ok((None, None));
    };

    // Lo que se resume: lo efectivo anterior al corte (incluido el resumen previo, si lo hay).
    let anteriores: Vec<Message> = compact::effective_indices(history)
        .into_iter()
        .filter(|&i| i < cut)
        .map(|i| history[i].clone())
        .collect();
    let mut mensajes = clear::clear_old_tool_results(&anteriores, clear::KEEP_TOOL_RESULTS);
    mensajes.push(Message::user(INSTRUCCION_DE_RESUMEN));

    let req = ProviderRequest {
        model,
        system: compact::SUMMARY_SYSTEM_PROMPT.to_string(),
        messages: mensajes,
        tools: vec![],
        temperature: 0.0,
        max_tokens: compact::SUMMARY_MAX_TOKENS,
        effort: None,
    };
    let mut stream = tokio::select! {
        _ = cancel.cancelled() => return Ok((None, None)),
        result = provider.stream(req, cancel.clone()) => result?,
    };
    let mut recogido = String::new();
    // Un flujo que acaba en error o sin `Done` ha podido quedarse a medias: un resumen
    // truncado por un fallo se descarta en lugar de presentarse como bueno.
    let mut cierre: Option<FinishReason> = None;
    let mut uso: Option<Usage> = None;
    loop {
        let chunk = tokio::select! {
            _ = cancel.cancelled() => break,
            chunk = stream.next() => chunk,
        };
        match chunk {
            Some(StreamChunk::TextDelta(t)) => recogido.push_str(&t),
            Some(StreamChunk::Usage(parcial)) => {
                let total = uso.get_or_insert(Usage {
                    input_tokens: 0,
                    output_tokens: 0,
                });
                total.input_tokens = total.input_tokens.saturating_add(parcial.input_tokens);
                total.output_tokens = total.output_tokens.saturating_add(parcial.output_tokens);
            }
            Some(StreamChunk::Done(razon)) => {
                cierre = Some(razon);
                break;
            }
            None => break,
            Some(_) => {}
        }
    }
    if cancel.is_cancelled() {
        return Ok((None, uso));
    }
    if matches!(cierre, None | Some(FinishReason::Error)) {
        tracing::warn!("el flujo del resumen terminó sin cierre válido; se descarta");
        return Ok((None, uso));
    }

    let recogido = recogido.trim();
    if recogido.is_empty() {
        return Ok((None, uso));
    }
    // El marcador de `limit_bytes` se suma al texto conservado: se le reserva hueco para que
    // el resumen entero respete `MAX_SUMMARY_BYTES`.
    let texto = limit_bytes(
        recogido,
        compact::MAX_SUMMARY_BYTES.saturating_sub(MARCADOR_DE_RECORTE),
    )
    .into_owned();
    let resumen = GeneratedSummary {
        text: texto,
        usage: uso.clone(),
        covers: cut,
    };
    Ok((Some(resumen), uso))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Capabilities, ProviderStream, StreamChunk};
    use crate::session::sqlite::SqliteStore;
    use crate::tools::{ExecutionMode, Tool, ToolCtx, ToolOutput};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::Duration;
    use tokio::sync::{broadcast, mpsc, Semaphore};

    /// Proveedor que anota el esfuerzo de cada petición que recibe.
    struct EffortSpy {
        seen: Mutex<Vec<Option<String>>>,
    }

    #[async_trait]
    impl Provider for EffortSpy {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: false,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            self.seen.lock().unwrap().push(req.effort.clone());
            Ok(Box::pin(futures::stream::iter(vec![StreamChunk::Done(
                FinishReason::Stop,
            )])))
        }
    }

    #[tokio::test]
    async fn agent_passes_config_effort_to_the_request() {
        let provider = Arc::new(EffortSpy {
            seen: Mutex::new(Vec::new()),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("e.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: Some("high".into()),
        };
        let agent = AgentLoop::new(
            provider.clone(),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            provider.seen.lock().unwrap().as_slice(),
            &[Some("high".to_string())]
        );
    }

    struct QueueProvider {
        scripts: Mutex<VecDeque<Vec<StreamChunk>>>,
    }

    #[async_trait]
    impl Provider for QueueProvider {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            _req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(futures::stream::iter(script)))
        }
    }

    struct Noop;

    #[async_trait]
    impl Tool for Noop {
        fn name(&self) -> &str {
            "noop"
        }
        fn description(&self) -> &str {
            "n"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
            ExecutionMode::Exclusive
        }
        async fn run(
            &self,
            _c: ToolCtx,
            _i: serde_json::Value,
            _x: CancellationToken,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::ok("ran"))
        }
    }

    #[tokio::test]
    async fn runs_tool_then_finishes() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "noop".into(),
                    input: serde_json::json!({}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("done".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("l.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();

        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(Noop));

        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );

        let outcome = agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.steps, 2);
        assert_eq!(outcome.final_text, "done");
        let msgs = store.messages(sid).await.unwrap();
        assert!(msgs.iter().any(|m| m.role == Role::Tool));
    }

    #[tokio::test]
    async fn denied_tool_stops_and_is_recorded() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "noop".into(),
                    input: serde_json::json!({}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("after".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("d.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(Noop));
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(crate::permission::PermissionService::with_responder(
                Box::new(crate::permission::DenyAll),
            )),
            cfg,
            dir.path().to_path_buf(),
        );
        let _ = agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let msgs = store.messages(sid).await.unwrap();
        assert!(msgs.iter().any(|m| m
            .parts
            .iter()
            .any(|p| matches!(p, ContentPart::ToolResult { is_error: true, .. }))));
    }

    #[tokio::test]
    async fn cancelled_turn_marks_partial_message_as_error() {
        struct CancelMidStream;

        #[async_trait]
        impl Provider for CancelMidStream {
            fn capabilities(&self) -> Capabilities {
                Capabilities {
                    tool_use: false,
                    reasoning: false,
                    streaming: true,
                }
            }

            async fn stream(
                &self,
                _req: ProviderRequest,
                cancel: CancellationToken,
            ) -> Result<ProviderStream> {
                Ok(Box::pin(futures::stream::unfold(
                    (false, cancel),
                    |(sent_delta, cancel)| async move {
                        if sent_delta {
                            tokio::task::yield_now().await;
                            cancel.cancel();
                            None
                        } else {
                            Some((StreamChunk::TextDelta("partial".into()), (true, cancel)))
                        }
                    },
                )))
            }
        }

        let provider = Arc::new(CancelMidStream);
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("c.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );
        let _ = agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let msgs = store.messages(sid).await.unwrap();
        assert!(msgs.iter().any(|m| m.parts.iter().any(|p| matches!(
            p,
            ContentPart::Finish {
                reason: FinishReason::Error
            }
        ))));
    }

    #[tokio::test]
    async fn edit_publishes_a_file_diff() {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "edit".into(),
                    input: serde_json::json!({"path":"a.txt","old":"world","new":"rust"}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("done".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "hello world").unwrap();

        let store = Arc::new(
            SqliteStore::open(dir.path().join("l.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();

        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::edit::EditTool));

        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            bus,
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );

        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let mut found = None;
        while let Ok(ev) = rx.try_recv() {
            if let LiveEvent::FileDiff {
                path,
                added,
                removed,
            } = ev
            {
                found = Some((path, added, removed));
            }
        }
        assert_eq!(found, Some(("a.txt".to_string(), 1, 1)));
    }

    /// Lanzador de prueba: entrega cada aviso de pregunta una sola vez, como el real.
    struct InputNoticeLauncher {
        notices: std::sync::Mutex<Vec<Message>>,
    }

    #[async_trait]
    impl crate::subagent::SubagentLauncher for InputNoticeLauncher {
        async fn start(
            &self,
            _parent: SessionId,
            _label: String,
            _prompt: String,
            _agent: Option<String>,
            _cancel: CancellationToken,
        ) -> Result<(SessionId, crate::job::JobId)> {
            Err(crate::error::CoreError::Tool("not used".into()))
        }

        async fn control(
            &self,
            _parent: SessionId,
            _child: SessionId,
            _action: &str,
            _message: Option<String>,
        ) -> Result<()> {
            Err(crate::error::CoreError::Tool("not used".into()))
        }

        async fn list(
            &self,
            _parent: SessionId,
        ) -> Result<Vec<crate::session::SubagentDescriptor>> {
            Ok(Vec::new())
        }

        async fn deliver_notices(&self, _parent: SessionId) -> Result<Vec<Message>> {
            Ok(Vec::new())
        }

        async fn pending_input_notices(&self, _parent: SessionId) -> Result<Vec<Message>> {
            Ok(std::mem::take(&mut *self.notices.lock().unwrap()))
        }
    }

    /// Construye un agente de prueba con un proveedor guionizado y un bus propio.
    fn woken_agent(
        dir: &std::path::Path,
        store: Arc<SqliteStore>,
        scripts: Vec<Vec<StreamChunk>>,
        bus: EventBus,
    ) -> AgentLoop {
        AgentLoop::new(
            Arc::new(QueueProvider {
                scripts: Mutex::new(scripts.into()),
            }),
            store,
            Arc::new(ToolRegistry::new()),
            bus,
            Arc::new(PermissionService::allow_all()),
            CodeLoomConfig {
                provider: crate::config::ProviderConfig {
                    kind: crate::config::ProviderKind::OpenAiCompat,
                    base_url: "http://x".into(),
                    api_key_env: "K".into(),
                },
                model: "m".into(),
                max_steps: 5,
                effort: None,
            },
            dir.to_path_buf(),
        )
    }

    /// Vacía el bus y devuelve los eventos recibidos hasta ese momento.
    fn drain(rx: &mut tokio::sync::broadcast::Receiver<LiveEvent>) -> Vec<LiveEvent> {
        let mut out = Vec::new();
        while let Ok(event) = rx.try_recv() {
            out.push(event);
        }
        out
    }

    #[tokio::test]
    async fn run_woken_without_notices_runs_no_turn_and_emits_wake_idle() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("idle.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        // Si el proveedor llegara a consultarse, dejaría un mensaje del asistente.
        let agent = woken_agent(
            dir.path(),
            store.clone(),
            vec![vec![
                StreamChunk::TextDelta("no debería".into()),
                StreamChunk::Done(FinishReason::Stop),
            ]],
            bus,
        );

        let outcome = agent
            .run_woken(sid, CancellationToken::new())
            .await
            .unwrap();

        assert!(outcome.is_none());
        assert!(store.messages(sid).await.unwrap().is_empty());
        let events = drain(&mut rx);
        assert!(!events
            .iter()
            .any(|event| matches!(event, LiveEvent::TurnStarted | LiveEvent::TurnEnded)));
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, LiveEvent::WakeIdle))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn run_woken_with_a_notice_runs_a_turn_without_inventing_a_user_message() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("woken.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let mut agent = woken_agent(
            dir.path(),
            store.clone(),
            vec![vec![
                StreamChunk::TextDelta("hecho".into()),
                StreamChunk::Done(FinishReason::Stop),
            ]],
            bus,
        );
        let notice = Message::user("[Subagent obrero needs input]\n1. ¿Cuál?");
        agent.subagents = Some(Arc::new(InputNoticeLauncher {
            notices: std::sync::Mutex::new(vec![notice.clone()]),
        }));

        let outcome = agent
            .run_woken(sid, CancellationToken::new())
            .await
            .unwrap()
            .expect("con un aviso debe ejecutarse un turno");

        assert_eq!(outcome.final_text, "hecho");
        let stored = store.messages(sid).await.unwrap();
        let user_texts: Vec<String> = stored
            .iter()
            .filter(|message| message.role == Role::User)
            .map(|message| message.text())
            .collect();
        assert_eq!(user_texts, vec![notice.text()]);
        let events = drain(&mut rx);
        assert!(events
            .iter()
            .any(|event| matches!(event, LiveEvent::TurnStarted)));
        assert!(events
            .iter()
            .any(|event| matches!(event, LiveEvent::TurnEnded)));
        assert!(!events
            .iter()
            .any(|event| matches!(event, LiveEvent::WakeIdle)));
    }

    #[tokio::test]
    async fn flush_subagent_notices_injects_input_notices() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("n.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let mut agent = AgentLoop::new(
            Arc::new(QueueProvider {
                scripts: Mutex::new(VecDeque::new()),
            }),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            bus,
            Arc::new(PermissionService::allow_all()),
            CodeLoomConfig {
                provider: crate::config::ProviderConfig {
                    kind: crate::config::ProviderKind::OpenAiCompat,
                    base_url: "http://x".into(),
                    api_key_env: "K".into(),
                },
                model: "m".into(),
                max_steps: 5,
                effort: None,
            },
            dir.path().to_path_buf(),
        );
        let notice = Message::user(
            "[Subagent obrero needs input]
1. ¿Cuál?",
        );
        agent.subagents = Some(Arc::new(InputNoticeLauncher {
            notices: std::sync::Mutex::new(vec![notice.clone()]),
        }));

        assert_eq!(agent.flush_subagent_notices(sid).await.unwrap(), 1);

        let stored = store.messages(sid).await.unwrap();
        assert!(stored.iter().any(|m| m.text() == notice.text()));
        let mut published = false;
        while let Ok(event) = rx.try_recv() {
            if let LiveEvent::SessionMessageAdded { session, message } = event {
                published |= session == sid && message.text() == notice.text();
            }
        }
        assert!(
            published,
            "el aviso debería publicarse como SessionMessageAdded"
        );
    }

    #[tokio::test]
    async fn job_notice_detail_is_capped() {
        use crate::job::{JobRegistry, JobSpec, LocalJobRegistry};
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("tope_job.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let jobs = Arc::new(LocalJobRegistry::default());
        let agent = AgentLoop::new(
            Arc::new(QueueProvider {
                scripts: Mutex::new(VecDeque::new()),
            }),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            CodeLoomConfig {
                provider: crate::config::ProviderConfig {
                    kind: crate::config::ProviderKind::OpenAiCompat,
                    base_url: "http://x".into(),
                    api_key_env: "K".into(),
                },
                model: "m".into(),
                max_steps: 5,
                effort: None,
            },
            dir.path().to_path_buf(),
        )
        .with_jobs(jobs.clone());

        // El job falla con un mensaje de 50 000 bytes: ese texto es el `detail` del aviso.
        let job = jobs
            .start(JobSpec {
                kind: "test".into(),
                label: "tarea larga".into(),
                owner: sid,
                output_limit: 20,
                run: Box::new(|_, _| {
                    Box::pin(async { Err(crate::error::CoreError::Tool("e".repeat(50_000))) })
                }),
            })
            .unwrap();
        jobs.wait(sid, job.id()).await.unwrap();

        assert_eq!(agent.flush_subagent_notices(sid).await.unwrap(), 1);
        let stored = store.messages(sid).await.unwrap();
        let aviso = stored
            .iter()
            .map(|m| m.text())
            .find(|texto| texto.starts_with("[Job "))
            .expect("el aviso del job debería quedar en el historial");
        assert!(aviso.starts_with(&format!("[Job {} failed] tarea larga\n", job.id().0)));
        // Tope de 8 000 bytes de detalle más el aviso de recorte; lejos de los 50 000 originales.
        assert!(
            aviso.len() < crate::tools::MAX_NOTICE_BYTES + 300,
            "medía {}",
            aviso.len()
        );
        // ADAPTADO: el aviso se recorta ahora con cabeza y cola y el marcador «bytes omitted».
        assert!(aviso.contains("bytes omitted ...]"));
    }

    /// Catálogo con `acme` (`https://acme.example/v1`) y dos modelos con precios distintos.
    struct Voluminosa;

    #[async_trait]
    impl Tool for Voluminosa {
        fn name(&self) -> &str {
            "voluminosa"
        }
        fn description(&self) -> &str {
            "n"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
            ExecutionMode::Exclusive
        }
        async fn run(
            &self,
            _c: ToolCtx,
            _i: serde_json::Value,
            _x: CancellationToken,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::ok("x".repeat(50_000)))
        }
    }

    /// Un turno en el que el modelo llama a `voluminosa`. Devuelve el contenido del resultado
    /// de herramienta tal como queda en el historial.
    async fn resultado_de_voluminosa(spill: Option<ToolSpill>, dir: &std::path::Path) -> String {
        let scripts = vec![
            vec![
                StreamChunk::ToolCallComplete {
                    id: "c1".into(),
                    name: "voluminosa".into(),
                    input: serde_json::json!({}),
                },
                StreamChunk::Done(FinishReason::ToolCalls),
            ],
            vec![
                StreamChunk::TextDelta("fin".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(scripts.into()),
        });
        let store = Arc::new(
            SqliteStore::open(dir.join("l.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(Voluminosa));
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let mut agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.to_path_buf(),
        );
        if let Some(spill) = spill {
            agent = agent.with_spill(spill);
        }
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let msgs = store.messages(sid).await.unwrap();
        msgs.iter()
            .flat_map(|m| m.parts.iter())
            .find_map(|p| match p {
                ContentPart::ToolResult { content, .. } => Some(content.clone()),
                _ => None,
            })
            .expect("debe haber un resultado de herramienta")
    }

    #[tokio::test]
    async fn oversized_tool_result_is_spilled_and_the_path_is_in_the_message() {
        let dir = tempfile::tempdir().unwrap();
        let spill = ToolSpill::new(dir.path().join("spill"));
        let contenido = resultado_de_voluminosa(Some(spill), dir.path()).await;

        assert!(
            contenido.contains("[full output saved to: "),
            "sin ruta en el recorte"
        );
        assert!(contenido.len() <= MAX_TOOL_OUTPUT_BYTES);
        assert!(contenido.ends_with(']'));
        let inicio =
            contenido.find("[full output saved to: ").unwrap() + "[full output saved to: ".len();
        let ruta = &contenido[inicio..contenido.len() - 1];
        let guardado = std::fs::read_to_string(ruta).unwrap();
        assert_eq!(guardado, "x".repeat(50_000));
    }

    #[tokio::test]
    async fn without_spill_the_tool_result_has_no_path() {
        let dir = tempfile::tempdir().unwrap();
        let contenido = resultado_de_voluminosa(None, dir.path()).await;
        assert!(!contenido.contains("saved to"));
        assert!(contenido.contains("bytes omitted"));
    }

    fn priced_catalog() -> Arc<RwLock<crate::catalog::Catalog>> {
        let catalog = crate::catalog::Catalog::from_json(
            r#"{"acme": {"id": "acme", "npm": "@ai-sdk/openai-compatible",
                "api": "https://acme.example/v1", "env": ["ACME_KEY"],
                "models": {
                    "m-caro": {"id": "m-caro", "tool_call": true, "cost": {"input": 3, "output": 15}},
                    "m-barato": {"id": "m-barato", "tool_call": true, "cost": {"input": 0.5, "output": 1}}
                }}}"#,
        )
        .expect("el JSON de precios es válido");
        Arc::new(RwLock::new(catalog))
    }

    /// Configuración del proveedor `acme` con el modelo indicado.
    fn acme_config(model: &str) -> CodeLoomConfig {
        CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "https://acme.example/v1".into(),
                api_key_env: "K".into(),
            },
            model: model.into(),
            max_steps: 5,
            effort: None,
        }
    }

    fn usage_chunk(input: u32, output: u32) -> StreamChunk {
        StreamChunk::Usage(crate::message::Usage {
            input_tokens: input,
            output_tokens: output,
        })
    }

    #[tokio::test]
    async fn agent_records_one_usage_row_per_response_with_current_model_and_price() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("a.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        // Como Anthropic: dos fragmentos de uso en una misma respuesta.
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(
                vec![vec![
                    StreamChunk::TextDelta("hola".into()),
                    usage_chunk(10, 1),
                    usage_chunk(0, 5),
                    StreamChunk::Done(FinishReason::Stop),
                ]]
                .into(),
            ),
        });
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            acme_config("m-caro"),
            dir.path().to_path_buf(),
        )
        .with_catalog(priced_catalog());
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let rows = store.session_usage(sid).await.unwrap();
        assert_eq!(rows.len(), 1, "una respuesta, un apunte");
        assert_eq!(rows[0].model, "m-caro");
        assert_eq!(rows[0].input_tokens, 10);
        assert_eq!(rows[0].output_tokens, 6);
        assert_eq!(
            (rows[0].input_price, rows[0].output_price),
            (Some(3.0), Some(15.0))
        );
    }

    #[tokio::test]
    async fn agent_records_null_prices_without_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("sin.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(
                vec![vec![
                    usage_chunk(7, 3),
                    StreamChunk::Done(FinishReason::Stop),
                ]]
                .into(),
            ),
        });
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            acme_config("m-caro"),
            dir.path().to_path_buf(),
        );
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let rows = store.session_usage(sid).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].input_tokens, rows[0].output_tokens), (7, 3));
        assert_eq!((rows[0].input_price, rows[0].output_price), (None, None));
        assert_eq!(rows[0].cost(), None);
    }

    #[tokio::test]
    async fn agent_usage_record_carries_agent_session() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("sesion-agente.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(
                vec![vec![
                    StreamChunk::TextDelta("hola".into()),
                    usage_chunk(4, 2),
                    StreamChunk::Done(FinishReason::Stop),
                ]]
                .into(),
            ),
        });
        let agent = AgentLoop::new(
            provider,
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            acme_config("m-caro"),
            dir.path().to_path_buf(),
        )
        .with_catalog(priced_catalog());
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let rows = store.session_usage(sid).await.unwrap();
        assert_eq!(rows.len(), 1, "una respuesta, un apunte");
        assert_eq!(
            rows[0].session, sid,
            "el apunte lleva la sesión del propio agente"
        );
    }

    #[tokio::test]
    async fn model_switch_mid_session_keeps_each_price() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("cambio.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let provider = Arc::new(QueueProvider {
            scripts: Mutex::new(
                vec![
                    vec![usage_chunk(100, 10), StreamChunk::Done(FinishReason::Stop)],
                    vec![usage_chunk(200, 20), StreamChunk::Done(FinishReason::Stop)],
                ]
                .into(),
            ),
        });
        let agent = AgentLoop::new(
            provider.clone(),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            acme_config("m-caro"),
            dir.path().to_path_buf(),
        )
        .with_catalog(priced_catalog());
        agent
            .run(sid, "uno".into(), CancellationToken::new())
            .await
            .unwrap();
        agent.update_provider(provider.clone(), acme_config("m-barato"));
        agent
            .run(sid, "dos".into(), CancellationToken::new())
            .await
            .unwrap();

        let rows = store.session_usage(sid).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].model, "m-caro");
        assert_eq!(
            (rows[0].input_price, rows[0].output_price),
            (Some(3.0), Some(15.0))
        );
        assert_eq!(rows[1].model, "m-barato");
        assert_eq!(
            (rows[1].input_price, rows[1].output_price),
            (Some(0.5), Some(1.0))
        );
        // 100·3 + 10·15 + 200·0.5 + 20·1, todo por millón: 0.00057 dólares.
        let total: f64 = rows.iter().filter_map(|row| row.cost()).sum();
        assert!(
            (total - 0.00057).abs() < 1e-12,
            "total de la sesión: {total}"
        );
    }

    /// Proveedor que anota el prompt de sistema de cada petición que recibe.
    struct SystemSpy {
        seen: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl Provider for SystemSpy {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: false,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            self.seen.lock().unwrap().push(req.system.clone());
            Ok(Box::pin(futures::stream::iter(vec![StreamChunk::Done(
                FinishReason::Stop,
            )])))
        }
    }

    #[tokio::test]
    async fn the_model_is_told_which_agents_it_can_delegate_to() {
        let dir = tempfile::tempdir().unwrap();
        let agentes = dir.path().join("agentes");
        std::fs::create_dir(&agentes).unwrap();
        std::fs::write(
            agentes.join("rust.md"),
            "---
name: rust
description: Experto en Rust
---

Cuerpo.
",
        )
        .unwrap();
        let registro = Arc::new(crate::registry::AgentRegistry::new(vec![
            Box::new(crate::registry::BuiltInSource),
            Box::new(crate::registry::FileSource::new(
                agentes,
                crate::registry::ProfileLevel::Global,
            )),
        ]));
        let provider = Arc::new(SystemSpy {
            seen: Mutex::new(Vec::new()),
        });
        let store = Arc::new(
            SqliteStore::open(dir.path().join("s.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::subagent::SubagentTool));
        let agent = AgentLoop::new(
            provider.clone(),
            store,
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        )
        .with_agent_registry(registro);
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let vistos = provider.seen.lock().unwrap();
        assert!(
            vistos[0].contains("- rust (global): Experto en Rust"),
            "el modelo debe ver el agente: {}",
            vistos[0]
        );
    }

    #[tokio::test]
    async fn the_system_prompt_sections_are_capped() {
        let dir = tempfile::tempdir().unwrap();
        let agentes = dir.path().join("agentes");
        std::fs::create_dir(&agentes).unwrap();
        // Cien agentes con descripción larga: la lista, sin tope, pasaría de los 20 KB.
        for n in 0..100 {
            std::fs::write(
                agentes.join(format!("a{n}.md")),
                format!(
                    "---\nname: a{n}\ndescription: {}\n---\n\nCuerpo.\n",
                    "d".repeat(200)
                ),
            )
            .unwrap();
        }
        std::fs::write(
            dir.path().join("AGENTS.md"),
            "c".repeat(crate::config::context::MAX_CONTEXT_FILE_BYTES * 5),
        )
        .unwrap();
        let registro = Arc::new(crate::registry::AgentRegistry::new(vec![
            Box::new(crate::registry::BuiltInSource),
            Box::new(crate::registry::FileSource::new(
                agentes,
                crate::registry::ProfileLevel::Global,
            )),
        ]));
        let provider = Arc::new(SystemSpy {
            seen: Mutex::new(Vec::new()),
        });
        let store = Arc::new(
            SqliteStore::open(dir.path().join("s.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::subagent::SubagentTool));
        let agent = AgentLoop::new(
            provider.clone(),
            store,
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        )
        .with_agent_registry(registro)
        .with_system_extra(Some("e".repeat(MAX_SYSTEM_EXTRA_BYTES * 5)));
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let vistos = provider.seen.lock().unwrap();
        // Tres secciones, cada una con su tope más el marcador y los separadores.
        let techo = crate::config::context::MAX_CONTEXT_FILE_BYTES
            + MAX_AGENT_SECTION_BYTES
            + MAX_SYSTEM_EXTRA_BYTES
            + 600;
        assert!(
            vistos[0].len() <= techo,
            "el prompt de sistema mide {} y el techo es {techo}",
            vistos[0].len()
        );
        assert!(vistos[0].contains("AGENTS.md recortado"));
        assert!(vistos[0].contains("bytes omitted"));
    }

    #[tokio::test]
    async fn the_model_receives_the_configured_rule() {
        let dir = tempfile::tempdir().unwrap();
        let agentes = dir.path().join("agentes");
        std::fs::create_dir(&agentes).unwrap();
        std::fs::write(
            agentes.join("rust.md"),
            "---
name: rust
description: Experto en Rust
---

Cuerpo.
",
        )
        .unwrap();
        let registro = Arc::new(crate::registry::AgentRegistry::new(vec![
            Box::new(crate::registry::BuiltInSource),
            Box::new(crate::registry::FileSource::new(
                agentes,
                crate::registry::ProfileLevel::Global,
            )),
        ]));
        let provider = Arc::new(SystemSpy {
            seen: Mutex::new(Vec::new()),
        });
        let store = Arc::new(
            SqliteStore::open(dir.path().join("s.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::subagent::SubagentTool));
        let agent = AgentLoop::new(
            provider.clone(),
            store,
            Arc::new(tools),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        )
        .with_agent_registry(registro)
        .with_delegation_confirm(ConfirmDelegation::Always);
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();
        let vistos = provider.seen.lock().unwrap();
        assert!(
            vistos[0].contains("Do not start any subagent until"),
            "el modelo debe recibir la regla configurada: {}",
            vistos[0]
        );
    }

    /// Proveedor que guarda el historial de cada petición recibida.
    struct HistorySpy {
        seen: Mutex<Vec<Vec<crate::message::Message>>>,
    }

    #[async_trait]
    impl Provider for HistorySpy {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: false,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            self.seen.lock().unwrap().push(req.messages);
            Ok(Box::pin(futures::stream::iter(vec![StreamChunk::Done(
                FinishReason::Stop,
            )])))
        }
    }

    #[tokio::test]
    async fn request_clears_old_tool_results_but_store_keeps_them() {
        let provider = Arc::new(HistorySpy {
            seen: Mutex::new(Vec::new()),
        });
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("c.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        for i in 0..10 {
            let call_id = format!("c{i}");
            store
                .append_message(
                    sid,
                    // El input varía en cada llamada: con firmas iguales, `should_stop` cortaría el turno.
                    &Message::assistant(vec![ContentPart::ToolCall {
                        id: call_id.clone(),
                        name: "noop".into(),
                        input: serde_json::json!({ "i": i }),
                    }]),
                )
                .await
                .unwrap();
            store
                .append_message(
                    sid,
                    &Message::new(
                        Role::Tool,
                        vec![ContentPart::ToolResult {
                            call_id,
                            content: "x".repeat(1000),
                            is_error: false,
                        }],
                    ),
                )
                .await
                .unwrap();
        }

        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let agent = AgentLoop::new(
            provider.clone(),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );
        agent
            .run(sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let contents_of = |messages: &[Message]| -> Vec<String> {
            messages
                .iter()
                .flat_map(|m| m.parts.iter())
                .filter_map(|p| match p {
                    ContentPart::ToolResult { content, .. } => Some(content.clone()),
                    _ => None,
                })
                .collect()
        };

        let seen = provider.seen.lock().unwrap();
        let sent = contents_of(&seen[0]);
        assert_eq!(sent.len(), 10);
        for content in &sent[..4] {
            assert_eq!(
                content,
                "[tool result cleared: 1000 bytes; run the tool again if you need it]"
            );
        }
        for content in &sent[4..] {
            assert_eq!(content.len(), 1000);
        }

        let stored = contents_of(&store.messages(sid).await.unwrap());
        assert_eq!(stored.len(), 10);
        assert!(stored.iter().all(|c| c.len() == 1000));
    }

    /// Proveedor simulado de la compactación: responde siempre el mismo texto y guarda cada
    /// petición completa que recibe.
    struct SummaryProvider {
        texto: String,
        /// Cierre del flujo: `None` simula un flujo que se corta sin `Done`.
        cierre: Mutex<Option<FinishReason>>,
        /// Consumo que el proveedor declara en la respuesta, si lo declara.
        uso: Option<Usage>,
        seen: Mutex<Vec<ProviderRequest>>,
    }

    #[async_trait]
    impl Provider for SummaryProvider {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: false,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            self.seen.lock().unwrap().push(req);
            let mut chunks = vec![StreamChunk::TextDelta(self.texto.clone())];
            if let Some(uso) = &self.uso {
                chunks.push(StreamChunk::Usage(uso.clone()));
            }
            if let Some(razon) = *self.cierre.lock().unwrap() {
                chunks.push(StreamChunk::Done(razon));
            }
            Ok(Box::pin(futures::stream::iter(chunks)))
        }
    }

    /// Entorno de compactación: un almacén con el historial persistido y un agente cuyo
    /// proveedor responde `texto`.
    struct EntornoDeResumen {
        _dir: tempfile::TempDir,
        store: Arc<SqliteStore>,
        sid: SessionId,
        proveedor: Arc<SummaryProvider>,
        bus: EventBus,
        agente: AgentLoop,
    }

    async fn entorno_de_resumen(texto: &str, historial: &[Message]) -> EntornoDeResumen {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("r.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        for mensaje in historial {
            store.append_message(sid, mensaje).await.unwrap();
        }
        let proveedor = Arc::new(SummaryProvider {
            texto: texto.to_string(),
            cierre: Mutex::new(Some(FinishReason::Stop)),
            uso: Some(Usage {
                input_tokens: 700,
                output_tokens: 90,
            }),
            seen: Mutex::new(Vec::new()),
        });
        let bus = EventBus::new(64);
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 5,
            effort: None,
        };
        let agente = AgentLoop::new(
            proveedor.clone(),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            bus.clone(),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );
        EntornoDeResumen {
            _dir: dir,
            store,
            sid,
            proveedor,
            bus,
            agente,
        }
    }

    /// `n` mensajes de usuario de 4 000 bytes, es decir, 1 000 tokens estimados cada uno.
    fn historial_grande(n: usize) -> Vec<Message> {
        (0..n).map(|_| Message::user("x".repeat(4000))).collect()
    }

    #[tokio::test]
    async fn compact_persists_a_summary_message_and_leaves_the_log_intact() {
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cut = compact::choose_cut(&antes).expect("hay historia que resumir");

        let cancel = CancellationToken::new();
        assert!(e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap());

        let despues = e.store.messages(e.sid).await.unwrap();
        assert_eq!(despues.len(), antes.len() + 1);
        assert_eq!(&despues[..antes.len()], antes.as_slice());
        let (covers, texto) =
            compact::parse_summary(despues.last().unwrap()).expect("el resumen va al final");
        assert_eq!(covers, cut);
        assert_eq!(texto, "resumen");
    }

    #[tokio::test]
    async fn compact_request_has_no_tools_and_ends_with_the_summarize_instruction() {
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cancel = CancellationToken::new();
        e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap();

        let vistas = e.proveedor.seen.lock().unwrap();
        assert_eq!(vistas.len(), 1, "una sola petición de resumen");
        let req = &vistas[0];
        assert!(req.tools.is_empty());
        assert_eq!(req.system, compact::SUMMARY_SYSTEM_PROMPT);
        assert_eq!(req.temperature, 0.0);
        assert_eq!(req.max_tokens, compact::SUMMARY_MAX_TOKENS);
        assert!(req.effort.is_none());
        let ultimo = req.messages.last().expect("la petición lleva mensajes");
        assert_eq!(ultimo.role, Role::User);
        assert_eq!(
            ultimo.text(),
            "Summarize the conversation above following your instructions."
        );
    }

    #[tokio::test]
    async fn compact_caps_an_oversized_summary() {
        let e = entorno_de_resumen(&"y".repeat(40_000), &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cancel = CancellationToken::new();
        assert!(e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap());

        let despues = e.store.messages(e.sid).await.unwrap();
        let (_, texto) = compact::parse_summary(despues.last().unwrap()).unwrap();
        // El tope es exacto: el hueco del marcador de omisión sale del propio tope.
        assert!(
            texto.len() <= compact::MAX_SUMMARY_BYTES,
            "el resumen mide {} bytes",
            texto.len()
        );
        assert!(texto.len() < 40_000, "el recorte debe haber actuado");
    }

    /// Resultados de compactación manual que hay en el bus, en orden de llegada.
    fn manuales_del_bus(
        rx: &mut tokio::sync::broadcast::Receiver<LiveEvent>,
    ) -> Vec<crate::event::ManualCompaction> {
        let mut vistos = Vec::new();
        while let Ok(evento) = rx.try_recv() {
            if let LiveEvent::ManualCompactionEnded { outcome } = evento {
                vistos.push(outcome);
            }
        }
        vistos
    }

    #[tokio::test]
    async fn compact_now_summarizes_without_waiting_for_the_threshold() {
        use crate::event::ManualCompaction;
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let mut rx = e.bus.subscribe();

        let resultado = e.agente.compact_now(e.sid, CancellationToken::new()).await;

        assert_eq!(resultado, ManualCompaction::Compacted);
        assert_eq!(manuales_del_bus(&mut rx), vec![ManualCompaction::Compacted]);
        let despues = e.store.messages(e.sid).await.unwrap();
        assert!(compact::parse_summary(despues.last().unwrap()).is_some());
    }

    #[tokio::test]
    async fn compact_now_with_nothing_to_summarize_calls_no_model() {
        use crate::event::ManualCompaction;
        let e = entorno_de_resumen("resumen", &historial_grande(2)).await;
        let mut rx = e.bus.subscribe();

        let resultado = e.agente.compact_now(e.sid, CancellationToken::new()).await;

        assert_eq!(resultado, ManualCompaction::NothingToSummarize);
        assert_eq!(
            manuales_del_bus(&mut rx),
            vec![ManualCompaction::NothingToSummarize]
        );
        assert!(e.proveedor.seen.lock().unwrap().is_empty());
        assert_eq!(e.store.messages(e.sid).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn compact_now_reports_failure_when_the_summary_is_empty() {
        use crate::event::ManualCompaction;
        let e = entorno_de_resumen(
            "  
 ",
            &historial_grande(100),
        )
        .await;
        let mut rx = e.bus.subscribe();

        let resultado = e.agente.compact_now(e.sid, CancellationToken::new()).await;

        assert_eq!(resultado, ManualCompaction::Failed);
        assert_eq!(manuales_del_bus(&mut rx), vec![ManualCompaction::Failed]);
        assert_eq!(e.store.messages(e.sid).await.unwrap().len(), 100);
    }

    #[tokio::test]
    async fn compact_returns_false_when_the_model_returns_nothing() {
        let e = entorno_de_resumen("  \n ", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cancel = CancellationToken::new();
        assert!(!e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap());
        assert_eq!(e.store.messages(e.sid).await.unwrap().len(), antes.len());
    }

    #[tokio::test]
    async fn compact_discards_a_summary_whose_stream_ended_in_error_or_was_cut() {
        for cierre in [Some(FinishReason::Error), None] {
            let e = entorno_de_resumen("resumen a medias", &historial_grande(100)).await;
            *e.proveedor.cierre.lock().unwrap() = cierre;
            let antes = e.store.messages(e.sid).await.unwrap();
            let cancel = CancellationToken::new();
            assert!(!e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap());
            assert_eq!(
                e.store.messages(e.sid).await.unwrap().len(),
                antes.len(),
                "no se persiste un resumen truncado ({cierre:?})"
            );
        }
    }

    #[tokio::test]
    async fn compact_records_the_usage_of_the_summary_call_even_if_it_is_discarded() {
        for cierre in [Some(FinishReason::Stop), Some(FinishReason::Error)] {
            let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
            *e.proveedor.cierre.lock().unwrap() = cierre;
            let antes = e.store.messages(e.sid).await.unwrap();
            let mut rx = e.bus.subscribe();
            let cancel = CancellationToken::new();
            e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap();

            let apuntes = e.store.session_usage(e.sid).await.unwrap();
            assert_eq!(apuntes.len(), 1, "un apunte por la llamada ({cierre:?})");
            assert_eq!(apuntes[0].input_tokens, 700);
            assert_eq!(apuntes[0].output_tokens, 90);
            let mut visto = false;
            // ADAPTADO: el consumo del resumen usa CompactionUsage
            while let Ok(evento) = rx.try_recv() {
                visto |= matches!(
                    evento,
                    LiveEvent::CompactionUsage {
                        input_tokens: 700,
                        output_tokens: 90
                    }
                );
            }
            assert!(visto, "el consumo también llega a los contadores en vivo");
        }
    }

    #[tokio::test]
    async fn generate_summary_does_not_touch_the_store_or_the_bus() {
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let mut rx = e.bus.subscribe();
        let cancel = CancellationToken::new();
        let proveedor: Arc<dyn Provider> = e.proveedor.clone();

        let (resumen, uso) = generate_summary(proveedor, "m".into(), &antes, &cancel)
            .await
            .unwrap();
        let resumen = resumen.expect("hay resumen");
        assert_eq!(resumen.text, "resumen");
        assert_eq!(resumen.covers, compact::choose_cut(&antes).unwrap());
        // El consumo sale en la respuesta, para que el llamador lo apunte.
        assert_eq!(
            uso.map(|u| (u.input_tokens, u.output_tokens)),
            Some((700, 90))
        );

        assert_eq!(e.store.messages(e.sid).await.unwrap().len(), antes.len());
        assert!(e.store.session_usage(e.sid).await.unwrap().is_empty());
        assert!(rx.try_recv().is_err(), "no publica ningún evento");
    }

    #[tokio::test]
    async fn compact_usage_goes_through_compaction_usage_not_usage() {
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let mut rx = e.bus.subscribe();
        let cancel = CancellationToken::new();
        assert!(e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap());

        let mut consumos_de_resumen = Vec::new();
        let mut usos_normales = 0;
        while let Ok(evento) = rx.try_recv() {
            match evento {
                LiveEvent::CompactionUsage {
                    input_tokens,
                    output_tokens,
                } => consumos_de_resumen.push((input_tokens, output_tokens)),
                LiveEvent::Usage { .. } => usos_normales += 1,
                _ => {}
            }
        }
        assert_eq!(consumos_de_resumen, vec![(700, 90)]);
        assert_eq!(usos_normales, 0, "el resumen no debe publicar Usage");
    }

    #[tokio::test]
    async fn compact_returns_false_when_there_is_nothing_to_summarize() {
        let historial = [Message::user("hola"), Message::user("adiós")];
        let e = entorno_de_resumen("resumen", &historial).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cancel = CancellationToken::new();
        assert!(!e.agente.compact(e.sid, &antes, 0, &cancel).await.unwrap());
        assert!(
            e.proveedor.seen.lock().unwrap().is_empty(),
            "no debe haber petición de resumen"
        );
    }

    #[tokio::test]
    async fn compact_publishes_context_compacted() {
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cut = compact::choose_cut(&antes).expect("hay historia que resumir");
        let mut rx = e.bus.subscribe();

        let cancel = CancellationToken::new();
        assert!(e
            .agente
            .compact(e.sid, &antes, 1234, &cancel)
            .await
            .unwrap());

        let mut eventos = Vec::new();
        while let Ok(evento) = rx.try_recv() {
            eventos.push(evento);
        }
        let compactado = eventos.iter().find_map(|evento| match evento {
            LiveEvent::ContextCompacted {
                replaced,
                tokens_before,
                summary_bytes,
                ..  // ADAPTADO: campo nuevo
            } => Some((*replaced, *tokens_before, *summary_bytes)),
            _ => None,
        });
        assert_eq!(compactado, Some((cut, 1234, "resumen".len())));
        assert!(
            eventos
                .iter()
                .any(|evento| matches!(evento, LiveEvent::MessageAdded(_))),
            "el mensaje de resumen también se publica"
        );
    }

    #[tokio::test]
    async fn activate_persists_the_message_and_publishes_the_activation() {
        let e = entorno_de_resumen("resumen", &historial_grande(100)).await;
        let antes = e.store.messages(e.sid).await.unwrap();
        let cut = compact::choose_cut(&antes).expect("hay historia que resumir");
        let mut rx = e.bus.subscribe();
        let resumen = GeneratedSummary {
            text: "resumen".to_string(),
            usage: None,
            covers: cut,
        };

        e.agente
            .activate(e.sid, &resumen, 777, Activation::Background)
            .await
            .unwrap();

        let despues = e.store.messages(e.sid).await.unwrap();
        assert_eq!(despues.len(), antes.len() + 1);
        let (covers, texto) =
            compact::parse_summary(despues.last().unwrap()).expect("el resumen va al final");
        assert_eq!(covers, cut);
        assert_eq!(texto, "resumen");

        let mut eventos = Vec::new();
        while let Ok(evento) = rx.try_recv() {
            eventos.push(evento);
        }
        let activacion = eventos.iter().find_map(|evento| match evento {
            LiveEvent::ContextCompacted {
                replaced,
                tokens_before,
                summary_bytes,
                activation,
            } => Some((
                *replaced,
                *tokens_before,
                *summary_bytes,
                activation.clone(),
            )),
            _ => None,
        });
        assert_eq!(
            activacion,
            Some((cut, 777, "resumen".len(), Activation::Background))
        );
        assert!(
            eventos
                .iter()
                .any(|evento| matches!(evento, LiveEvent::MessageAdded(_))),
            "el mensaje de resumen también se publica"
        );
    }

    /// Una petición es de resumen si no lleva herramientas y su prompt de sistema es el de resumir.
    fn es_peticion_de_resumen(req: &ProviderRequest) -> bool {
        req.tools.is_empty() && req.system == compact::SUMMARY_SYSTEM_PROMPT
    }

    /// Proveedor simulado de un turno completo. Distingue la petición de resumen de las normales
    /// con `es_peticion_de_resumen`: la de resumen responde `resumen` o falla según `falla_resumen`;
    /// las normales salen de `normales` en orden y, agotadas, responden un texto final.
    struct PlanProvider {
        resumen: String,
        falla_resumen: bool,
        normales: Mutex<VecDeque<Vec<StreamChunk>>>,
        seen: Mutex<Vec<ProviderRequest>>,
        /// Cuántas peticiones normales (no de resumen) se rechazan aún por exceso de contexto.
        desbordes: Mutex<usize>,
    }

    #[async_trait]
    impl Provider for PlanProvider {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            let de_resumen = es_peticion_de_resumen(&req);
            self.seen.lock().unwrap().push(req);
            if de_resumen {
                if self.falla_resumen {
                    return Err(crate::error::CoreError::Provider("fallo simulado".into()));
                }
                return Ok(Box::pin(futures::stream::iter(vec![
                    StreamChunk::TextDelta(self.resumen.clone()),
                    StreamChunk::Done(FinishReason::Stop),
                ])));
            }
            {
                let mut desbordes = self.desbordes.lock().unwrap();
                if *desbordes > 0 {
                    *desbordes -= 1;
                    return Err(crate::error::CoreError::Provider(
                        "http 400 Bad Request: This model's maximum context length is 8192 tokens"
                            .into(),
                    ));
                }
            }
            let guion = self
                .normales
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    vec![
                        StreamChunk::TextDelta("hecho".into()),
                        StreamChunk::Done(FinishReason::Stop),
                    ]
                });
            Ok(Box::pin(futures::stream::iter(guion)))
        }
    }

    /// Herramienta que devuelve 4 000 bytes: cada resultado pesa 1 000 tokens estimados.
    struct Relleno;

    #[async_trait]
    impl Tool for Relleno {
        fn name(&self) -> &str {
            "relleno"
        }
        fn description(&self) -> &str {
            "r"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
            ExecutionMode::Exclusive
        }
        async fn run(
            &self,
            _c: ToolCtx,
            _i: serde_json::Value,
            _x: CancellationToken,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::ok("x".repeat(4000)))
        }
    }

    /// Respuesta normal que pide una llamada a `relleno` con una entrada distinta por paso, para
    /// que `should_stop` no corte el turno por repetición.
    fn llamada_relleno(n: usize) -> Vec<StreamChunk> {
        vec![
            StreamChunk::ToolCallComplete {
                id: format!("t{n}"),
                name: "relleno".into(),
                input: serde_json::json!({ "i": n }),
            },
            StreamChunk::Done(FinishReason::ToolCalls),
        ]
    }

    /// Catálogo con el proveedor de `http://x` y un modelo `m` de ventana `ventana` tokens.
    fn catalogo_de_ventana(ventana: u64) -> Arc<RwLock<crate::catalog::Catalog>> {
        let json = format!(
            r#"{{"x": {{"id": "x", "npm": "@ai-sdk/openai-compatible", "api": "http://x", "env": ["K"],
                "models": {{"m": {{"id": "m", "tool_call": true,
                    "limit": {{"context": {ventana}, "output": 4096}}}}}}}}}}"#
        );
        Arc::new(RwLock::new(
            crate::catalog::Catalog::from_json(&json).expect("el JSON de ventana es válido"),
        ))
    }

    struct EntornoDeTurno {
        _dir: tempfile::TempDir,
        store: Arc<SqliteStore>,
        sid: SessionId,
        proveedor: Arc<PlanProvider>,
        agente: AgentLoop,
    }

    async fn entorno_de_turno(
        proveedor: PlanProvider,
        catalogo: Arc<RwLock<crate::catalog::Catalog>>,
        historial: &[Message],
        herramientas: ToolRegistry,
    ) -> EntornoDeTurno {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("t.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        for mensaje in historial {
            store.append_message(sid, mensaje).await.unwrap();
        }
        let proveedor = Arc::new(proveedor);
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 10,
            effort: None,
        };
        let agente = AgentLoop::new(
            proveedor.clone(),
            store.clone(),
            Arc::new(herramientas),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        )
        .with_catalog(catalogo);
        EntornoDeTurno {
            _dir: dir,
            store,
            sid,
            proveedor,
            agente,
        }
    }

    fn proveedor_de(
        resumen: &str,
        falla_resumen: bool,
        normales: Vec<Vec<StreamChunk>>,
    ) -> PlanProvider {
        PlanProvider {
            resumen: resumen.to_string(),
            falla_resumen,
            normales: Mutex::new(VecDeque::from(normales)),
            seen: Mutex::new(Vec::new()),
            desbordes: Mutex::new(0),
        }
    }

    /// Como `proveedor_de`, pero las primeras `n` peticiones normales las rechaza por exceso de
    /// contexto.
    fn proveedor_que_desborda(resumen: &str, n: usize) -> PlanProvider {
        let proveedor = proveedor_de(resumen, false, vec![]);
        *proveedor.desbordes.lock().unwrap() = n;
        proveedor
    }

    #[tokio::test]
    async fn a_context_overflow_rejection_compacts_and_retries_the_request() {
        // Ventana enorme: la compactación preventiva no actúa; sólo el rechazo del proveedor.
        let historial = historial_grande(100);
        let e = entorno_de_turno(
            proveedor_que_desborda("resumen", 1),
            catalogo_de_ventana(10_000_000),
            &historial,
            ToolRegistry::new(),
        )
        .await;

        e.agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
            .expect("el turno se recupera del rechazo");

        let vistas = e.proveedor.seen.lock().unwrap();
        let normales: Vec<_> = vistas
            .iter()
            .filter(|r| !es_peticion_de_resumen(r))
            .collect();
        assert_eq!(normales.len(), 2, "la petición rechazada y su reintento");
        assert_eq!(vistas.len(), 3, "más una de resumen");
        assert!(
            normales[1].messages.len() < normales[0].messages.len(),
            "el reintento lleva el historial resumido"
        );
        assert!(compact::parse_summary(&normales[1].messages[0]).is_some());
    }

    #[tokio::test]
    async fn a_context_overflow_that_persists_stops_after_the_compaction_limit() {
        let historial = historial_grande(100);
        let e = entorno_de_turno(
            proveedor_que_desborda("resumen", usize::MAX),
            catalogo_de_ventana(10_000_000),
            &historial,
            ToolRegistry::new(),
        )
        .await;

        let Err(error) = e
            .agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
        else {
            panic!("sin remedio, el error debe llegar");
        };
        assert!(compact::is_context_overflow(&error), "{error}");

        let vistas = e.proveedor.seen.lock().unwrap();
        let resumenes = vistas.iter().filter(|r| es_peticion_de_resumen(r)).count();
        assert!(
            resumenes <= compact::MAX_COMPACTIONS_PER_TURN,
            "{resumenes} resúmenes"
        );
    }

    /// Historial alternado: los pares son llamadas del asistente y los impares sus resultados. Cada
    /// mensaje pesa unos 1 000 tokens, de modo que el corte natural de una cola de 10 000 tokens
    /// cae en un resultado y debe retroceder hasta su llamada.
    fn historial_con_herramientas(n: usize) -> Vec<Message> {
        (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    Message::assistant(vec![ContentPart::ToolCall {
                        id: format!("c{i}"),
                        name: "relleno".into(),
                        input: serde_json::json!({ "i": format!("{i}{}", "p".repeat(4000)) }),
                    }])
                } else {
                    Message::new(
                        Role::Tool,
                        vec![ContentPart::ToolResult {
                            call_id: format!("c{}", i - 1),
                            content: "x".repeat(4000),
                            is_error: false,
                        }],
                    )
                }
            })
            .collect()
    }

    /// Comprueba que cada resultado de herramienta de la petición lleva su llamada delante y que,
    /// si la petición empieza por un resumen, la cola no empieza por un resultado.
    fn assert_emparejado(mensajes: &[Message]) {
        if compact::parse_summary(&mensajes[0]).is_some() {
            assert_ne!(
                mensajes[1].role,
                Role::Tool,
                "la cola no puede empezar por un resultado de herramienta"
            );
        }
        let mut llamadas = std::collections::HashSet::new();
        for mensaje in mensajes {
            for parte in &mensaje.parts {
                match parte {
                    ContentPart::ToolCall { id, .. } => {
                        llamadas.insert(id.clone());
                    }
                    ContentPart::ToolResult { call_id, .. } => {
                        assert!(
                            llamadas.contains(call_id),
                            "resultado {call_id} sin su llamada delante"
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    #[tokio::test]
    async fn request_is_summary_plus_tail_once_the_window_is_reached() {
        let historial = historial_grande(100);
        let e = entorno_de_turno(
            proveedor_de("resumen", false, vec![]),
            catalogo_de_ventana(1000),
            &historial,
            ToolRegistry::new(),
        )
        .await;
        let mut esperado = historial.clone();
        esperado.push(Message::user("hi"));
        let cut = compact::choose_cut(&esperado).expect("hay historia que resumir");

        e.agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let vistas = e.proveedor.seen.lock().unwrap();
        assert_eq!(vistas.len(), 2, "una petición de resumen y una normal");
        assert!(es_peticion_de_resumen(&vistas[0]));
        let normal = &vistas[1];
        assert!(!es_peticion_de_resumen(normal));
        let (covers, _) = compact::parse_summary(&normal.messages[0])
            .expect("la petición empieza por el resumen");
        assert_eq!(covers, cut);
        assert_eq!(normal.messages.len(), 1 + (esperado.len() - cut));
        assert_emparejado(&normal.messages);
    }

    #[tokio::test]
    async fn store_keeps_every_message_after_compaction() {
        let historial = historial_grande(100);
        let e = entorno_de_turno(
            proveedor_de("resumen", false, vec![]),
            catalogo_de_ventana(1000),
            &historial,
            ToolRegistry::new(),
        )
        .await;
        let antes = e.store.messages(e.sid).await.unwrap();

        e.agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        // Tras el turno: lo anterior íntegro, el «hi», el resumen y la respuesta del asistente.
        let despues = e.store.messages(e.sid).await.unwrap();
        assert_eq!(despues.len(), antes.len() + 3);
        assert_eq!(&despues[..antes.len()], antes.as_slice());
        assert_eq!(despues[antes.len()].text(), "hi");
        assert!(compact::parse_summary(&despues[antes.len() + 1]).is_some());
    }

    #[tokio::test]
    async fn no_compaction_when_the_window_is_unknown() {
        let historial = historial_grande(100);
        let e = entorno_de_turno(
            proveedor_de("resumen", false, vec![]),
            Arc::new(RwLock::new(crate::catalog::Catalog::default())),
            &historial,
            ToolRegistry::new(),
        )
        .await;

        e.agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let vistas = e.proveedor.seen.lock().unwrap();
        assert_eq!(vistas.len(), 1, "solo la petición normal, sin resumen");
        assert!(!es_peticion_de_resumen(&vistas[0]));
        assert_eq!(vistas[0].messages.len(), historial.len() + 1);
    }

    #[tokio::test]
    async fn a_failing_summary_does_not_abort_the_turn_and_is_not_retried() {
        // Tres pasos con herramienta: si el fallo se reintentara en cada paso, habría más de una
        // petición de resumen.
        let historial = historial_grande(100);
        let mut herramientas = ToolRegistry::new();
        herramientas.register(Arc::new(Relleno));
        let e = entorno_de_turno(
            proveedor_de(
                "resumen",
                true,
                vec![
                    llamada_relleno(1),
                    llamada_relleno(2),
                    vec![
                        StreamChunk::TextDelta("fin".into()),
                        StreamChunk::Done(FinishReason::Stop),
                    ],
                ],
            ),
            catalogo_de_ventana(1000),
            &historial,
            herramientas,
        )
        .await;
        let antes = e.store.messages(e.sid).await.unwrap();

        let resultado = e
            .agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await;
        assert!(
            resultado.is_ok(),
            "el turno termina aunque falle el resumen"
        );

        let vistas = e.proveedor.seen.lock().unwrap();
        assert_eq!(
            vistas.iter().filter(|r| es_peticion_de_resumen(r)).count(),
            1,
            "una sola petición de resumen, sin reintento"
        );
        let normales: Vec<&ProviderRequest> = vistas
            .iter()
            .filter(|r| !es_peticion_de_resumen(r))
            .collect();
        assert_eq!(
            normales.len(),
            3,
            "los tres pasos del turno llegan al proveedor"
        );
        let normal = normales[0];
        assert_eq!(
            normal.messages.len(),
            antes.len() + 1,
            "sin resumen, la petición lleva el historial completo"
        );
        let despues = e.store.messages(e.sid).await.unwrap();
        assert!(
            despues.iter().all(|m| compact::parse_summary(m).is_none()),
            "no se persiste ningún resumen"
        );
    }

    #[tokio::test]
    async fn compactions_per_turn_are_capped() {
        // 300 mensajes de 1 000 tokens. Cada paso añade un resultado de 1 000 tokens, así que
        // siempre queda algo que resumir y cada paso intentaría compactar: solo deben hacerse
        // `MAX_COMPACTIONS_PER_TURN`.
        let historial = historial_grande(300);
        let normales = vec![
            llamada_relleno(1),
            llamada_relleno(2),
            llamada_relleno(3),
            llamada_relleno(4),
            vec![
                StreamChunk::TextDelta("fin".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        ];
        let mut herramientas = ToolRegistry::new();
        herramientas.register(Arc::new(Relleno));
        let e = entorno_de_turno(
            proveedor_de("resumen", false, normales),
            catalogo_de_ventana(1000),
            &historial,
            herramientas,
        )
        .await;

        e.agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let vistas = e.proveedor.seen.lock().unwrap();
        let resumenes = vistas.iter().filter(|r| es_peticion_de_resumen(r)).count();
        assert_eq!(resumenes, compact::MAX_COMPACTIONS_PER_TURN);
        assert_eq!(vistas.len() - resumenes, 5, "las cinco peticiones normales");
    }

    #[tokio::test]
    async fn tool_call_and_result_stay_paired_in_the_request() {
        let historial = historial_con_herramientas(101);
        let e = entorno_de_turno(
            proveedor_de("resumen", false, vec![]),
            catalogo_de_ventana(1000),
            &historial,
            ToolRegistry::new(),
        )
        .await;
        let mut esperado = historial.clone();
        esperado.push(Message::user("hi"));
        // El corte natural cae en el resultado 91 y se retrocede hasta la llamada 90.
        assert_eq!(compact::choose_cut(&esperado), Some(90));

        e.agente
            .run(e.sid, "hi".into(), CancellationToken::new())
            .await
            .unwrap();

        let vistas = e.proveedor.seen.lock().unwrap();
        let normal = vistas
            .iter()
            .find(|r| !es_peticion_de_resumen(r))
            .expect("hay una petición normal");
        assert!(compact::parse_summary(&normal.messages[0]).is_some());
        assert_emparejado(&normal.messages);
    }

    /// Almacén que cuenta las lecturas del consumo acumulado y delega todo lo demás en el real.
    struct AlmacenContador {
        dentro: Arc<SqliteStore>,
        lecturas_de_consumo: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl SessionStore for AlmacenContador {
        async fn create_session(&self, project: &str) -> Result<SessionId> {
            self.dentro.create_session(project).await
        }
        async fn list_sessions(&self, project: &str) -> Result<Vec<crate::session::SessionInfo>> {
            self.dentro.list_sessions(project).await
        }
        async fn append_message(&self, sid: SessionId, msg: &Message) -> Result<()> {
            self.dentro.append_message(sid, msg).await
        }
        async fn messages(&self, sid: SessionId) -> Result<Vec<Message>> {
            self.dentro.messages(sid).await
        }
        async fn save_subagent(
            &self,
            descriptor: &crate::session::SubagentDescriptor,
        ) -> Result<()> {
            self.dentro.save_subagent(descriptor).await
        }
        async fn update_subagent_status(
            &self,
            parent: SessionId,
            child: SessionId,
            status: crate::task::TaskStatus,
        ) -> Result<()> {
            self.dentro
                .update_subagent_status(parent, child, status)
                .await
        }
        async fn list_subagents(
            &self,
            parent: SessionId,
        ) -> Result<Vec<crate::session::SubagentDescriptor>> {
            self.dentro.list_subagents(parent).await
        }
        async fn complete_subagent(
            &self,
            parent: SessionId,
            child: SessionId,
            artifacts: &[crate::a2a::Artifact],
        ) -> Result<()> {
            self.dentro
                .complete_subagent(parent, child, artifacts)
                .await
        }
        async fn append_subagent_notice(
            &self,
            parent: SessionId,
            child: SessionId,
            message: &Message,
        ) -> Result<bool> {
            self.dentro
                .append_subagent_notice(parent, child, message)
                .await
        }
        async fn record_usage(&self, session: SessionId, record: &UsageRecord) -> Result<()> {
            self.dentro.record_usage(session, record).await
        }
        async fn session_usage(&self, session: SessionId) -> Result<Vec<UsageRecord>> {
            self.lecturas_de_consumo
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.dentro.session_usage(session).await
        }
    }

    /// Presupuesto solo de tokens, con el límite indicado.
    fn presupuesto_de_tokens(limite: u64) -> BudgetConfig {
        BudgetConfig {
            session_tokens: Some(limite),
            session_cost_usd: None,
        }
    }

    /// Respuesta de texto que declara `entrada` y `salida` tokens de consumo.
    fn respuesta_con_consumo(texto: &str, entrada: u32, salida: u32) -> Vec<StreamChunk> {
        vec![
            StreamChunk::TextDelta(texto.into()),
            StreamChunk::Usage(Usage {
                input_tokens: entrada,
                output_tokens: salida,
            }),
            StreamChunk::Done(FinishReason::Stop),
        ]
    }

    /// Niveles y consumo de los avisos de presupuesto que hay en el bus, en orden de llegada.
    fn avisos_del_bus(rx: &mut tokio::sync::broadcast::Receiver<LiveEvent>) -> Vec<(u8, f64)> {
        let mut avisos = Vec::new();
        while let Ok(evento) = rx.try_recv() {
            if let LiveEvent::BudgetWarning {
                level_percent,
                used,
                ..
            } = evento
            {
                avisos.push((level_percent, used));
            }
        }
        avisos
    }

    #[tokio::test]
    async fn usage_crossing_the_budget_publishes_a_warning() {
        let proveedor = proveedor_de("", false, vec![respuesta_con_consumo("hola", 700, 100)]);
        let mut e = entorno_de_turno(
            proveedor,
            catalogo_de_ventana(100_000),
            &[],
            ToolRegistry::new(),
        )
        .await;
        e.agente = e.agente.with_budget(presupuesto_de_tokens(1000));
        let mut rx = e.agente.bus.subscribe();

        e.agente
            .run(e.sid, "hola".into(), CancellationToken::new())
            .await
            .unwrap();

        // 800 tokens sobre un límite de 1000: se cruza el 80 % y no el 100 %.
        assert_eq!(avisos_del_bus(&mut rx), vec![(80, 800.0)]);
    }

    /// Registra `child` como hijo de `parent` en el almacén, como hace el lanzador.
    async fn registrar_hijo(store: &SqliteStore, parent: SessionId) -> SessionId {
        let child = store.create_session("/p").await.unwrap();
        store
            .save_subagent(&crate::session::SubagentDescriptor {
                parent,
                child,
                job_id: "subagent-1".into(),
                label: "hijo".into(),
                status: crate::task::TaskStatus::new(crate::task::TaskState::Working),
                artifacts: Vec::new(),
                notice_delivered: false,
                agent: None,
                agent_origin: None,
                reference_task_ids: Vec::new(),
            })
            .await
            .unwrap();
        child
    }

    #[tokio::test]
    async fn a_child_spending_crosses_the_family_budget_once() {
        let proveedor = proveedor_de("", false, vec![respuesta_con_consumo("hijo", 100, 50)]);
        let mut e = entorno_de_turno(
            proveedor,
            catalogo_de_ventana(100_000),
            &[],
            ToolRegistry::new(),
        )
        .await;
        let hijo = registrar_hijo(&e.store, e.sid).await;
        // El padre ya lleva 700 de 1000. El hijo gasta 150: la familia llega a 850 y cruza el 80 %.
        e.store
            .record_usage(
                e.sid,
                &UsageRecord {
                    session: e.sid,
                    model: "m".into(),
                    input_tokens: 700,
                    output_tokens: 0,
                    input_price: None,
                    output_price: None,
                    created_at_ms: 0,
                },
            )
            .await
            .unwrap();
        e.agente = e
            .agente
            .with_budget(presupuesto_de_tokens(1000))
            .with_budget_family(e.sid);
        let mut rx = e.agente.bus.subscribe();

        e.agente
            .run(hijo, "tarea".into(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(avisos_del_bus(&mut rx), vec![(80, 850.0)]);
    }

    #[tokio::test]
    async fn the_warning_is_not_repeated_on_the_next_response() {
        let proveedor = proveedor_de(
            "",
            false,
            vec![
                respuesta_con_consumo("primera", 700, 100),
                respuesta_con_consumo("segunda", 200, 100),
            ],
        );
        let mut e = entorno_de_turno(
            proveedor,
            catalogo_de_ventana(100_000),
            &[],
            ToolRegistry::new(),
        )
        .await;
        e.agente = e.agente.with_budget(presupuesto_de_tokens(1000));
        let mut rx = e.agente.bus.subscribe();

        e.agente
            .run(e.sid, "uno".into(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(avisos_del_bus(&mut rx), vec![(80, 800.0)]);

        // 800 -> 1100: cruza el 100 %, pero el 80 % ya se dio y no vuelve a salir.
        e.agente
            .run(e.sid, "dos".into(), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(avisos_del_bus(&mut rx), vec![(100, 1100.0)]);
    }

    #[tokio::test]
    async fn no_budget_means_no_store_read_and_no_warning() {
        let dir = tempfile::tempdir().unwrap();
        let dentro = Arc::new(
            SqliteStore::open(dir.path().join("sin.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let almacen = Arc::new(AlmacenContador {
            dentro: dentro.clone(),
            lecturas_de_consumo: std::sync::atomic::AtomicUsize::new(0),
        });
        let sid = dentro.create_session("/p").await.unwrap();
        let proveedor = Arc::new(proveedor_de(
            "",
            false,
            vec![respuesta_con_consumo("hola", 5000, 500)],
        ));
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 10,
            effort: None,
        };
        // Sin `with_budget`: el presupuesto es el vacío por defecto.
        let agente = AgentLoop::new(
            proveedor,
            almacen.clone(),
            Arc::new(ToolRegistry::new()),
            EventBus::new(64),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        );
        let mut rx = agente.bus.subscribe();

        agente
            .run(sid, "hola".into(), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(
            almacen
                .lecturas_de_consumo
                .load(std::sync::atomic::Ordering::SeqCst),
            0,
            "sin presupuesto no se lee el consumo acumulado"
        );
        assert!(avisos_del_bus(&mut rx).is_empty());
        assert_eq!(
            dentro.session_usage(sid).await.unwrap().len(),
            1,
            "el apunte se guarda igual"
        );
    }

    #[tokio::test]
    async fn the_turn_is_not_stopped_by_the_warning() {
        // El primer paso pide una herramienta y declara un consumo que supera el 100 %. El turno
        // debe seguir y el asistente responder en el segundo paso.
        let proveedor = proveedor_de(
            "",
            false,
            vec![
                vec![
                    StreamChunk::TextDelta("voy a mirar".into()),
                    StreamChunk::Usage(Usage {
                        input_tokens: 500,
                        output_tokens: 0,
                    }),
                    StreamChunk::ToolCallComplete {
                        id: "c1".into(),
                        name: "noop".into(),
                        input: serde_json::json!({}),
                    },
                    StreamChunk::Done(FinishReason::ToolCalls),
                ],
                respuesta_con_consumo("listo", 10, 5),
            ],
        );
        let mut herramientas = ToolRegistry::new();
        herramientas.register(Arc::new(Noop));
        let mut e =
            entorno_de_turno(proveedor, catalogo_de_ventana(100_000), &[], herramientas).await;
        e.agente = e.agente.with_budget(presupuesto_de_tokens(100));
        let mut rx = e.agente.bus.subscribe();

        let resultado = e
            .agente
            .run(e.sid, "hola".into(), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(resultado.final_text, "listo");
        // 500 sobre 100: cruza el 80 % y el 100 % en el primer paso.
        assert_eq!(avisos_del_bus(&mut rx), vec![(80, 500.0), (100, 500.0)]);
    }

    /// Proveedor de resumen controlable. Solo la primera petición de resumen espera a `compuerta`;
    /// las siguientes responden al momento, así que una vía síncrona posterior no se bloquea. Cada
    /// petición de resumen avisa por el canal de llegadas en cuanto entra en el proveedor.
    struct ResumenRetenido {
        resumen: String,
        compuerta: Semaphore,
        llegadas: mpsc::UnboundedSender<()>,
        peticiones: AtomicUsize,
        normales: Mutex<VecDeque<Vec<StreamChunk>>>,
        /// Si existe, cada respuesta normal lo cancela: simula que la persona cancela el turno.
        cancelar_turno: Option<CancellationToken>,
    }

    impl ResumenRetenido {
        /// Devuelve el proveedor y el receptor de llegadas de peticiones de resumen.
        fn nuevo(
            resumen: &str,
            normales: Vec<Vec<StreamChunk>>,
            cancelar_turno: Option<CancellationToken>,
        ) -> (Self, mpsc::UnboundedReceiver<()>) {
            let (llegadas, recibidas) = mpsc::unbounded_channel();
            let proveedor = Self {
                resumen: resumen.to_string(),
                compuerta: Semaphore::new(0),
                llegadas,
                peticiones: AtomicUsize::new(0),
                normales: Mutex::new(VecDeque::from(normales)),
                cancelar_turno,
            };
            (proveedor, recibidas)
        }

        /// Peticiones de resumen recibidas hasta ahora, retenidas o no.
        fn peticiones_de_resumen(&self) -> usize {
            self.peticiones.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl Provider for ResumenRetenido {
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<ProviderStream> {
            if es_peticion_de_resumen(&req) {
                let orden = self.peticiones.fetch_add(1, Ordering::SeqCst);
                let _ = self.llegadas.send(());
                if orden == 0 {
                    self.compuerta
                        .acquire()
                        .await
                        .expect("la compuerta no se cierra")
                        .forget();
                }
                return Ok(Box::pin(futures::stream::iter(vec![
                    StreamChunk::TextDelta(self.resumen.clone()),
                    usage_chunk(700, 90),
                    StreamChunk::Done(FinishReason::Stop),
                ])));
            }
            if let Some(cancelar) = &self.cancelar_turno {
                cancelar.cancel();
            }
            let guion = self
                .normales
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| {
                    vec![
                        StreamChunk::TextDelta("hecho".into()),
                        StreamChunk::Done(FinishReason::Stop),
                    ]
                });
            Ok(Box::pin(futures::stream::iter(guion)))
        }
    }

    struct EntornoEnSegundoPlano {
        _dir: tempfile::TempDir,
        store: Arc<SqliteStore>,
        sid: SessionId,
        proveedor: Arc<ResumenRetenido>,
        llegadas: mpsc::UnboundedReceiver<()>,
        bus: EventBus,
        agente: AgentLoop,
    }

    /// Entorno de un agente con el proveedor controlable, ventana de 50 000 tokens y el historial
    /// dado ya guardado. `background` fija `[compaction] background`.
    async fn entorno_en_segundo_plano(
        proveedor: (ResumenRetenido, mpsc::UnboundedReceiver<()>),
        historial: &[Message],
        background: bool,
        presupuesto: BudgetConfig,
    ) -> EntornoEnSegundoPlano {
        let (proveedor, llegadas) = proveedor;
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("b.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let sid = store.create_session("/p").await.unwrap();
        for mensaje in historial {
            store.append_message(sid, mensaje).await.unwrap();
        }
        let proveedor = Arc::new(proveedor);
        let cfg = CodeLoomConfig {
            provider: crate::config::ProviderConfig {
                kind: crate::config::ProviderKind::OpenAiCompat,
                base_url: "http://x".into(),
                api_key_env: "K".into(),
            },
            model: "m".into(),
            max_steps: 10,
            effort: None,
        };
        let bus = EventBus::new(1024);
        let agente = AgentLoop::new(
            proveedor.clone(),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            bus.clone(),
            Arc::new(PermissionService::allow_all()),
            cfg,
            dir.path().to_path_buf(),
        )
        .with_catalog(catalogo_de_ventana(50_000))
        .with_compaction(compact::CompactionConfig { background })
        .with_budget(presupuesto);
        EntornoEnSegundoPlano {
            _dir: dir,
            store,
            sid,
            proveedor,
            llegadas,
            bus,
            agente,
        }
    }

    /// Espera al turno con un límite generoso. El límite solo evita que un fallo deje colgada la
    /// suite; en el caso correcto no añade tiempo.
    async fn sin_bloquear<T>(futuro: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(30), futuro)
            .await
            .expect("el turno no debe quedarse esperando")
    }

    /// Recoge los eventos hasta `BackgroundCompactionReady`, incluido.
    async fn hasta_listo(rx: &mut broadcast::Receiver<LiveEvent>) -> Vec<LiveEvent> {
        sin_bloquear(async {
            let mut recogidos = Vec::new();
            loop {
                let evento = rx.recv().await.expect("el bus sigue abierto");
                let listo = matches!(evento, LiveEvent::BackgroundCompactionReady { .. });
                recogidos.push(evento);
                if listo {
                    return recogidos;
                }
            }
        })
        .await
    }

    /// Saca de `rx` todos los eventos que ya estén publicados.
    fn vaciar(rx: &mut broadcast::Receiver<LiveEvent>) -> Vec<LiveEvent> {
        let mut eventos = Vec::new();
        while let Ok(evento) = rx.try_recv() {
            eventos.push(evento);
        }
        eventos
    }

    /// Activaciones de las compactaciones publicadas en `eventos`, en orden.
    fn activaciones(eventos: &[LiveEvent]) -> Vec<Activation> {
        eventos
            .iter()
            .filter_map(|evento| match evento {
                LiveEvent::ContextCompacted { activation, .. } => Some(activation.clone()),
                _ => None,
            })
            .collect()
    }

    /// Avisos de presupuesto de `eventos`, como `(porcentaje, usado)`.
    fn avisos_de(eventos: &[LiveEvent]) -> Vec<(u8, f64)> {
        eventos
            .iter()
            .filter_map(|evento| match evento {
                LiveEvent::BudgetWarning {
                    level_percent,
                    used,
                    ..
                } => Some((*level_percent, *used)),
                _ => None,
            })
            .collect()
    }

    /// Con 31 000 tokens y una ventana de 50 000 se cruza el 60 % (30 000) y no el 80 % (40 000).
    #[tokio::test]
    async fn background_summary_starts_at_the_buffer_ratio_without_blocking_the_turn() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        let mut rx = e.bus.subscribe();

        let turno = sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();

        // El turno terminó mientras el resumen seguía retenido en el proveedor.
        assert_eq!(turno.final_text, "hecho");
        e.llegadas
            .recv()
            .await
            .expect("llegó la petición de resumen");
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Running
        );
        let eventos = vaciar(&mut rx);
        let arranque = eventos.iter().find_map(|evento| match evento {
            LiveEvent::BackgroundCompactionStarted { tokens_before } => Some(*tokens_before),
            _ => None,
        });
        assert!(
            arranque.is_some_and(|tokens| tokens >= 30_000),
            "se publica el arranque con los tokens del momento: {arranque:?}"
        );
        assert!(
            activaciones(&eventos).is_empty(),
            "el turno no compacta por su cuenta"
        );
        assert_eq!(e.proveedor.peticiones_de_resumen(), 1);

        // Al liberar la tarea, el resumen queda listo.
        e.proveedor.compuerta.add_permits(1);
        hasta_listo(&mut rx).await;
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Ready
        );
    }

    /// Un resumen listo se activa al cruzar el 80 %, sin ninguna llamada al modelo para resumir.
    #[tokio::test]
    async fn a_ready_summary_is_activated_without_a_model_call_at_the_compact_ratio() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        e.proveedor.compuerta.add_permits(1);
        let mut rx = e.bus.subscribe();
        sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();
        hasta_listo(&mut rx).await;
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Ready
        );

        // Diez mensajes más llevan el contexto por encima del 80 % (40 000 tokens).
        for mensaje in historial_grande(10) {
            e.store.append_message(e.sid, &mensaje).await.unwrap();
        }
        let turno = sin_bloquear(
            e.agente
                .run(e.sid, "siguiente".into(), CancellationToken::new()),
        )
        .await
        .unwrap();

        assert_eq!(turno.final_text, "hecho");
        assert_eq!(activaciones(&vaciar(&mut rx)), vec![Activation::Background]);
        assert_eq!(
            e.proveedor.peticiones_de_resumen(),
            1,
            "solo la petición del arranque"
        );
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Idle
        );
    }

    /// Con el resumen en curso al cruzar el 80 %, el turno espera a esa misma tarea y no lanza
    /// otra. La espera se registra antes de liberar la compuerta: el `join!` hace que la llamada
    /// se quede dormida en `wait` en su primera vuelta, y la liberación ocurre en la segunda.
    #[tokio::test]
    async fn a_running_summary_is_awaited_instead_of_starting_a_second_one() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        let mut rx = e.bus.subscribe();
        sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();
        e.llegadas
            .recv()
            .await
            .expect("llegó la petición de resumen");
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Running
        );

        let historia = e.store.messages(e.sid).await.unwrap();
        let cancel = CancellationToken::new();
        let liberar = async {
            tokio::task::yield_now().await;
            e.proveedor.compuerta.add_permits(1);
        };
        let (activado, ()) = tokio::join!(
            e.agente
                .compact_or_activate(e.sid, &historia, 40_000, &cancel),
            liberar
        );

        assert!(activado.unwrap());
        assert_eq!(e.proveedor.peticiones_de_resumen(), 1, "no se lanza otra");
        let activacion = activaciones(&vaciar(&mut rx));
        assert!(
            matches!(
                activacion.as_slice(),
                [Activation::BackgroundAwaited { .. }]
            ),
            "se espera a la tarea en curso: {activacion:?}"
        );
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Idle
        );
    }

    /// Si la tarea no termina dentro del plazo, se cancela y la compactación cae a la vía síncrona.
    /// El tiempo es virtual: la espera de `BACKGROUND_WAIT_SECS` vence sin tardar de verdad.
    #[tokio::test]
    async fn a_timed_out_summary_falls_back_to_the_synchronous_path() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        let mut rx = e.bus.subscribe();
        sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();
        e.llegadas
            .recv()
            .await
            .expect("llegó la petición de resumen");
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Running
        );

        let historia = e.store.messages(e.sid).await.unwrap();
        let cancel = CancellationToken::new();
        tokio::time::pause();
        let compactado = e
            .agente
            .compact_or_activate(e.sid, &historia, 40_000, &cancel)
            .await
            .unwrap();

        assert!(compactado);
        assert_eq!(activaciones(&vaciar(&mut rx)), vec![Activation::Sync]);
        assert_eq!(
            e.proveedor.peticiones_de_resumen(),
            2,
            "la de segundo plano, que caduca, y la síncrona"
        );
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Idle
        );
    }

    /// Un resumen listo deja de ser vigente si entre medias se persistió otro resumen: se descarta
    /// y la compactación usa la vía síncrona.
    #[tokio::test]
    async fn a_stale_summary_is_discarded_when_another_was_persisted() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        e.proveedor.compuerta.add_permits(1);
        let mut rx = e.bus.subscribe();
        sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();
        hasta_listo(&mut rx).await;

        // Otro resumen entra en el registro por otra vía (aquí, a mano) y luego llegan mensajes.
        let antes = e.store.messages(e.sid).await.unwrap();
        let corte = compact::choose_cut(&antes).expect("hay historia que resumir");
        e.store
            .append_message(e.sid, &compact::summary_message(corte, "otro resumen"))
            .await
            .unwrap();
        for mensaje in historial_grande(35) {
            e.store.append_message(e.sid, &mensaje).await.unwrap();
        }
        let historia = e.store.messages(e.sid).await.unwrap();
        assert!(
            compact::should_compact(&historia, 0, 50_000),
            "precondición: el contexto supera el 80 %"
        );

        let turno = sin_bloquear(
            e.agente
                .run(e.sid, "siguiente".into(), CancellationToken::new()),
        )
        .await
        .unwrap();

        assert_eq!(turno.final_text, "hecho");
        let eventos = vaciar(&mut rx);
        assert_eq!(activaciones(&eventos), vec![Activation::Sync]);
        assert!(
            eventos
                .iter()
                .any(|evento| matches!(evento, LiveEvent::BackgroundCompactionDiscarded)),
            "la interfaz debe saber que el listo caducó"
        );
        assert_eq!(
            e.proveedor.peticiones_de_resumen(),
            2,
            "el listo caducó: una de segundo plano y otra síncrona"
        );
    }

    /// Si el resumen en segundo plano no sirve (aquí, vacío), la sesión vuelve a reposo y se
    /// publica el descarte: sin él, la interfaz seguiría mostrando «resumiendo…».
    #[tokio::test]
    async fn a_background_summary_that_fails_publishes_the_discard() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("", vec![], None),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        e.proveedor.compuerta.add_permits(1);
        let mut rx = e.bus.subscribe();

        sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();
        let descartado = sin_bloquear(async {
            loop {
                let evento = rx.recv().await.expect("el bus sigue abierto");
                match evento {
                    LiveEvent::BackgroundCompactionDiscarded => return true,
                    LiveEvent::BackgroundCompactionReady { .. } => return false,
                    _ => {}
                }
            }
        })
        .await;

        assert!(descartado, "un resumen vacío no queda listo");
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Idle
        );
    }

    /// Con `background = false` no hay ninguna petición de resumen antes del 80 %.
    #[tokio::test]
    async fn background_off_behaves_exactly_as_before() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            false,
            BudgetConfig::default(),
        )
        .await;
        let mut rx = e.bus.subscribe();

        let turno = sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();

        assert_eq!(turno.final_text, "hecho");
        assert_eq!(
            e.proveedor.peticiones_de_resumen(),
            0,
            "ninguna petición de resumen antes del 80 %"
        );
        let eventos = vaciar(&mut rx);
        assert!(!eventos.iter().any(|evento| matches!(
            evento,
            LiveEvent::BackgroundCompactionStarted { .. }
                | LiveEvent::BackgroundCompactionReady { .. }
        )));
        assert!(activaciones(&eventos).is_empty());
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Idle
        );
    }

    /// Cancelar el turno después de lanzar el resumen no mata la tarea: sigue hasta quedar listo.
    #[tokio::test]
    async fn cancelling_the_turn_does_not_kill_the_background_summary() {
        let cancel = CancellationToken::new();
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], Some(cancel.clone())),
            &historial_grande(31),
            true,
            BudgetConfig::default(),
        )
        .await;
        let mut rx = e.bus.subscribe();

        sin_bloquear(e.agente.run(e.sid, "hola".into(), cancel.clone()))
            .await
            .unwrap();
        assert!(
            cancel.is_cancelled(),
            "el turno se canceló tras lanzar el resumen"
        );

        e.llegadas
            .recv()
            .await
            .expect("llegó la petición de resumen");
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Running
        );
        e.proveedor.compuerta.add_permits(1);
        hasta_listo(&mut rx).await;
        assert_eq!(
            e.agente.background.state(e.sid),
            background::SlotState::Ready
        );
    }

    /// La llamada del resumen en segundo plano se apunta como cualquier otra y cuenta para el
    /// presupuesto: 790 tokens sobre un límite de 500 generan avisos.
    #[tokio::test]
    async fn the_background_call_usage_is_recorded_and_counts_for_the_budget() {
        let mut e = entorno_en_segundo_plano(
            ResumenRetenido::nuevo("resumen", vec![], None),
            &historial_grande(31),
            true,
            presupuesto_de_tokens(500),
        )
        .await;
        e.proveedor.compuerta.add_permits(1);
        let mut rx = e.bus.subscribe();
        sin_bloquear(e.agente.run(e.sid, "hola".into(), CancellationToken::new()))
            .await
            .unwrap();

        // Los avisos se publican antes que `Ready`, así que se recogen con los eventos previos.
        let hasta = hasta_listo(&mut rx).await;

        let apuntes = e.store.session_usage(e.sid).await.unwrap();
        assert_eq!(apuntes.len(), 1, "un apunte por la llamada de resumen");
        assert_eq!(
            (apuntes[0].input_tokens, apuntes[0].output_tokens),
            (700, 90)
        );
        assert!(
            !avisos_de(&hasta).is_empty(),
            "el consumo del resumen cruza el presupuesto"
        );
    }
}
