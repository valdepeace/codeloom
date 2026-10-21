use crate::agent::AgentLoop;
use crate::catalog::Catalog;
use crate::config::CodeLoomConfig;
use crate::error::{CoreError, Result};
use crate::event::{EventBus, LiveEvent};
use crate::job::{JobId, JobRegistry, JobResult, JobSpec};
use crate::permission::PermissionService;
use crate::provider::Provider;
use crate::session::{SessionId, SessionStore, SubagentDescriptor, SubagentStatus};
use crate::subagent_input::{ChildResponder, InputBroker, PendingInput};
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
        cancel: CancellationToken,
    ) -> Result<(SessionId, JobId)>;

    async fn control(
        &self,
        parent: SessionId,
        child: SessionId,
        action: &str,
        message: Option<String>,
    ) -> Result<()>;

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

impl InProcessSubagentLauncher {
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
        }
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
}

#[async_trait]
impl SubagentLauncher for InProcessSubagentLauncher {
    async fn start(
        &self,
        parent: SessionId,
        label: String,
        prompt: String,
        parent_cancel: CancellationToken,
    ) -> Result<(SessionId, JobId)> {
        if prompt.trim().is_empty() || prompt.len() > 32_768 || label.len() > 128 {
            return Err(CoreError::Tool("invalid subagent prompt or label".into()));
        }
        let permit = self
            .active
            .clone()
            .try_acquire_owned()
            .map_err(|_| CoreError::Tool("maximum of 8 active subagents reached".into()))?;
        let project = self.cwd.to_string_lossy().into_owned();
        let child = self.store.create_session(&project).await?;
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
            ),
        )));
        let child_broker = self.broker.clone();
        let child_config = self.config.read().unwrap().clone();
        let child_cwd = self.cwd.clone();
        let child_jobs = self.jobs.clone();
        let child_catalog = self.catalog.clone();
        let descriptor_label = label.clone();
        // El bus del hijo se crea y se suscribe antes de lanzarlo, para no perder ningún
        // evento de su arranque. Su canal se cierra cuando se sueltan todos sus emisores.
        let child_bus = EventBus::new(256);
        let mut child_rx = child_bus.subscribe();
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
                    let child_agent = AgentLoop::new(
                        child_provider,
                        child_store.clone(),
                        child_tools,
                        child_bus,
                        child_permission,
                        child_config,
                        child_cwd,
                    )
                    .with_jobs(child_jobs)
                    .with_catalog(child_catalog);
                    let child_cancel = CancellationToken::new();
                    let outcome = tokio::select! {
                        result = child_agent.run(child, prompt, child_cancel.clone()) => result?,
                        _ = job_cancel.cancelled() => {
                            child_cancel.cancel();
                            return Ok(None);
                        }
                        _ = parent_cancel.cancelled() => {
                            child_cancel.cancel();
                            return Ok(None);
                        }
                    };
                    // El agente ya no emite: soltarlo cierra el bus del hijo y permite
                    // que la tarea de reenvío termine.
                    drop(child_agent);
                    let text = outcome.final_text;
                    child_store.complete_subagent(parent, child, &text).await?;
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
        let descriptor = SubagentDescriptor {
            parent,
            child,
            job_id: job.id().0.clone(),
            label: descriptor_label,
            status: SubagentStatus::Running,
            result: None,
            notice_delivered: false,
        };
        if let Err(error) = self.store.save_subagent(&descriptor).await {
            let _ = self.jobs.kill(parent, job.id());
            return Err(error);
        }
        let mut updates = self.jobs.subscribe(parent, job.id())?;
        let store = self.store.clone();
        let settle_bus = self.bus.clone();
        tokio::spawn(async move {
            loop {
                let snapshot = updates.borrow_and_update().clone();
                let status = match snapshot.status {
                    crate::job::JobStatus::Completed => Some(SubagentStatus::Completed),
                    crate::job::JobStatus::Killed => Some(SubagentStatus::Killed),
                    crate::job::JobStatus::Failed => Some(SubagentStatus::Failed),
                    _ => None,
                };
                if let Some(status) = status {
                    if let Err(error) = store.update_subagent_status(parent, child, status).await {
                        tracing::warn!("persist subagent status: {error}");
                    }
                    // Se publica tras el intento de persistir, también si falló: así el
                    // padre nunca lee el aviso antes del estado final.
                    settle_bus.publish(LiveEvent::SubagentSettled {
                        parent,
                        child,
                        status,
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
        let descriptor = self
            .store
            .list_subagents(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
            .ok_or_else(|| CoreError::Tool("unknown subagent".into()))?;
        let job = JobId(descriptor.job_id);
        if descriptor.status != SubagentStatus::Running {
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
            "interrupt" => self.jobs.kill(parent, &job),
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

    async fn list(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
        self.store.list_subagents(parent).await
    }

    async fn deliver_notices(&self, parent: SessionId) -> Result<Vec<crate::message::Message>> {
        let mut messages = Vec::new();
        for descriptor in self.store.list_subagents(parent).await? {
            if descriptor.status != SubagentStatus::Completed || descriptor.notice_delivered {
                continue;
            }
            if let Some(result) = descriptor.result {
                let message = crate::message::Message::user(format!(
                    "[Subagent {} completed]\n{result}",
                    descriptor.label
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
    use crate::session::sqlite::SqliteStore;
    use crate::tools::subagent::SubagentTool;
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

        assert_eq!(status, SubagentStatus::Completed);
        // Al llegar el evento, el estado final ya debe estar persistido y el aviso listo.
        assert!(store
            .list_subagents(parent)
            .await
            .unwrap()
            .iter()
            .any(|d| d.child == child && d.status == SubagentStatus::Completed));
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
}
