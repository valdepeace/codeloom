pub mod changes;
mod clear;
pub mod compact;
pub mod dispatch;
pub mod stop;

use crate::catalog::Catalog;
use crate::config::{CodeLoomConfig, ConfirmDelegation};
use crate::error::Result;
use crate::event::{EventBus, LiveEvent};
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
        .with_catalog(self.catalog.clone());
        let lanzador = match &self.registry {
            Some(registro) => lanzador.with_registry(registro.clone()),
            None => lanzador,
        };
        let lanzador = match &self.spill {
            Some(spill) => lanzador.with_spill(spill.clone()),
            None => lanzador,
        };
        Arc::new(lanzador)
    }

    /// Guarda el apunte de consumo de una respuesta, con el modelo y la URL de la configuración
    /// vigente y los precios del catálogo en ese instante. Un fallo se registra y no corta el turno.
    async fn record_usage(&self, session: SessionId, usage: &crate::message::Usage) {
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
        if let Err(error) = self.store.record_usage(session, &record).await {
            tracing::warn!(error = %error, "no se pudo guardar el apunte de consumo");
        }
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

        let mut system = crate::config::context::load_context_files(&self.cwd).join("\n\n");
        // Con la herramienta `subagent` y un registro, el modelo recibe la lista vigente de agentes:
        // se lee en cada turno, de modo que la recarga en caliente también le llega.
        if let (Some(_), Some(registro)) = (&self.subagents, &self.registry) {
            if !system.is_empty() {
                system.push_str("\n\n");
            }
            system.push_str(&registro.snapshot().prompt_section(self.delegation_confirm));
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
            system.push_str(extra);
        }
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

        loop {
            if cancel.is_cancelled() {
                break;
            }
            self.flush_subagent_notices(session).await?;
            history = self.store.messages(session).await?;
            if crate::agent::stop::should_stop(&history, steps, config.max_steps) {
                break;
            }
            steps += 1;
            self.bus.publish(LiveEvent::StepStarted {
                step: steps,
                max_steps: config.max_steps,
            });
            let req = ProviderRequest {
                model: config.model.clone(),
                system: system.clone(),
                messages: clear::clear_old_tool_results(&history, clear::KEEP_TOOL_RESULTS),
                tools: self.tools.specs(),
                temperature: 0.0,
                max_tokens: 4096,
                effort: config.effort.clone(),
            };
            let mut stream = tokio::select! {
                _ = cancel.cancelled() => break,
                result = provider.stream(req, cancel.clone()) => result?,
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
    /// propaga como `Err`.
    // Se retira en la tarea siguiente, cuando `run_turn` lo invoque.
    #[allow(dead_code)]
    async fn compact(
        &self,
        session: SessionId,
        history: &[Message],
        tokens_before: u64,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        let Some(cut) = compact::choose_cut(history) else {
            return Ok(false);
        };
        let provider = self.provider.read().unwrap().clone();
        let config = self.config.read().unwrap().clone();

        // Lo que se resume: lo efectivo anterior al corte (incluido el resumen previo, si lo hay).
        let anteriores: Vec<Message> = compact::effective_indices(history)
            .into_iter()
            .filter(|&i| i < cut)
            .map(|i| history[i].clone())
            .collect();
        let mut mensajes = clear::clear_old_tool_results(&anteriores, clear::KEEP_TOOL_RESULTS);
        mensajes.push(Message::user(INSTRUCCION_DE_RESUMEN));

        let req = ProviderRequest {
            model: config.model.clone(),
            system: compact::SUMMARY_SYSTEM_PROMPT.to_string(),
            messages: mensajes,
            tools: vec![],
            temperature: 0.0,
            max_tokens: compact::SUMMARY_MAX_TOKENS,
            effort: None,
        };
        let mut stream = tokio::select! {
            _ = cancel.cancelled() => return Ok(false),
            result = provider.stream(req, cancel.clone()) => result?,
        };
        let mut recogido = String::new();
        // Un flujo que acaba en error o sin `Done` ha podido quedarse a medias: un resumen
        // truncado por un fallo se descarta en lugar de presentarse como bueno.
        let mut cierre: Option<FinishReason> = None;
        loop {
            let chunk = tokio::select! {
                _ = cancel.cancelled() => return Ok(false),
                chunk = stream.next() => chunk,
            };
            match chunk {
                Some(StreamChunk::TextDelta(t)) => recogido.push_str(&t),
                Some(StreamChunk::Done(razon)) => {
                    cierre = Some(razon);
                    break;
                }
                None => break,
                Some(_) => {}
            }
        }
        if cancel.is_cancelled() {
            return Ok(false);
        }
        if matches!(cierre, None | Some(FinishReason::Error)) {
            tracing::warn!("el flujo del resumen terminó sin cierre válido; se descarta");
            return Ok(false);
        }

        let recogido = recogido.trim();
        if recogido.is_empty() {
            return Ok(false);
        }
        // El marcador de `limit_bytes` se suma al texto conservado: se le reserva hueco para que
        // el resumen entero respete `MAX_SUMMARY_BYTES`.
        let resumen = limit_bytes(
            recogido,
            compact::MAX_SUMMARY_BYTES.saturating_sub(MARCADOR_DE_RECORTE),
        );
        let mensaje = compact::summary_message(cut, &resumen);
        self.store.append_message(session, &mensaje).await?;
        self.bus.publish(LiveEvent::MessageAdded(mensaje));
        self.bus.publish(LiveEvent::ContextCompacted {
            replaced: cut,
            tokens_before,
            summary_bytes: resumen.len(),
        });
        Ok(true)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{Capabilities, ProviderStream, StreamChunk};
    use crate::session::sqlite::SqliteStore;
    use crate::tools::{ExecutionMode, Tool, ToolCtx, ToolOutput};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;

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
}
