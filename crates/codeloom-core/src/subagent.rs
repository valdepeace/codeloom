use crate::a2a::Artifact;
use crate::agent::AgentLoop;
use crate::budget::BudgetConfig;
use crate::catalog::Catalog;
use crate::config::CodeLoomConfig;
use crate::error::{CoreError, Result};
use crate::event::{EventBus, LiveEvent};
use crate::job::{JobId, JobRegistry, JobResult, JobSpec};
use crate::permission::PermissionService;
use crate::provider::Provider;
use crate::registry::{AgentEntry, AgentRegistry, Approval, Execution, AGENTE_POR_DEFECTO};
use crate::session::{SessionId, SessionStore, SubagentDescriptor};
use crate::subagent_input::{ChildResponder, InputBroker, PendingInput};
use crate::task::{TaskState, TaskStatus};
use crate::tools::spill::ToolSpill;
use crate::tools::ToolRegistry;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;

#[async_trait]
pub trait SubagentLauncher: Send + Sync {
    async fn start(
        &self,
        parent: SessionId,
        label: String,
        prompt: String,
        agent: Option<String>,
        cancel: CancellationToken,
    ) -> Result<(SessionId, JobId)>;

    async fn control(
        &self,
        parent: SessionId,
        child: SessionId,
        action: &str,
        message: Option<String>,
    ) -> Result<()>;

    /// `CancelTask` de A2A: pide cancelar al hijo y devuelve su tarea actualizada. Falla con
    /// `TaskNotFound` si no es hijo de `parent` y con `TaskNotCancelable` si ya terminó. El éxito
    /// no está garantizado: si el hijo no se asienta a tiempo se devuelve su estado actual, sin
    /// afirmar que se canceló.
    async fn cancel(&self, _parent: SessionId, _child: SessionId) -> Result<SubagentDescriptor> {
        Err(CoreError::Tool("subagent cancel unavailable".into()))
    }

    async fn list(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>>;

    async fn deliver_notices(&self, parent: SessionId) -> Result<Vec<crate::message::Message>>;

    /// Responde la pregunta pendiente de un hijo (`input-required`).
    async fn answer(
        &self,
        _parent: SessionId,
        _child: SessionId,
        _answers: Vec<String>,
    ) -> Result<()> {
        Err(CoreError::Tool("subagent answers unavailable".into()))
    }

    /// Avisos de las preguntas nuevas de los hijos; cada una sale una sola vez.
    async fn pending_input_notices(
        &self,
        _parent: SessionId,
    ) -> Result<Vec<crate::message::Message>> {
        Ok(Vec::new())
    }
}

pub struct InProcessSubagentLauncher {
    provider: Arc<RwLock<Arc<dyn Provider>>>,
    store: Arc<dyn SessionStore>,
    tools: Arc<ToolRegistry>,
    permission: Arc<PermissionService>,
    config: Arc<RwLock<CodeLoomConfig>>,
    cwd: PathBuf,
    jobs: Arc<dyn JobRegistry>,
    active: Arc<Semaphore>,
    bus: EventBus,
    broker: Arc<InputBroker>,
    /// Catálogo compartido con el agente padre; cada hijo lo usa para sus apuntes de consumo.
    catalog: Arc<RwLock<Catalog>>,
    /// Registro de agentes. Sin él solo existe el agente `default`, y el lanzador se comporta
    /// exactamente como antes de la tarea 6.
    registry: Option<Arc<AgentRegistry>>,
    /// Volcado de salidas recortadas que reciben los hijos; `None` como en el agente padre.
    spill: Option<ToolSpill>,
    /// Presupuesto del padre: cada hijo lo recibe y suma su consumo al total de la familia.
    budget: BudgetConfig,
    /// Lo máximo que `cancel` espera a que el hijo se asiente; `CANCEL_WAIT` salvo en las pruebas.
    cancel_wait: std::time::Duration,
}

/// Descarta la pregunta pendiente del hijo al terminar su trabajo, sea cual sea la
/// causa (final normal, cancelación o aborto), para no dejar entradas huérfanas.
struct DiscardOnExit {
    broker: Arc<InputBroker>,
    child: SessionId,
}

impl Drop for DiscardOnExit {
    fn drop(&mut self) {
        self.broker.discard(self.child);
    }
}

/// Lo máximo que `cancel` espera a que el hijo se asiente tras la orden de parar.
const CANCEL_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// Cada cuánto `cancel` relee el estado del hijo mientras espera.
const CANCEL_POLL: std::time::Duration = std::time::Duration::from_millis(10);

/// Mensaje de la tarea rechazada por capacidad agotada. Es el mismo error que recibe el padre.
const MENSAJE_CAPACIDAD: &str = "maximum of 8 active subagents reached";

/// Único punto del lanzador que cambia el estado de la tarea de un hijo. Valida la
/// transición con `TaskState::can_transition_to`, la persiste y publica `SubagentChanged`.
pub struct TaskTracker {
    store: Arc<dyn SessionStore>,
    bus: EventBus,
}

impl TaskTracker {
    pub(crate) fn new(store: Arc<dyn SessionStore>, bus: EventBus) -> Self {
        Self { store, bus }
    }

    /// Lleva la tarea del hijo a `status.state`. Si el estado actual no admite la
    /// transición (un terminal no se reinicia) devuelve error y no toca el almacén.
    pub(crate) async fn transition(
        &self,
        parent: SessionId,
        child: SessionId,
        status: TaskStatus,
    ) -> Result<()> {
        let mut descriptor = self.descriptor(parent, child).await?;
        if !descriptor.status.state.can_transition_to(status.state) {
            return Err(CoreError::Store(format!(
                "transición no admitida: {} -> {}",
                descriptor.status.state.proto_name(),
                status.state.proto_name()
            )));
        }
        self.store
            .update_subagent_status(parent, child, status.clone())
            .await?;
        descriptor.status = status;
        self.bus.publish(LiveEvent::SubagentChanged(descriptor));
        Ok(())
    }

    /// Publica el descriptor guardado del hijo tal cual está, p. ej. tras fijar su resultado.
    pub(crate) async fn publish(&self, parent: SessionId, child: SessionId) -> Result<()> {
        let descriptor = self.descriptor(parent, child).await?;
        self.bus.publish(LiveEvent::SubagentChanged(descriptor));
        Ok(())
    }

    async fn descriptor(&self, parent: SessionId, child: SessionId) -> Result<SubagentDescriptor> {
        self.store
            .list_subagents(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .ok_or_else(|| CoreError::Store(format!("unknown subagent {}", child.0)))
    }
}

impl InProcessSubagentLauncher {
    /// Descriptor vigente de un hijo de `parent`, o `TaskNotFound`.
    async fn descriptor_of(
        &self,
        parent: SessionId,
        child: SessionId,
    ) -> Result<SubagentDescriptor> {
        self.store
            .list_subagents(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .ok_or(CoreError::TaskNotFound(child.0))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: Arc<RwLock<Arc<dyn Provider>>>,
        store: Arc<dyn SessionStore>,
        tools: Arc<ToolRegistry>,
        permission: Arc<PermissionService>,
        config: Arc<RwLock<CodeLoomConfig>>,
        cwd: PathBuf,
        jobs: Arc<dyn JobRegistry>,
        bus: EventBus,
    ) -> Self {
        Self {
            provider,
            store,
            tools: Arc::new(
                tools
                    .without("subagent")
                    .without("subagent_control")
                    .without("subagent_list"),
            ),
            permission,
            config,
            cwd,
            jobs,
            active: Arc::new(Semaphore::new(8)),
            bus,
            broker: Arc::new(InputBroker::default()),
            catalog: Arc::new(RwLock::new(Catalog::default())),
            registry: None,
            spill: None,
            budget: BudgetConfig::default(),
            cancel_wait: CANCEL_WAIT,
        }
    }

    /// Cambia cuánto espera `cancel` a que el hijo se asiente. Existe para las pruebas, que no
    /// pueden pausar el reloj porque el almacén usa una piscina de conexiones con plazos reales.
    #[cfg(test)]
    fn with_cancel_wait(mut self, espera: std::time::Duration) -> Self {
        self.cancel_wait = espera;
        self
    }

    /// Comparte el presupuesto de gasto con los hijos que lance: avisan con el total de la familia.
    pub fn with_budget(mut self, budget: BudgetConfig) -> Self {
        self.budget = budget;
        self
    }

    /// Comparte el volcado a disco con los hijos que lance, como `with_catalog`.
    pub fn with_spill(mut self, spill: ToolSpill) -> Self {
        self.spill = Some(spill);
        self
    }

    /// Comparte el registro de agentes: `start` resuelve ahí el agente pedido.
    pub fn with_registry(mut self, registry: Arc<AgentRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Comparte el catálogo del agente padre con los hijos que lance.
    pub fn with_catalog(mut self, catalog: Arc<RwLock<Catalog>>) -> Self {
        self.catalog = catalog;
        self
    }

    /// Preguntas pendientes de los hijos de un padre (`input-required`).
    pub fn pending_inputs(&self, parent: SessionId) -> Vec<PendingInput> {
        self.broker.pending(parent)
    }

    /// Guarda un hijo rechazado por capacidad agotada: tarea `rejected` sin trabajo.
    async fn registrar_rechazo(
        &self,
        parent: SessionId,
        label: &str,
        project: &str,
        agent: Option<String>,
        agent_origin: Option<String>,
    ) -> Result<()> {
        let child = self.store.create_session(project).await?;
        let descriptor = SubagentDescriptor {
            parent,
            child,
            job_id: String::new(),
            label: label.to_string(),
            status: TaskStatus::new(TaskState::Rejected).with_message(MENSAJE_CAPACIDAD),
            artifacts: Vec::new(),
            notice_delivered: false,
            agent,
            agent_origin,
            reference_task_ids: Vec::new(),
        };
        self.store.save_subagent(&descriptor).await?;
        self.bus.publish(LiveEvent::SubagentChanged(descriptor));
        Ok(())
    }

    /// Resuelve el agente pedido (`None` = `default`) en la instantánea vigente y devuelve su
    /// entrada copiada por valor: una recarga posterior no la altera. Sin registro, solo
    /// `default` es válido y se devuelve `None` (sin perfil que aplicar). Todo rechazo es un
    /// error de validación: quien llama no ha creado aún tarea ni puesto.
    fn resolver_agente(&self, agent: Option<&str>) -> Result<Option<AgentEntry>> {
        let nombre = agent.unwrap_or(AGENTE_POR_DEFECTO);
        let Some(registro) = &self.registry else {
            if nombre == AGENTE_POR_DEFECTO {
                return Ok(None);
            }
            return Err(CoreError::Tool(format!(
                "no hay registro de agentes configurado: no se puede usar el agente «{nombre}»"
            )));
        };
        let instantanea = registro.snapshot();
        let entrada = instantanea.get(nombre).ok_or_else(|| {
            CoreError::Tool(format!("agente «{nombre}» inexistente en el registro"))
        })?;
        // Se comprueba antes que la aprobación: un remoto figura como pendiente y el motivo real es otro.
        if matches!(entrada.execution, Execution::Remote) {
            return Err(CoreError::Tool(format!(
                "agente remoto no disponible: «{nombre}»"
            )));
        }
        if instantanea.approval(nombre) == Some(Approval::Pending) {
            return Err(CoreError::Tool(format!(
                "agente «{nombre}» pendiente de aprobación: apruébalo antes de usarlo"
            )));
        }
        if let Execution::Local(spec) = &entrada.execution {
            if let Some(modelo) = &spec.model {
                // Sin catálogo, o sin proveedor con esta URL base, no hay nada que comprobar.
                let base_url = self.config.read().unwrap().provider.base_url.clone();
                let conocido = self.catalog.read().unwrap().knows_model(&base_url, modelo);
                if conocido == Some(false) {
                    return Err(CoreError::Tool(format!(
                        "el modelo «{modelo}» del agente «{nombre}» no figura en el catálogo"
                    )));
                }
            }
        }
        Ok(Some(entrada.clone()))
    }
}

/// Resumen de una petición de permiso para el mensaje de `auth-required`: herramienta, acción
/// y detalle recortado, de modo que el padre vea qué se pide sin volcar argumentos enteros.
pub(crate) fn resumen_de_permiso(tool: &str, action: &str, detail: &str) -> String {
    const LIMITE: usize = 160;
    let detalle = detail.trim();
    let resumen = if detalle.is_empty() {
        format!("{tool} {action}")
    } else {
        format!("{tool} {action}: {detalle}")
    };
    if resumen.chars().count() <= LIMITE {
        resumen
    } else {
        let mut recortado: String = resumen.chars().take(LIMITE - 1).collect();
        recortado.push('…');
        recortado
    }
}

#[async_trait]
impl SubagentLauncher for InProcessSubagentLauncher {
    async fn start(
        &self,
        parent: SessionId,
        label: String,
        prompt: String,
        agent: Option<String>,
        parent_cancel: CancellationToken,
    ) -> Result<(SessionId, JobId)> {
        if prompt.trim().is_empty() || prompt.len() > 32_768 || label.len() > 128 {
            return Err(CoreError::Tool("invalid subagent prompt or label".into()));
        }
        // El agente se valida antes de reservar puesto o crear tarea: un rechazo no deja rastro.
        let perfil = self.resolver_agente(agent.as_deref())?;
        let (agente, origen) = match &perfil {
            Some(entrada) => (Some(entrada.name.clone()), Some(entrada.origin.as_text())),
            None => (None, None),
        };
        let project = self.cwd.to_string_lossy().into_owned();
        let permit = match self.active.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                // La validación ya pasó: la capacidad agotada sí deja constancia como tarea `rejected`.
                self.registrar_rechazo(parent, &label, &project, agente.clone(), origen.clone())
                    .await?;
                return Err(CoreError::Tool(MENSAJE_CAPACIDAD.into()));
            }
        };
        let child = self.store.create_session(&project).await?;
        let tracker = Arc::new(TaskTracker::new(self.store.clone(), self.bus.clone()));
        let child_store = self.store.clone();
        let child_tools = self.tools.clone();
        let child_provider = self.provider.read().unwrap().clone();
        // Cada hijo tiene su propio servicio de permisos: delega en el del padre y
        // encamina sus preguntas (`ask_user`) al buzón de este lanzador.
        let child_permission = Arc::new(PermissionService::with_responder(Box::new(
            ChildResponder::new(
                self.permission.clone(),
                self.broker.clone(),
                parent,
                child,
                label.clone(),
                self.bus.clone(),
                tracker.clone(),
            ),
        )));
        let child_broker = self.broker.clone();
        // Configuración del hijo: la del padre, con lo que declare el perfil. Lo que el perfil
        // no declara se hereda. Se copia aquí, al arrancar, y la tarea ya no depende del registro.
        let mut child_config = self.config.read().unwrap().clone();
        let mut child_system = None;
        if let Some(AgentEntry {
            execution: Execution::Local(spec),
            ..
        }) = &perfil
        {
            if let Some(modelo) = &spec.model {
                child_config.model = modelo.clone();
            }
            if let Some(esfuerzo) = &spec.effort {
                child_config.effort = Some(esfuerzo.clone());
            }
            // `max_turns` limita los pasos del bucle del hijo: es el límite que ya lleva `max_steps`.
            if let Some(turnos) = spec.max_turns {
                child_config.max_steps = usize::try_from(turnos).unwrap_or(usize::MAX);
            }
            // `max_depth` leído y no aplicado: el hijo no recibe la herramienta `subagent`, así
            // que no puede anidar subagentes y no hay profundidad que limitar todavía.
            let sistema = spec.system_prompt();
            if !sistema.trim().is_empty() {
                child_system = Some(sistema);
            }
        }
        let child_cwd = self.cwd.clone();
        let child_jobs = self.jobs.clone();
        let child_catalog = self.catalog.clone();
        let child_spill = self.spill.clone();
        let child_budget = self.budget.clone();
        let descriptor_label = label.clone();
        // El bus del hijo se crea y se suscribe antes de lanzarlo, para no perder ningún
        // evento de su arranque. Su canal se cierra cuando se sueltan todos sus emisores.
        let child_bus = EventBus::new(256);
        let mut child_rx = child_bus.subscribe();
        // La compuerta retiene al hijo hasta que su tarea `submitted` está guardada.
        let (abrir_puerta, puerta) = tokio::sync::oneshot::channel::<()>();
        let cuerpo_tracker = tracker.clone();
        let job = self.jobs.start(JobSpec {
            kind: "subagent".into(),
            label,
            owner: parent,
            output_limit: 64 * 1024,
            run: Box::new(move |handle, job_cancel| {
                Box::pin(async move {
                    let _permit = permit;
                    let _discard = DiscardOnExit {
                        broker: child_broker,
                        child,
                    };
                    // Si el lanzador no llega a abrir la compuerta, el hijo no arranca.
                    if puerta.await.is_err() || job_cancel.is_cancelled() {
                        return Ok(None);
                    }
                    if let Err(error) = cuerpo_tracker
                        .transition(parent, child, TaskStatus::new(TaskState::Working))
                        .await
                    {
                        tracing::warn!("persist subagent status: {error}");
                    }
                    let mut child_agent = AgentLoop::new(
                        child_provider,
                        child_store.clone(),
                        child_tools,
                        child_bus,
                        child_permission,
                        child_config,
                        child_cwd,
                    )
                    .with_jobs(child_jobs)
                    .with_catalog(child_catalog)
                    .with_system_extra(child_system)
                    .with_budget(child_budget)
                    .with_budget_family(parent);
                    if let Some(spill) = child_spill {
                        child_agent = child_agent.with_spill(spill);
                    }
                    let child_cancel = CancellationToken::new();
                    let outcome = tokio::select! {
                        result = child_agent.run(child, prompt, child_cancel.clone()) => result?,
                        _ = job_cancel.cancelled() => {
                            child_cancel.cancel();
                            return Ok(None);
                        }
                        _ = parent_cancel.cancelled() => {
                            child_cancel.cancel();
                            // Cancelar al padre cancela al hijo: el trabajo se mata para que la
                            // tarea acabe en `canceled` y no en `completed` sin resultado.
                            job_cancel.cancel();
                            return Ok(None);
                        }
                    };
                    // El agente ya no emite: soltarlo cierra el bus del hijo y permite
                    // que la tarea de reenvío termine.
                    drop(child_agent);
                    let text = outcome.final_text;
                    child_store
                        .complete_subagent(parent, child, &[Artifact::text_artifact(text.as_str())])
                        .await?;
                    if let Err(error) = cuerpo_tracker.publish(parent, child).await {
                        tracing::warn!("publish subagent status: {error}");
                    }
                    handle.append(&text);
                    Ok(Some(JobResult { content: text }))
                })
            }),
        })?;
        // Reenvío filtrado: solo los eventos relevantes del hijo llegan al padre,
        // etiquetados con su identidad. La tarea termina cuando el bus del hijo se cierra.
        let forward_bus = self.bus.clone();
        tokio::spawn(async move {
            loop {
                match child_rx.recv().await {
                    Ok(event) => {
                        if event.is_child_relevant() {
                            forward_bus.publish(LiveEvent::ChildEvent {
                                parent,
                                child,
                                event: Box::new(event),
                            });
                        }
                    }
                    // Un hijo muy verboso puede adelantar el búfer: se pierden eventos, no el reenvío.
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        // La tarea nace `submitted` al reconocerla; el hijo empieza cuando queda guardada.
        let descriptor = SubagentDescriptor {
            parent,
            child,
            job_id: job.id().0.clone(),
            label: descriptor_label,
            status: TaskStatus::new(TaskState::Submitted),
            artifacts: Vec::new(),
            notice_delivered: false,
            agent: agente,
            agent_origin: origen,
            reference_task_ids: Vec::new(),
        };
        if let Err(error) = self.store.save_subagent(&descriptor).await {
            let _ = self.jobs.kill(parent, job.id());
            return Err(error);
        }
        let mut updates = self.jobs.subscribe(parent, job.id())?;
        self.bus.publish(LiveEvent::SubagentChanged(descriptor));
        let _ = abrir_puerta.send(());
        let store = self.store.clone();
        let settle_bus = self.bus.clone();
        tokio::spawn(async move {
            loop {
                let snapshot = updates.borrow_and_update().clone();
                let state = TaskState::from(snapshot.status);
                if state.is_terminal() {
                    // `complete_subagent` ya fijó `Completed` junto con el resultado: aquí solo se
                    // escribe lo que el almacén aún no tiene asentado (muerte, fallo o cierre sin resultado).
                    let asentado = match store.list_subagents(parent).await {
                        Ok(hijos) => hijos
                            .iter()
                            .any(|hijo| hijo.child == child && hijo.status.state.is_terminal()),
                        Err(error) => {
                            tracing::warn!("read subagent status: {error}");
                            false
                        }
                    };
                    if !asentado {
                        if let Err(error) = tracker
                            .transition(parent, child, TaskStatus::new(state))
                            .await
                        {
                            tracing::warn!("persist subagent status: {error}");
                        }
                    }
                    // Se publica tras el intento de persistir, también si falló: así el
                    // padre nunca lee el aviso antes del estado final.
                    settle_bus.publish(LiveEvent::SubagentSettled {
                        parent,
                        child,
                        status: state,
                    });
                    break;
                }
                if updates.changed().await.is_err() {
                    break;
                }
            }
        });
        Ok((child, job.id().clone()))
    }

    async fn control(
        &self,
        parent: SessionId,
        child: SessionId,
        action: &str,
        message: Option<String>,
    ) -> Result<()> {
        if action == "interrupt" {
            return self.cancel(parent, child).await.map(|_| ());
        }
        let descriptor = self
            .store
            .list_subagents(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .ok_or_else(|| CoreError::Tool("unknown subagent".into()))?;
        let job = JobId(descriptor.job_id);
        if descriptor.status.state.is_terminal() {
            return Err(CoreError::Tool("subagent has already finished".into()));
        }
        let snapshot = self
            .jobs
            .list(parent)
            .into_iter()
            .find(|snapshot| snapshot.id == job)
            .ok_or_else(|| CoreError::Tool("unknown subagent".into()))?;
        if !matches!(
            snapshot.status,
            crate::job::JobStatus::Running | crate::job::JobStatus::Stopping
        ) {
            return Err(CoreError::Tool("subagent has already finished".into()));
        }
        match action {
            "send_message" => {
                let message = message
                    .filter(|text| !text.trim().is_empty() && text.len() <= 32_768)
                    .ok_or_else(|| CoreError::Tool("message must be 1 to 32768 bytes".into()))?;
                self.store
                    .append_message(child, &crate::message::Message::user(message))
                    .await
            }
            _ => Err(CoreError::Tool(
                "action must be send_message or interrupt".into(),
            )),
        }
    }

    async fn cancel(&self, parent: SessionId, child: SessionId) -> Result<SubagentDescriptor> {
        let descriptor = self
            .store
            .list_subagents(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .ok_or(CoreError::TaskNotFound(child.0))?;
        if descriptor.status.state.is_terminal() {
            return Err(CoreError::TaskNotCancelable {
                child: child.0,
                state: descriptor.status.state,
            });
        }
        // Solo se ordena parar a un trabajo en marcha: si ya se está deteniendo, la orden se dio
        // antes y repetirla no cambia nada (idempotente mientras no se asiente).
        let job = JobId(descriptor.job_id.clone());
        let en_marcha = self.jobs.list(parent).into_iter().any(|snapshot| {
            snapshot.id == job && snapshot.status == crate::job::JobStatus::Running
        });
        if en_marcha {
            self.jobs.kill(parent, &job)?;
        }
        let asentado = async {
            loop {
                let actual = self.descriptor_of(parent, child).await?;
                if actual.status.state.is_terminal() {
                    return Ok::<_, CoreError>(actual);
                }
                tokio::time::sleep(CANCEL_POLL).await;
            }
        };
        match tokio::time::timeout(self.cancel_wait, asentado).await {
            Ok(resultado) => resultado,
            // No se asentó a tiempo: se devuelve lo último que consta, que puede no ser `Canceled`.
            Err(_) => self.descriptor_of(parent, child).await,
        }
    }

    async fn list(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
        self.store.list_subagents(parent).await
    }

    async fn deliver_notices(&self, parent: SessionId) -> Result<Vec<crate::message::Message>> {
        let mut messages = Vec::new();
        for descriptor in self.store.list_subagents(parent).await? {
            if descriptor.status.state != TaskState::Completed || descriptor.notice_delivered {
                continue;
            }
            if let Some(result) = descriptor.result_text() {
                let message = crate::message::Message::user(format!(
                    "[Subagent {} completed]\n{}",
                    descriptor.label,
                    crate::tools::limit_bytes(&result, crate::tools::MAX_NOTICE_BYTES)
                ));
                if self
                    .store
                    .append_subagent_notice(parent, descriptor.child, &message)
                    .await?
                {
                    messages.push(message);
                }
            }
        }
        Ok(messages)
    }

    async fn answer(
        &self,
        parent: SessionId,
        child: SessionId,
        answers: Vec<String>,
    ) -> Result<()> {
        self.broker.answer(parent, child, answers)
    }

    async fn pending_input_notices(
        &self,
        parent: SessionId,
    ) -> Result<Vec<crate::message::Message>> {
        Ok(self
            .broker
            .take_unannounced(parent)
            .into_iter()
            .map(|input| {
                let mut lines = vec![format!("[Subagent {} needs input]", input.label)];
                for (index, question) in input.questions.iter().enumerate() {
                    let options: Vec<&str> = question
                        .options
                        .iter()
                        .map(|option| option.label.as_str())
                        .collect();
                    lines.push(format!("{}. {}", index + 1, question.question));
                    lines.push(format!("   options: {}", options.join(" | ")));
                }
                lines.push(format!(
                    "Reply with subagent_control action=answer child_id={} answers=[<one string per question, in order>]",
                    input.child.0
                ));
                crate::message::Message::user(lines.join("
"))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ProviderConfig, ProviderKind};
    use crate::job::{JobRegistry, JobStatus, LocalJobRegistry};
    use crate::message::FinishReason;
    use crate::permission::PermissionService;
    use crate::provider::testutil::ScriptedProvider;
    use crate::provider::StreamChunk;
    use crate::registry::{
        AgentOrigin, AgentSource, BuiltInSource, FileSource, ProfileLevel, SourceLoad, TrustStore,
    };
    use crate::session::sqlite::SqliteStore;
    use crate::task::TaskState;
    use crate::tools::subagent::SubagentTool;
    use crate::tools::MAX_NOTICE_BYTES;
    use std::time::Duration;

    #[tokio::test]
    async fn runs_a_child_and_queues_its_parent_notice_without_recursive_tool() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("subagents.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            script: vec![
                StreamChunk::TextDelta("child result".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(SubagentTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
                effort: None,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
            EventBus::new(64),
        );

        assert!(runtime.tools.get("subagent").is_none());
        assert!(runtime.tools.get("subagent_control").is_none());
        assert!(runtime.tools.get("subagent_list").is_none());
        let (child, job) = runtime
            .start(
                parent,
                "research".into(),
                "do work".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(runtime
            .control(SessionId(999), child, "interrupt", None)
            .await
            .is_err());
        let outcome = jobs.wait(parent, &job).await.unwrap();

        assert_eq!(outcome.status, JobStatus::Completed);
        assert!(store.messages(child).await.unwrap().iter().any(|message| {
            message.role == crate::message::Role::Assistant
                && message.text().contains("child result")
        }));
        assert!(runtime
            .deliver_notices(parent)
            .await
            .unwrap()
            .iter()
            .any(|message| message.text().contains("child result")));
        assert!(runtime.deliver_notices(parent).await.unwrap().is_empty());
        assert!(runtime
            .control(parent, child, "interrupt", None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn parent_notice_text_is_unchanged_after_the_artifact_change() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("aviso.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            script: vec![
                StreamChunk::TextDelta("child result".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(SubagentTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
                effort: None,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
            EventBus::new(64),
        );
        let (_child, job) = runtime
            .start(
                parent,
                "research".into(),
                "do work".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let avisos = runtime.deliver_notices(parent).await.unwrap();
        assert_eq!(avisos.len(), 1);
        // Texto exacto del aviso al padre: lo que había antes del cambio a artefactos.
        assert_eq!(
            avisos[0].text(),
            "[Subagent research completed]\nchild result"
        );
    }

    #[tokio::test]
    async fn subagent_notice_result_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("tope.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            script: vec![
                StreamChunk::TextDelta("y".repeat(50_000)),
                StreamChunk::Done(FinishReason::Stop),
            ],
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(SubagentTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
                effort: None,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
            EventBus::new(64),
        );
        let (_child, job) = runtime
            .start(
                parent,
                "research".into(),
                "do work".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let avisos = runtime.deliver_notices(parent).await.unwrap();
        assert_eq!(avisos.len(), 1);
        let texto = avisos[0].text();
        assert!(texto.starts_with("[Subagent research completed]\n"));
        // Tope de 8 000 bytes de resultado más el aviso de recorte; lejos de los 50 000 originales.
        assert!(
            texto.len() < MAX_NOTICE_BYTES + 200,
            "medía {}",
            texto.len()
        );
        // ADAPTADO: el aviso se recorta ahora con cabeza y cola y el marcador «bytes omitted».
        assert!(texto.contains("bytes omitted ...]"));
    }

    #[tokio::test]
    async fn settled_event_is_published_after_the_status_is_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("settled.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider {
            script: vec![
                StreamChunk::TextDelta("child result".into()),
                StreamChunk::Done(FinishReason::Stop),
            ],
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(SubagentTool));
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
                effort: None,
            })),
            dir.path().to_path_buf(),
            Arc::new(LocalJobRegistry::default()),
            bus,
        );

        let (child, _job) = runtime
            .start(
                parent,
                "research".into(),
                "do work".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let status = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Ok(crate::event::LiveEvent::SubagentSettled {
                        child: settled,
                        status,
                        ..
                    }) if settled == child => return status,
                    Ok(_) => {}
                    Err(error) => panic!("el bus debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el hijo debería asentarse");

        assert_eq!(status, TaskState::Completed);
        // Al llegar el evento, el estado final ya debe estar persistido y el aviso listo.
        assert!(store
            .list_subagents(parent)
            .await
            .unwrap()
            .iter()
            .any(|d| d.child == child && d.status.state == TaskState::Completed));
        assert!(runtime
            .deliver_notices(parent)
            .await
            .unwrap()
            .iter()
            .any(|message| message.text().contains("child result")));
    }

    #[tokio::test]
    async fn child_keeps_ask_user_tool() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("subagents.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider { script: vec![] });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        assert!(tools.get("ask_user").is_some());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store,
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 2,
                effort: None,
            })),
            dir.path().to_path_buf(),
            Arc::new(LocalJobRegistry::default()),
            EventBus::new(64),
        );
        assert!(runtime.tools.get("ask_user").is_some());
    }

    /// Proveedor de prueba con un guion por cada paso de la conversación.
    struct QueueProvider {
        scripts: std::sync::Mutex<std::collections::VecDeque<Vec<StreamChunk>>>,
    }

    #[async_trait]
    impl Provider for QueueProvider {
        fn capabilities(&self) -> crate::provider::Capabilities {
            crate::provider::Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            _req: crate::provider::ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<crate::provider::ProviderStream> {
            let script = self.scripts.lock().unwrap().pop_front().unwrap_or_default();
            Ok(Box::pin(futures::stream::iter(script)))
        }
    }

    #[tokio::test]
    async fn interrupt_discards_the_pending_question() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("interrupt.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(QueueProvider {
            scripts: std::sync::Mutex::new(
                vec![vec![
                    StreamChunk::ToolCallComplete {
                        id: "c1".into(),
                        name: "ask_user".into(),
                        input: serde_json::json!({"questions": [{
                            "question": "¿Cuál?",
                            "options": [{"label": "Sí"}, {"label": "No"}]
                        }]}),
                    },
                    StreamChunk::Done(FinishReason::ToolCalls),
                ]]
                .into(),
            ),
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 4,
                effort: None,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
            EventBus::new(64),
        );

        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");

        runtime
            .control(parent, child, "interrupt", None)
            .await
            .unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(5), jobs.wait(parent, &job))
            .await
            .expect("el hijo interrumpido debería terminar")
            .unwrap();

        assert_eq!(outcome.status, JobStatus::Killed);
        assert!(runtime.pending_inputs(parent).is_empty());
        assert!(runtime
            .answer(parent, child, vec!["Sí".into()])
            .await
            .is_err());
    }

    #[tokio::test]
    async fn killed_job_becomes_canceled() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("killed.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(QueueProvider {
            scripts: std::sync::Mutex::new(
                vec![vec![
                    StreamChunk::ToolCallComplete {
                        id: "c1".into(),
                        name: "ask_user".into(),
                        input: serde_json::json!({"questions": [{
                            "question": "¿Cuál?",
                            "options": [{"label": "Sí"}, {"label": "No"}]
                        }]}),
                    },
                    StreamChunk::Done(FinishReason::ToolCalls),
                ]]
                .into(),
            ),
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 4,
                effort: None,
            })),
            dir.path().to_path_buf(),
            Arc::new(LocalJobRegistry::default()),
            bus,
        );

        let (child, _job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");

        runtime
            .control(parent, child, "interrupt", None)
            .await
            .unwrap();
        let settled = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Ok(crate::event::LiveEvent::SubagentSettled {
                        child: settled,
                        status,
                        ..
                    }) if settled == child => return status,
                    Ok(_) => {}
                    Err(error) => panic!("el bus debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el hijo matado debería asentarse");

        assert_eq!(settled, TaskState::Canceled);
        let listed = store.list_subagents(parent).await.unwrap();
        assert_eq!(listed[0].status.state, TaskState::Canceled);
    }

    #[tokio::test]
    async fn child_that_asks_blocks_until_the_parent_answers() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("ask.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(QueueProvider {
            scripts: std::sync::Mutex::new(
                vec![
                    vec![
                        StreamChunk::ToolCallComplete {
                            id: "c1".into(),
                            name: "ask_user".into(),
                            input: serde_json::json!({"questions": [{
                                "question": "¿Cuál?",
                                "options": [{"label": "Sí"}, {"label": "No"}]
                            }]}),
                        },
                        StreamChunk::Done(FinishReason::ToolCalls),
                    ],
                    vec![
                        StreamChunk::TextDelta("hecho".into()),
                        StreamChunk::Done(FinishReason::Stop),
                    ],
                ]
                .into(),
            ),
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 4,
                effort: None,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
            EventBus::new(64),
        );

        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");
        assert_eq!(runtime.pending_inputs(parent)[0].child, child);

        runtime
            .answer(parent, child, vec!["Sí".into()])
            .await
            .unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(5), jobs.wait(parent, &job))
            .await
            .expect("el hijo debería terminar tras la respuesta")
            .unwrap();

        assert_eq!(outcome.status, JobStatus::Completed);
        assert!(runtime.pending_inputs(parent).is_empty());
        let messages = store.messages(child).await.unwrap();
        assert!(messages
            .iter()
            .flat_map(|message| message.parts.iter())
            .any(|part| matches!(
                part,
                crate::message::ContentPart::ToolResult { content, .. } if content.contains("Sí")
            )));
    }

    #[tokio::test]
    async fn pending_input_notices_are_delivered_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("notices.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(QueueProvider {
            scripts: std::sync::Mutex::new(
                vec![vec![
                    StreamChunk::ToolCallComplete {
                        id: "c1".into(),
                        name: "ask_user".into(),
                        input: serde_json::json!({"questions": [{
                            "question": "¿Cuál?",
                            "options": [{"label": "Sí"}, {"label": "No"}]
                        }]}),
                    },
                    StreamChunk::Done(FinishReason::ToolCalls),
                ]]
                .into(),
            ),
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 4,
                effort: None,
            })),
            dir.path().to_path_buf(),
            jobs.clone(),
            EventBus::new(64),
        );

        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");

        let first = runtime.pending_input_notices(parent).await.unwrap();
        assert_eq!(first.len(), 1);
        let text = first[0].text();
        assert!(text.starts_with("[Subagent obrero needs input]"), "{text}");
        assert!(text.contains("¿Cuál?"), "{text}");
        assert!(text.contains("Sí") && text.contains("No"), "{text}");
        assert!(
            text.contains(&format!(
                "subagent_control action=answer child_id={}",
                child.0
            )),
            "{text}"
        );
        assert!(runtime
            .pending_input_notices(parent)
            .await
            .unwrap()
            .is_empty());

        runtime
            .answer(parent, child, vec!["Sí".into()])
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), jobs.wait(parent, &job))
            .await
            .expect("el hijo debería terminar tras la respuesta")
            .unwrap();
    }

    /// Herramienta trivial que devuelve su entrada; sirve para observar sus eventos desde el padre.
    struct EchoTool;

    #[async_trait]
    impl crate::tools::Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "devuelve su entrada"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn run(
            &self,
            _ctx: crate::tools::ToolCtx,
            input: serde_json::Value,
            _cancel: CancellationToken,
        ) -> Result<crate::tools::ToolOutput> {
            Ok(crate::tools::ToolOutput::ok(input.to_string()))
        }
    }

    /// Lanzador con la herramienta `echo` y un guion de pasos para el hijo.
    fn echo_launcher(
        dir: &std::path::Path,
        store: Arc<SqliteStore>,
        jobs: Arc<LocalJobRegistry>,
        bus: EventBus,
        script: Vec<Vec<StreamChunk>>,
    ) -> InProcessSubagentLauncher {
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(EchoTool));
        let provider: Arc<dyn Provider> = Arc::new(QueueProvider {
            scripts: std::sync::Mutex::new(script.into()),
        });
        InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store,
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 4,
                effort: None,
            })),
            dir.to_path_buf(),
            jobs,
            bus,
        )
    }

    #[tokio::test]
    async fn child_events_are_forwarded_tagged_with_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("forward.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let bus = EventBus::new(256);
        let mut rx = bus.subscribe();
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = echo_launcher(
            dir.path(),
            store.clone(),
            jobs.clone(),
            bus.clone(),
            vec![
                vec![
                    StreamChunk::ToolCallComplete {
                        id: "c1".into(),
                        name: "echo".into(),
                        input: serde_json::json!({"texto": "hola"}),
                    },
                    StreamChunk::Done(FinishReason::ToolCalls),
                ],
                vec![
                    StreamChunk::TextDelta("listo".into()),
                    StreamChunk::Done(FinishReason::Stop),
                ],
            ],
        );

        let (child, _job) = runtime
            .start(
                parent,
                "obrero".into(),
                "usa echo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        // Se recogen los reenvíos hasta el TurnEnded del hijo (acotado en tiempo).
        let forwarded = tokio::time::timeout(Duration::from_secs(5), async {
            let mut seen = Vec::new();
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::ChildEvent {
                        parent: p,
                        child: c,
                        event,
                    }) => {
                        let finished = c == child && matches!(*event, LiveEvent::TurnEnded);
                        seen.push((p, c, *event));
                        if finished {
                            return seen;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => panic!("el bus del padre debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el hijo debería reenviar su TurnEnded");

        assert!(
            forwarded
                .iter()
                .all(|(p, c, _)| *p == parent && *c == child),
            "{forwarded:?}"
        );
        assert!(
            !forwarded
                .iter()
                .any(|(_, _, e)| matches!(e, LiveEvent::Delta { .. })),
            "{forwarded:?}"
        );
        let started = forwarded.iter().position(
            |(_, _, e)| matches!(e, LiveEvent::ToolStarted { name, .. } if name.as_str() == "echo"),
        );
        let finished = forwarded.iter().position(|(_, _, e)| {
            matches!(
                e,
                LiveEvent::ToolFinished {
                    is_error: false,
                    ..
                }
            )
        });
        let ended = forwarded
            .iter()
            .position(|(_, _, e)| matches!(e, LiveEvent::TurnEnded));
        let (Some(started), Some(finished), Some(ended)) = (started, finished, ended) else {
            panic!("faltan eventos reenviados: {forwarded:?}");
        };
        assert!(started < finished && finished < ended, "{forwarded:?}");
    }

    #[tokio::test]
    async fn child_messages_are_forwarded_tagged_with_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("messages.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let bus = EventBus::new(256);
        let mut rx = bus.subscribe();
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = echo_launcher(
            dir.path(),
            store.clone(),
            jobs.clone(),
            bus.clone(),
            vec![vec![
                StreamChunk::TextDelta("listo".into()),
                StreamChunk::Done(FinishReason::Stop),
            ]],
        );

        let (child, _job) = runtime
            .start(
                parent,
                "obrero".into(),
                "responde".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        // Se recogen los reenvíos hasta el TurnEnded del hijo (acotado en tiempo).
        let forwarded = tokio::time::timeout(Duration::from_secs(5), async {
            let mut seen = Vec::new();
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::ChildEvent {
                        parent: p,
                        child: c,
                        event,
                    }) => {
                        let finished = c == child && matches!(*event, LiveEvent::TurnEnded);
                        seen.push((p, c, *event));
                        if finished {
                            return seen;
                        }
                    }
                    Ok(_) => {}
                    Err(error) => panic!("el bus del padre debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el hijo debería reenviar su TurnEnded");

        let message = forwarded.iter().find_map(|(p, c, e)| match e {
            LiveEvent::MessageAdded(_) => Some((*p, *c)),
            _ => None,
        });
        assert_eq!(
            message,
            Some((parent, child)),
            "el mensaje del hijo debería llegar al padre etiquetado con su identidad: {forwarded:?}"
        );
    }

    #[tokio::test]
    async fn forwarder_ends_when_the_child_finishes() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("ends.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let bus = EventBus::new(256);
        let mut rx = bus.subscribe();
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = echo_launcher(
            dir.path(),
            store.clone(),
            jobs.clone(),
            bus.clone(),
            vec![vec![
                StreamChunk::TextDelta("listo".into()),
                StreamChunk::Done(FinishReason::Stop),
            ]],
        );

        let metrics = tokio::runtime::Handle::current().metrics();
        let baseline = metrics.num_alive_tasks();
        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "termina".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let outcome = tokio::time::timeout(Duration::from_secs(5), jobs.wait(parent, &job))
            .await
            .expect("el hijo debería terminar")
            .unwrap();
        assert_eq!(outcome.status, JobStatus::Completed);

        // Al terminar el hijo, el reenvío debe acabar y no quedar tareas huérfanas.
        tokio::time::timeout(Duration::from_secs(5), async {
            while metrics.num_alive_tasks() > baseline {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el reenvío debería terminar con el hijo");

        // Todo lo reenviado ya está en el bus; tras el TurnEnded no llega nada más del hijo.
        let mut child_events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let LiveEvent::ChildEvent {
                child: c, event, ..
            } = event
            {
                if c == child {
                    child_events.push(*event);
                }
            }
        }
        assert!(
            matches!(child_events.last(), Some(LiveEvent::TurnEnded)),
            "{child_events:?}"
        );
        assert_eq!(
            child_events
                .iter()
                .filter(|e| matches!(e, LiveEvent::TurnEnded))
                .count(),
            1,
            "{child_events:?}"
        );
    }

    /// Lanzador cuyo hijo pregunta al padre y se queda esperando `input-required`.
    fn lanzador_que_pregunta(
        dir: &std::path::Path,
        store: Arc<SqliteStore>,
        jobs: Arc<LocalJobRegistry>,
        bus: EventBus,
    ) -> InProcessSubagentLauncher {
        let provider: Arc<dyn Provider> = Arc::new(QueueProvider {
            scripts: std::sync::Mutex::new(
                vec![vec![
                    StreamChunk::ToolCallComplete {
                        id: "c1".into(),
                        name: "ask_user".into(),
                        input: serde_json::json!({"questions": [{
                            "question": "¿Cuál?",
                            "options": [{"label": "Sí"}, {"label": "No"}]
                        }]}),
                    },
                    StreamChunk::Done(FinishReason::ToolCalls),
                ]]
                .into(),
            ),
        });
        let mut tools = ToolRegistry::new();
        tools.register(Arc::new(crate::tools::ask_user::AskUserTool));
        InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store,
            Arc::new(tools),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(CodeLoomConfig {
                provider: ProviderConfig {
                    kind: ProviderKind::OpenAiCompat,
                    base_url: "http://localhost".into(),
                    api_key_env: "KEY".into(),
                },
                model: "test".into(),
                max_steps: 4,
                effort: None,
            })),
            dir.to_path_buf(),
            jobs,
            bus,
        )
    }

    /// Espera, con cota de tiempo, a que el almacén registre el estado pedido para el hijo.
    async fn espera_estado_del_hijo(
        store: &SqliteStore,
        parent: SessionId,
        child: SessionId,
        estado: TaskState,
    ) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let actual = store
                    .list_subagents(parent)
                    .await
                    .unwrap()
                    .into_iter()
                    .find(|d| d.child == child)
                    .map(|d| d.status.state);
                if actual == Some(estado) {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("el hijo debería quedar en {estado:?}"));
    }

    #[tokio::test]
    async fn launcher_saves_the_child_as_submitted_then_working() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("submitted.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let bus = EventBus::new(256);
        let mut rx = bus.subscribe();
        let runtime = echo_launcher(
            dir.path(),
            store.clone(),
            Arc::new(LocalJobRegistry::default()),
            bus,
            vec![vec![
                StreamChunk::TextDelta("listo".into()),
                StreamChunk::Done(FinishReason::Stop),
            ]],
        );

        let (child, _job) = runtime
            .start(
                parent,
                "obrero".into(),
                "termina".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let estados = tokio::time::timeout(Duration::from_secs(5), async {
            let mut visto = Vec::new();
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::SubagentChanged(descriptor)) if descriptor.child == child => {
                        visto.push(descriptor.status.state);
                    }
                    Ok(LiveEvent::SubagentSettled { child: settled, .. }) if settled == child => {
                        return visto;
                    }
                    Ok(_) => {}
                    Err(error) => panic!("el bus debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el hijo debería asentarse");

        assert_eq!(
            estados,
            vec![
                TaskState::Submitted,
                TaskState::Working,
                TaskState::Completed
            ],
            "cada transición se publica en orden"
        );
    }

    #[tokio::test]
    async fn launcher_rejects_when_capacity_is_exhausted_and_records_a_rejected_task() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("rejected.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let bus = EventBus::new(64);
        let mut rx = bus.subscribe();
        let runtime = echo_launcher(
            dir.path(),
            store.clone(),
            Arc::new(LocalJobRegistry::default()),
            bus,
            vec![],
        );
        // Se ocupan los ocho puestos a mano: el noveno hijo debe rechazarse.
        let _ocupados: Vec<_> = (0..8)
            .map(|_| runtime.active.clone().try_acquire_owned().unwrap())
            .collect();

        let error = runtime
            .start(
                parent,
                "noveno".into(),
                "hazlo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("maximum of 8 active subagents reached"),
            "{error}"
        );

        let hijos = store.list_subagents(parent).await.unwrap();
        assert_eq!(hijos.len(), 1, "queda una tarea rechazada");
        let hijo = &hijos[0];
        assert_eq!(hijo.status.state, TaskState::Rejected);
        assert_eq!(
            hijo.status.message_text().as_deref(),
            Some("maximum of 8 active subagents reached")
        );
        assert!(hijo.job_id.is_empty(), "un rechazado no tiene trabajo");

        let publicado = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::SubagentChanged(descriptor))
                        if descriptor.child == hijo.child =>
                    {
                        return descriptor.status.state;
                    }
                    Ok(_) => {}
                    Err(error) => panic!("el bus debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el rechazo debería publicarse");
        assert_eq!(publicado, TaskState::Rejected);
    }

    #[tokio::test]
    async fn launcher_validation_errors_create_no_task() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("validation.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let runtime = echo_launcher(
            dir.path(),
            store.clone(),
            Arc::new(LocalJobRegistry::default()),
            EventBus::new(16),
            vec![],
        );

        let casos = [
            ("obrero".to_string(), "   ".to_string()),
            ("obrero".to_string(), "x".repeat(32_769)),
            ("x".repeat(129), "hola".to_string()),
        ];
        for (etiqueta, prompt) in casos {
            assert!(
                runtime
                    .start(parent, etiqueta, prompt, None, CancellationToken::new())
                    .await
                    .is_err(),
                "debería rechazarse la entrada inválida"
            );
        }
        assert!(
            store.list_subagents(parent).await.unwrap().is_empty(),
            "la validación no crea tarea"
        );
    }

    #[tokio::test]
    async fn interrupt_moves_the_task_to_canceled() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("interrupt_canceled.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let bus = EventBus::new(256);
        let mut rx = bus.subscribe();
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime = lanzador_que_pregunta(dir.path(), store.clone(), jobs.clone(), bus);
        let (child, _job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");

        runtime
            .control(parent, child, "interrupt", None)
            .await
            .unwrap();
        espera_estado_del_hijo(&store, parent, child, TaskState::Canceled).await;
        let publicado = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::SubagentChanged(descriptor))
                        if descriptor.child == child
                            && descriptor.status.state == TaskState::Canceled =>
                    {
                        return true;
                    }
                    Ok(_) => {}
                    Err(error) => panic!("el bus debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("la cancelación debería publicarse");
        assert!(publicado);
    }

    #[tokio::test]
    async fn parent_cancel_moves_the_child_to_canceled() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("parent_cancel.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime =
            lanzador_que_pregunta(dir.path(), store.clone(), jobs.clone(), EventBus::new(64));
        let padre = CancellationToken::new();
        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                padre.clone(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");

        // Esc del padre: cancela su turno, y con él al hijo, que no puede terminar en `completed`.
        padre.cancel();
        let outcome = tokio::time::timeout(Duration::from_secs(5), jobs.wait(parent, &job))
            .await
            .expect("el hijo debería terminar al cancelar al padre")
            .unwrap();
        assert_eq!(outcome.status, JobStatus::Killed);
        espera_estado_del_hijo(&store, parent, child, TaskState::Canceled).await;
        assert!(runtime.pending_inputs(parent).is_empty());
    }

    /// Proveedor que graba cada petición (modelo, sistema y esfuerzo). Si tiene puerta, la
    /// respuesta espera a que se abra, para poder cambiar el registro con el hijo en curso.
    struct ProveedorGrabador {
        peticiones: Arc<std::sync::Mutex<Vec<(String, String, Option<String>)>>>,
        puerta: Option<Arc<tokio::sync::Notify>>,
    }

    #[async_trait]
    impl Provider for ProveedorGrabador {
        fn capabilities(&self) -> crate::provider::Capabilities {
            crate::provider::Capabilities {
                tool_use: true,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            req: crate::provider::ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<crate::provider::ProviderStream> {
            self.peticiones.lock().unwrap().push((
                req.model.clone(),
                req.system.clone(),
                req.effort.clone(),
            ));
            if let Some(puerta) = &self.puerta {
                puerta.notified().await;
            }
            Ok(Box::pin(futures::stream::iter(vec![
                StreamChunk::TextDelta("listo".into()),
                StreamChunk::Done(FinishReason::Stop),
            ])))
        }
    }

    /// Proveedor que responde `listo` declarando `entrada` tokens de consumo.
    struct ProveedorConUso {
        entrada: u32,
    }

    #[async_trait]
    impl Provider for ProveedorConUso {
        fn capabilities(&self) -> crate::provider::Capabilities {
            crate::provider::Capabilities {
                tool_use: false,
                reasoning: false,
                streaming: true,
            }
        }
        async fn stream(
            &self,
            _req: crate::provider::ProviderRequest,
            _cancel: CancellationToken,
        ) -> Result<crate::provider::ProviderStream> {
            Ok(Box::pin(futures::stream::iter(vec![
                StreamChunk::TextDelta("listo".into()),
                StreamChunk::Usage(crate::message::Usage {
                    input_tokens: self.entrada,
                    output_tokens: 0,
                }),
                StreamChunk::Done(FinishReason::Stop),
            ])))
        }
    }

    #[tokio::test]
    async fn a_child_inherits_the_budget_and_its_warning_reaches_the_parent_bus() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "presupuesto.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let bus = EventBus::new(256);
        let mut rx = bus.subscribe();
        let provider: Arc<dyn Provider> = Arc::new(ProveedorConUso { entrada: 900 });
        let runtime = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(ToolRegistry::new()),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(config_de_prueba())),
            dir.path().to_path_buf(),
            jobs.clone(),
            bus,
        )
        .with_budget(BudgetConfig {
            session_tokens: Some(1000),
            session_cost_usd: None,
        });

        let (_, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "tarea".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let aviso = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match rx.recv().await {
                    Ok(LiveEvent::ChildEvent { event, .. }) => {
                        if let LiveEvent::BudgetWarning {
                            level_percent,
                            used,
                            ..
                        } = *event
                        {
                            return (level_percent, used);
                        }
                    }
                    Ok(_) => {}
                    Err(error) => panic!("el bus debería seguir abierto: {error}"),
                }
            }
        })
        .await
        .expect("el aviso del hijo debería llegar al bus del padre");
        assert_eq!(aviso, (80, 900.0));
    }

    /// Configuración del padre de las pruebas: modelo `test` y esfuerzo `low`.
    fn config_de_prueba() -> CodeLoomConfig {
        CodeLoomConfig {
            provider: ProviderConfig {
                kind: ProviderKind::OpenAiCompat,
                base_url: "http://localhost".into(),
                api_key_env: "KEY".into(),
            },
            model: "test".into(),
            max_steps: 4,
            effort: Some("low".into()),
        }
    }

    /// Lanzador de prueba con el proveedor grabador y, si se pasan, un registro y un catálogo.
    fn lanzador_grabador(
        dir: &std::path::Path,
        store: Arc<SqliteStore>,
        jobs: Arc<LocalJobRegistry>,
        proveedor: ProveedorGrabador,
        registro: Option<Arc<AgentRegistry>>,
        catalogo: Option<Catalog>,
    ) -> InProcessSubagentLauncher {
        let provider: Arc<dyn Provider> = Arc::new(proveedor);
        let lanzador = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store,
            Arc::new(ToolRegistry::new()),
            Arc::new(PermissionService::allow_all()),
            Arc::new(RwLock::new(config_de_prueba())),
            dir.to_path_buf(),
            jobs,
            EventBus::new(64),
        );
        let lanzador = match catalogo {
            Some(catalogo) => lanzador.with_catalog(Arc::new(RwLock::new(catalogo))),
            None => lanzador,
        };
        match registro {
            Some(registro) => lanzador.with_registry(registro),
            None => lanzador,
        }
    }

    async fn tienda_de_prueba(
        dir: &std::path::Path,
        nombre: &str,
    ) -> (Arc<SqliteStore>, SessionId) {
        let store = Arc::new(
            SqliteStore::open(dir.join(nombre).to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        (store, parent)
    }

    /// Registro con el integrado y una carpeta global de perfiles.
    fn registro_global(global: &std::path::Path) -> Arc<AgentRegistry> {
        Arc::new(AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(global.to_path_buf(), ProfileLevel::Global)),
        ]))
    }

    fn escribir_perfil(dir: &std::path::Path, nombre: &str, texto: &str) {
        std::fs::write(dir.join(format!("{nombre}.md")), texto).unwrap();
    }

    const PERFIL_CON_PERSONA: &str = "---\nname: frontend\ndescription: UI\npersona:\n  role: Ingeniero\nmodel: modelo-perfil\neffort: high\n---\n\nCuerpo del agente.\n";

    /// Fuente de prueba con un agente remoto. Nada lo produce todavía (A8).
    struct FuenteRemotaDePrueba;

    impl AgentSource for FuenteRemotaDePrueba {
        fn id(&self) -> &str {
            "remota-de-prueba"
        }
        fn load(&self) -> SourceLoad {
            SourceLoad {
                entries: vec![AgentEntry {
                    name: "remoto".into(),
                    origin: AgentOrigin::Remote {
                        card_url: "https://agente.example/card".into(),
                    },
                    card: crate::card::AgentCard::local(
                        "remoto",
                        "Agente remoto de prueba",
                        "1.0.0",
                    ),
                    execution: Execution::Remote,
                    content_hash: None,
                }],
                problems: Vec::new(),
            }
        }
    }

    #[tokio::test]
    async fn cancel_returns_the_task_canceled_and_drops_its_question() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "cancel-ok.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let runtime =
            lanzador_que_pregunta(dir.path(), store.clone(), jobs.clone(), EventBus::new(64));
        let (child, _job) = runtime
            .start(
                parent,
                "obrero".into(),
                "decide algo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.pending_inputs(parent).is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería quedar esperando una respuesta");

        // Cancelable en `input-required` (D3): la tarea actualizada vuelve ya asentada.
        let tarea = runtime.cancel(parent, child).await.unwrap();

        assert_eq!(tarea.child, child);
        assert_eq!(tarea.status.state, TaskState::Canceled);
        assert!(runtime.pending_inputs(parent).is_empty());

        // Una tarea ya cancelada no vuelve a ser cancelable (D4).
        match runtime.cancel(parent, child).await {
            Err(CoreError::TaskNotCancelable { child: id, state }) => {
                assert_eq!((id, state), (child.0, TaskState::Canceled));
            }
            otro => panic!("se esperaba TaskNotCancelable, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_of_a_completed_child_is_not_cancelable() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "cancel-completed.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let runtime = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            None,
            None,
        );
        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "tarea".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();
        espera_estado_del_hijo(&store, parent, child, TaskState::Completed).await;

        match runtime.cancel(parent, child).await {
            Err(CoreError::TaskNotCancelable { state, .. }) => {
                assert_eq!(state, TaskState::Completed);
            }
            otro => panic!("se esperaba TaskNotCancelable, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_of_a_child_of_another_parent_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "cancel-ajeno.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let runtime = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            None,
            None,
        );
        let (child, job) = runtime
            .start(
                parent,
                "obrero".into(),
                "tarea".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();
        let otro_padre = store.create_session("/project").await.unwrap();

        match runtime.cancel(otro_padre, child).await {
            Err(CoreError::TaskNotFound(id)) => assert_eq!(id, child.0),
            otro => panic!("se esperaba TaskNotFound, y salió {otro:?}"),
        }
        match runtime.cancel(parent, SessionId(9999)).await {
            Err(CoreError::TaskNotFound(id)) => assert_eq!(id, 9999),
            otro => panic!("se esperaba TaskNotFound, y salió {otro:?}"),
        }
    }

    /// Hijo cuyo trabajo ignora la orden de parar: nunca termina. Sirve para ver qué hace `cancel`
    /// cuando el éxito no llega.
    async fn hijo_que_no_se_detiene(
        store: &Arc<SqliteStore>,
        jobs: &Arc<LocalJobRegistry>,
        parent: SessionId,
    ) -> SessionId {
        let child = store.create_session("/project").await.unwrap();
        let job = jobs
            .start(JobSpec {
                kind: "subagent".into(),
                label: "terco".into(),
                owner: parent,
                output_limit: 1024,
                run: Box::new(|_handle, _cancel| {
                    Box::pin(async move {
                        std::future::pending::<()>().await;
                        Ok(None)
                    })
                }),
            })
            .unwrap();
        store
            .save_subagent(&SubagentDescriptor {
                parent,
                child,
                job_id: job.id().0.clone(),
                label: "terco".into(),
                status: TaskStatus::new(TaskState::Working),
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
    async fn cancel_does_not_claim_success_if_the_child_does_not_settle() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "cancel-terco.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let runtime = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            None,
            None,
        );
        let runtime = runtime.with_cancel_wait(Duration::from_millis(150));
        let child = hijo_que_no_se_detiene(&store, &jobs, parent).await;

        let tarea = runtime.cancel(parent, child).await.unwrap();

        assert_eq!(
            tarea.status.state,
            TaskState::Working,
            "no se asentó: no puede decirse que se canceló"
        );
        // El trabajo recibió la orden de parar (D2) pero sigue deteniéndose.
        let job = JobId(tarea.job_id.clone());
        let estado = jobs
            .list(parent)
            .into_iter()
            .find(|snapshot| snapshot.id == job)
            .unwrap()
            .status;
        assert_eq!(estado, JobStatus::Stopping);
    }

    #[tokio::test]
    async fn a_repeated_cancel_while_stopping_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "cancel-repetido.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let runtime = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            None,
            None,
        );
        let runtime = runtime.with_cancel_wait(Duration::from_millis(150));
        let child = hijo_que_no_se_detiene(&store, &jobs, parent).await;

        let primera = runtime.cancel(parent, child).await.unwrap();
        let segunda = runtime
            .cancel(parent, child)
            .await
            .expect("repetir la orden mientras se detiene no es un error (D4)");

        assert_eq!(primera.status.state, TaskState::Working);
        assert_eq!(segunda.status.state, TaskState::Working);
    }

    #[tokio::test]
    async fn start_without_registry_behaves_as_before() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "sin-registro.db").await;
        let peticiones = Arc::new(std::sync::Mutex::new(Vec::new()));
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: peticiones.clone(),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            None,
            None,
        );

        // Sin agente y con `default`, sin registro: el mismo camino de siempre.
        for agente in [None, Some(AGENTE_POR_DEFECTO.to_string())] {
            let (_child, job) = lanzador
                .start(
                    parent,
                    "obrero".into(),
                    "hazlo".into(),
                    agente,
                    CancellationToken::new(),
                )
                .await
                .unwrap();
            jobs.wait(parent, &job).await.unwrap();
        }

        let vistas = peticiones.lock().unwrap().clone();
        assert_eq!(vistas.len(), 2);
        for (modelo, sistema, esfuerzo) in vistas {
            assert_eq!(modelo, "test", "el modelo del padre");
            assert_eq!(esfuerzo.as_deref(), Some("low"), "el esfuerzo del padre");
            assert_eq!(
                sistema,
                crate::config::context::load_context_files(dir.path()).join("\n\n"),
                "el sistema es el contexto del proyecto, sin añadidos"
            );
        }
        assert!(store
            .list_subagents(parent)
            .await
            .unwrap()
            .iter()
            .all(|hijo| hijo.agent.is_none() && hijo.agent_origin.is_none()));
    }

    #[tokio::test]
    async fn start_with_a_named_agent_without_registry_is_a_validation_error() {
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "sin-registro-error.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let lanzador = lanzador_grabador(dir.path(), store.clone(), jobs, proveedor, None, None);

        let error = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("obrero".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("obrero"), "{error}");
        assert!(store.list_subagents(parent).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn start_without_agent_uses_default_and_inherits_model() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "defecto.db").await;
        let peticiones = Arc::new(std::sync::Mutex::new(Vec::new()));
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: peticiones.clone(),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            Some(registro_global(global.path())),
            None,
        );

        let (_child, job) = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                None,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let (modelo, sistema, esfuerzo) = peticiones.lock().unwrap()[0].clone();
        assert_eq!(modelo, "test");
        assert_eq!(esfuerzo.as_deref(), Some("low"));
        assert_eq!(
            sistema,
            crate::config::context::load_context_files(dir.path()).join("\n\n"),
            "`default` no aporta prompt de sistema propio"
        );
        let hijo = &store.list_subagents(parent).await.unwrap()[0];
        assert_eq!(hijo.agent.as_deref(), Some(AGENTE_POR_DEFECTO));
        assert_eq!(hijo.agent_origin.as_deref(), Some("built-in"));
    }

    #[tokio::test]
    async fn start_with_a_file_agent_applies_persona_model_and_effort() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        escribir_perfil(global.path(), "frontend", PERFIL_CON_PERSONA);
        let (store, parent) = tienda_de_prueba(dir.path(), "perfil.db").await;
        let peticiones = Arc::new(std::sync::Mutex::new(Vec::new()));
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: peticiones.clone(),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store,
            jobs.clone(),
            proveedor,
            Some(registro_global(global.path())),
            None,
        );

        let (_child, job) = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("frontend".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let (modelo, sistema, esfuerzo) = peticiones.lock().unwrap()[0].clone();
        assert_eq!(modelo, "modelo-perfil");
        assert_eq!(esfuerzo.as_deref(), Some("high"));
        assert!(sistema.contains("Rol: Ingeniero\n"), "{sistema}");
        assert!(sistema.contains("Cuerpo del agente."), "{sistema}");
    }

    #[tokio::test]
    async fn start_with_missing_model_in_profile_inherits_from_parent() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        escribir_perfil(
            global.path(),
            "revisor",
            "---\nname: revisor\ndescription: Revisa\n---\n\nRevisa con cuidado.\n",
        );
        let (store, parent) = tienda_de_prueba(dir.path(), "herencia.db").await;
        let peticiones = Arc::new(std::sync::Mutex::new(Vec::new()));
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: peticiones.clone(),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store,
            jobs.clone(),
            proveedor,
            Some(registro_global(global.path())),
            None,
        );

        let (_child, job) = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("revisor".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let (modelo, sistema, esfuerzo) = peticiones.lock().unwrap()[0].clone();
        assert_eq!(modelo, "test", "sin modelo en el perfil, el del padre");
        assert_eq!(
            esfuerzo.as_deref(),
            Some("low"),
            "sin esfuerzo, el del padre"
        );
        assert!(sistema.contains("Revisa con cuidado."), "{sistema}");
    }

    #[tokio::test]
    async fn start_with_unknown_agent_creates_no_task() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "inexistente.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs,
            proveedor,
            Some(registro_global(global.path())),
            None,
        );

        let error = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("no-existe".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert!(error.to_string().contains("no-existe"), "{error}");
        assert!(store.list_subagents(parent).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn start_with_pending_project_agent_creates_no_task_and_says_so() {
        let proyecto = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        escribir_perfil(
            proyecto.path(),
            "revisor",
            &PERFIL_CON_PERSONA.replace("frontend", "revisor"),
        );
        let almacen = TrustStore::open(datos.path().to_path_buf()).unwrap();
        let registro = AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(
                proyecto.path().to_path_buf(),
                ProfileLevel::Project,
            )),
        ])
        .with_trust(almacen, proyecto.path().to_path_buf(), false);
        let (store, parent) = tienda_de_prueba(dir.path(), "pendiente.db").await;
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            Arc::new(LocalJobRegistry::default()),
            proveedor,
            Some(Arc::new(registro)),
            None,
        );

        let error = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("revisor".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("pendiente de aprobación"),
            "{error}"
        );
        assert!(error.to_string().contains("revisor"), "{error}");
        assert!(store.list_subagents(parent).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn start_with_remote_agent_creates_no_task() {
        let dir = tempfile::tempdir().unwrap();
        let registro = AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FuenteRemotaDePrueba),
        ]);
        let (store, parent) = tienda_de_prueba(dir.path(), "remoto.db").await;
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            Arc::new(LocalJobRegistry::default()),
            proveedor,
            Some(Arc::new(registro)),
            None,
        );

        let error = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("remoto".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert!(
            error.to_string().contains("agente remoto no disponible"),
            "{error}"
        );
        assert!(store.list_subagents(parent).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn start_with_model_unknown_to_catalog_is_a_validation_error() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        escribir_perfil(
            global.path(),
            "inventado",
            "---\nname: inventado\ndescription: Modelo falso\nmodel: m-inventado\n---\n\nCuerpo.\n",
        );
        escribir_perfil(
            global.path(),
            "real",
            "---\nname: real\ndescription: Modelo conocido\nmodel: m-real\n---\n\nCuerpo.\n",
        );
        // El catálogo tiene un proveedor con la URL base del padre (`http://localhost`).
        let catalogo = Catalog::from_json(
            r#"{"p": {"id": "p", "npm": "@ai-sdk/openai-compatible", "api": "http://localhost",
                "models": {"m-real": {"id": "m-real", "tool_call": true}}}}"#,
        )
        .unwrap();
        let (store, parent) = tienda_de_prueba(dir.path(), "catalogo.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            Some(registro_global(global.path())),
            Some(catalogo),
        );

        let error = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("inventado".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("m-inventado"), "{error}");
        assert!(store.list_subagents(parent).await.unwrap().is_empty());

        let (_child, job) = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("real".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();
    }

    #[tokio::test]
    async fn reload_after_start_does_not_change_the_running_task() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        escribir_perfil(
            global.path(),
            "frontend",
            "---\nname: frontend\ndescription: UI\nmodel: modelo-v1\n---\n\nv1.\n",
        );
        let (store, parent) = tienda_de_prueba(dir.path(), "recarga.db").await;
        let peticiones = Arc::new(std::sync::Mutex::new(Vec::new()));
        let puerta = Arc::new(tokio::sync::Notify::new());
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: peticiones.clone(),
            puerta: Some(puerta.clone()),
        };
        let registro = registro_global(global.path());
        let lanzador = lanzador_grabador(
            dir.path(),
            store,
            jobs.clone(),
            proveedor,
            Some(registro.clone()),
            None,
        );

        let (_child, job) = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("frontend".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        // El hijo queda retenido en la puerta, con su petición ya registrada.
        tokio::time::timeout(Duration::from_secs(5), async {
            while peticiones.lock().unwrap().is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("el hijo debería haber hecho su petición");

        escribir_perfil(
            global.path(),
            "frontend",
            "---\nname: frontend\ndescription: UI\nmodel: modelo-v2\n---\n\nv2.\n",
        );
        registro.reload();
        let recargado = match &registro.snapshot().get("frontend").unwrap().execution {
            Execution::Local(spec) => spec.model.clone(),
            Execution::Remote => None,
        };
        assert_eq!(
            recargado.as_deref(),
            Some("modelo-v2"),
            "la recarga sí cambió"
        );

        puerta.notify_one();
        let outcome = tokio::time::timeout(Duration::from_secs(5), jobs.wait(parent, &job))
            .await
            .expect("el hijo debería terminar")
            .unwrap();
        assert_eq!(outcome.status, JobStatus::Completed);
        assert_eq!(
            peticiones.lock().unwrap()[0].0,
            "modelo-v1",
            "la tarea en curso no cambia"
        );
    }

    #[tokio::test]
    async fn descriptor_records_agent_name_and_origin() {
        let global = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        escribir_perfil(global.path(), "frontend", PERFIL_CON_PERSONA);
        let (store, parent) = tienda_de_prueba(dir.path(), "descriptor.db").await;
        let jobs = Arc::new(LocalJobRegistry::default());
        let proveedor = ProveedorGrabador {
            peticiones: Arc::new(std::sync::Mutex::new(Vec::new())),
            puerta: None,
        };
        let lanzador = lanzador_grabador(
            dir.path(),
            store.clone(),
            jobs.clone(),
            proveedor,
            Some(registro_global(global.path())),
            None,
        );

        let (_child, job) = lanzador
            .start(
                parent,
                "obrero".into(),
                "hazlo".into(),
                Some("frontend".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        jobs.wait(parent, &job).await.unwrap();

        let hijo = &store.list_subagents(parent).await.unwrap()[0];
        assert_eq!(hijo.agent.as_deref(), Some("frontend"));
        let origen = AgentOrigin::File {
            path: global.path().join("frontend.md"),
            level: ProfileLevel::Global,
        };
        assert_eq!(
            hijo.agent_origin.as_deref(),
            Some(origen.as_text().as_str())
        );
    }

    #[tokio::test]
    async fn tracker_refuses_to_leave_a_terminal_state() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("tracker.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let child = store.create_session("/project").await.unwrap();
        store
            .save_subagent(&SubagentDescriptor {
                parent,
                child,
                job_id: "subagent-1".into(),
                label: "obrero".into(),
                status: TaskStatus::new(TaskState::Completed),
                artifacts: vec![Artifact::text_artifact("hecho")],
                notice_delivered: false,
                agent: None,
                agent_origin: None,
                reference_task_ids: Vec::new(),
            })
            .await
            .unwrap();
        let tracker = TaskTracker::new(store.clone(), EventBus::new(8));

        let error = tracker
            .transition(parent, child, TaskStatus::new(TaskState::Working))
            .await
            .unwrap_err();
        assert!(matches!(error, CoreError::Store(_)), "{error:?}");
        assert_eq!(
            store.list_subagents(parent).await.unwrap()[0].status.state,
            TaskState::Completed
        );
    }
}
