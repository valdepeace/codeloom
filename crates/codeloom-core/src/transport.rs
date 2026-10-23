//! Transporte de A2A: el contrato común de hablar con un agente, sea local o remoto.
//!
//! Cabe señalar que este módulo no conoce el cable: usa los tipos del núcleo (`Message`,
//! `SubagentDescriptor` en el papel de `Task`, `AgentCard`). `InProcessTransport` lo cumple en
//! memoria sobre el lanzador de subagentes; el transporte remoto (A8) lo cumplirá por HTTP.

use crate::a2a::{Message, PartContent};
use crate::card::AgentCard;
use crate::error::{CoreError, Result};
use crate::event::EventBus;
use crate::registry::{AgentRegistry, AGENTE_POR_DEFECTO};
use crate::session::{SessionId, SubagentDescriptor};
use crate::subagent::SubagentLauncher;
use crate::task::TaskState;
use crate::task_events::{flujo_de_tarea, Refrescar, StreamResponse};
use async_trait::async_trait;
use futures::stream::BoxStream;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Petición de `SendMessage`.
///
/// Sin `task_id` crea una tarea nueva; con él reanuda una tarea en `input-required`. Con
/// `reference_task_ids` la tarea nueva es el seguimiento de otra ya terminada (A11).
#[derive(Debug, Clone)]
pub struct SendRequest {
    /// Sesión del padre, dueña de las tareas.
    pub parent: SessionId,
    /// Agente destinatario; `None` es el agente por defecto.
    pub agent: Option<String>,
    /// Etiqueta de la tarea nueva (se ignora al reanudar).
    pub label: String,
    pub message: Message,
    /// Tarea que se reanuda.
    pub task_id: Option<SessionId>,
    /// Tareas a las que sigue la nueva; hoy se admite una sola.
    pub reference_task_ids: Vec<SessionId>,
}

/// Las operaciones de A2A que el núcleo ofrece, con independencia de cómo viajen.
///
/// `SubscribeToTask` queda para A9. Las tareas se devuelven como `SubagentDescriptor`, porque el
/// núcleo no tiene un tipo `Task` propio.
#[async_trait]
pub trait AgentTransport: Send + Sync {
    /// `SendMessage`: crea una tarea, la sigue o reanuda otra, y devuelve la tarea tras aceptar el
    /// envío.
    async fn send_message(
        &self,
        request: SendRequest,
        cancel: CancellationToken,
    ) -> Result<SubagentDescriptor>;

    /// `GetTask`. `TaskNotFound` si no existe o es de otro padre.
    async fn get_task(&self, parent: SessionId, task: SessionId) -> Result<SubagentDescriptor>;

    /// `ListTasks`: las tareas del padre.
    async fn list_tasks(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>>;

    /// `CancelTask`: devuelve la tarea actualizada (véase `SubagentLauncher::cancel`).
    async fn cancel_task(&self, parent: SessionId, task: SessionId) -> Result<SubagentDescriptor>;

    /// Tarjeta del agente; `None` es el agente por defecto.
    async fn agent_card(&self, agent: Option<&str>) -> Result<AgentCard>;

    /// `SubscribeToTask`: flujo de eventos de una tarea que aún no es terminal. Empieza con una
    /// instantánea y se cierra tras el estado terminal. `TaskNotFound` si no existe o es de otro
    /// padre; `UnsupportedOperation` si ya es terminal (así lo fija el proto).
    async fn subscribe_task(
        &self,
        _parent: SessionId,
        _task: SessionId,
    ) -> Result<BoxStream<'static, StreamResponse>> {
        Err(CoreError::UnsupportedOperation(
            "task subscription unavailable".into(),
        ))
    }
}

/// Transporte en memoria: traduce cada operación a una llamada al lanzador. No abre ningún socket.
pub struct InProcessTransport {
    launcher: Arc<dyn SubagentLauncher>,
    registry: Option<Arc<AgentRegistry>>,
    bus: Option<EventBus>,
}

impl InProcessTransport {
    pub fn new(launcher: Arc<dyn SubagentLauncher>, registry: Option<Arc<AgentRegistry>>) -> Self {
        Self {
            launcher,
            registry,
            bus: None,
        }
    }

    /// Bus del que se derivan los flujos de `subscribe_task`; sin él no hay suscripción.
    pub fn with_bus(mut self, bus: EventBus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Textos de un mensaje, uno por `Part`. Este transporte solo lleva texto: cualquier otra
    /// `Part`, o un mensaje sin ninguna, se rechaza antes de entregar nada.
    fn textos(message: &Message) -> Result<Vec<String>> {
        let textos: Vec<String> = message
            .parts
            .iter()
            .map(|part| match &part.content {
                PartContent::Text(texto) => Some(texto.clone()),
                _ => None,
            })
            .collect::<Option<_>>()
            .ok_or_else(|| {
                CoreError::Tool("the in-process transport only carries text parts".into())
            })?;
        if textos.is_empty() {
            return Err(CoreError::Tool("a message needs at least one part".into()));
        }
        Ok(textos)
    }
}

#[async_trait]
impl AgentTransport for InProcessTransport {
    async fn send_message(
        &self,
        request: SendRequest,
        cancel: CancellationToken,
    ) -> Result<SubagentDescriptor> {
        let textos = Self::textos(&request.message)?;
        if let Some(task) = request.task_id {
            if !request.reference_task_ids.is_empty() {
                return Err(CoreError::Tool(
                    "a message cannot both resume a task and reference others".into(),
                ));
            }
            let actual = self.get_task(request.parent, task).await?;
            if actual.status.state != TaskState::InputRequired {
                return Err(CoreError::Tool(format!(
                    "task {} is not waiting for input (state: {})",
                    task.0,
                    actual.status.state.proto_name()
                )));
            }
            // Cada `Part` de texto es la respuesta a una pregunta, en orden. El broker comprueba
            // que el número coincida y, si no, no entrega nada.
            self.launcher.answer(request.parent, task, textos).await?;
            return self.get_task(request.parent, task).await;
        }
        let prompt = textos.join("\n");
        let (child, _job) = match request.reference_task_ids.as_slice() {
            [] => {
                self.launcher
                    .start(request.parent, request.label, prompt, request.agent, cancel)
                    .await?
            }
            [anterior] => {
                self.launcher
                    .start_follow_up(
                        request.parent,
                        request.label,
                        prompt,
                        request.agent,
                        *anterior,
                        cancel,
                    )
                    .await?
            }
            _ => {
                return Err(CoreError::Tool(
                    "only one referenced task is supported".into(),
                ))
            }
        };
        self.get_task(request.parent, child).await
    }

    async fn get_task(&self, parent: SessionId, task: SessionId) -> Result<SubagentDescriptor> {
        self.launcher
            .list(parent)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == task)
            .ok_or(CoreError::TaskNotFound(task.0))
    }

    async fn list_tasks(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
        self.launcher.list(parent).await
    }

    async fn cancel_task(&self, parent: SessionId, task: SessionId) -> Result<SubagentDescriptor> {
        self.launcher.cancel(parent, task).await
    }

    async fn agent_card(&self, agent: Option<&str>) -> Result<AgentCard> {
        let registry = self
            .registry
            .as_ref()
            .ok_or_else(|| CoreError::Tool("no agent registry configured".into()))?;
        let nombre = agent.unwrap_or(AGENTE_POR_DEFECTO);
        registry
            .snapshot()
            .get(nombre)
            .map(|entrada| entrada.card.clone())
            .ok_or_else(|| CoreError::Tool(format!("unknown agent «{nombre}»")))
    }

    async fn subscribe_task(
        &self,
        parent: SessionId,
        task: SessionId,
    ) -> Result<BoxStream<'static, StreamResponse>> {
        let bus = self.bus.as_ref().ok_or_else(|| {
            CoreError::UnsupportedOperation("task subscription unavailable".into())
        })?;
        // Se suscribe antes de leer la tarea: un cambio entre ambas cosas no se pierde.
        let rx = bus.subscribe();
        let actual = self.get_task(parent, task).await?;
        if actual.status.state.is_terminal() {
            return Err(CoreError::UnsupportedOperation(format!(
                "task {} is already in a terminal state ({})",
                task.0,
                actual.status.state.proto_name()
            )));
        }
        let lanzador = self.launcher.clone();
        let refrescar: Refrescar = Box::new(move || {
            let lanzador = lanzador.clone();
            Box::pin(async move {
                lanzador
                    .list(parent)
                    .await
                    .ok()?
                    .into_iter()
                    .find(|descriptor| descriptor.child == task)
            })
        });
        Ok(flujo_de_tarea(rx, actual, refrescar))
    }
}

/// Suite de contrato: lo que cualquier `AgentTransport` debe cumplir, sea local o remoto.
///
/// Cabe señalar que solo comprueba comportamientos independientes del transporte (la finalización
/// de las tareas depende de cada agente). A8 la ejecutará contra el transporte remoto.
#[cfg(any(test, feature = "test-support"))]
pub mod contract {
    use super::*;
    use crate::a2a::{Part, Role};

    fn mensaje(partes: Vec<Part>) -> Message {
        Message {
            message_id: "contrato-1".into(),
            context_id: None,
            task_id: None,
            role: Role::User,
            parts: partes,
            metadata: None,
            extensions: Vec::new(),
            reference_task_ids: Vec::new(),
        }
    }

    /// Ejecuta el contrato. `parent` y `otro_padre` son sesiones distintas, y `otro_padre` no debe
    /// tener tareas todavía.
    pub async fn transport_contract(
        transporte: &dyn AgentTransport,
        parent: SessionId,
        otro_padre: SessionId,
    ) {
        let peticion = |partes: Vec<Part>| SendRequest {
            parent,
            agent: None,
            label: "contrato".into(),
            message: mensaje(partes),
            task_id: None,
            reference_task_ids: Vec::new(),
        };

        // Una tarea inexistente no se encuentra ni se cancela.
        assert!(matches!(
            transporte.get_task(parent, SessionId(987_654)).await,
            Err(CoreError::TaskNotFound(987_654))
        ));
        assert!(matches!(
            transporte.cancel_task(parent, SessionId(987_654)).await,
            Err(CoreError::TaskNotFound(987_654))
        ));
        // Un mensaje sin partes no crea nada.
        assert!(transporte
            .send_message(peticion(Vec::new()), CancellationToken::new())
            .await
            .is_err());
        assert!(transporte.list_tasks(parent).await.unwrap().is_empty());

        // Una tarea aceptada se puede consultar y listar, y es del padre que la creó.
        let tarea = transporte
            .send_message(
                peticion(vec![Part::text("haz algo")]),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(tarea.parent, parent);
        let consultada = transporte.get_task(parent, tarea.child).await.unwrap();
        assert_eq!(consultada.child, tarea.child);
        let listadas = transporte.list_tasks(parent).await.unwrap();
        assert!(listadas.iter().any(|t| t.child == tarea.child));

        // Otro padre ni la ve ni sabe que existe.
        assert!(matches!(
            transporte.get_task(otro_padre, tarea.child).await,
            Err(CoreError::TaskNotFound(_))
        ));
        assert!(matches!(
            transporte.cancel_task(otro_padre, tarea.child).await,
            Err(CoreError::TaskNotFound(_))
        ));
        assert!(transporte.list_tasks(otro_padre).await.unwrap().is_empty());

        // Suscribirse: una tarea ajena o inexistente no se encuentra; la propia o bien ya es
        // terminal (no soportado) o bien abre un flujo que empieza con una instantánea.
        assert!(matches!(
            transporte.subscribe_task(parent, SessionId(987_654)).await,
            Err(CoreError::TaskNotFound(987_654))
        ));
        assert!(matches!(
            transporte.subscribe_task(otro_padre, tarea.child).await,
            Err(CoreError::TaskNotFound(_))
        ));
        match transporte.subscribe_task(parent, tarea.child).await {
            Err(CoreError::UnsupportedOperation(_)) => {}
            Ok(mut flujo) => {
                use futures::StreamExt;
                assert!(matches!(flujo.next().await, Some(StreamResponse::Task(_))));
            }
            Err(otro) => panic!("suscripción inesperada: {otro:?}"),
        }

        // La tarjeta del agente por defecto existe; la de un agente desconocido, no.
        assert!(transporte.agent_card(None).await.is_ok());
        assert!(transporte.agent_card(Some("no-existe")).await.is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::a2a::{Part, Role};
    use crate::config::{CodeLoomConfig, ProviderConfig, ProviderKind};
    use crate::event::EventBus;
    use crate::job::LocalJobRegistry;
    use crate::message::FinishReason;
    use crate::permission::PermissionService;
    use crate::provider::{Provider, StreamChunk};
    use crate::registry::BuiltInSource;
    use crate::session::sqlite::SqliteStore;
    use crate::session::SessionStore;
    use crate::subagent::InProcessSubagentLauncher;
    use crate::tools::ToolRegistry;
    use std::sync::RwLock;
    use std::time::Duration;

    /// Proveedor de prueba: sirve un guion por paso y, agotados, responde `listo`.
    struct ColaProveedor {
        guiones: std::sync::Mutex<std::collections::VecDeque<Vec<StreamChunk>>>,
    }

    #[async_trait]
    impl crate::provider::Provider for ColaProveedor {
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
            let guion = self.guiones.lock().unwrap().pop_front().unwrap_or_else(|| {
                vec![
                    StreamChunk::TextDelta("listo".into()),
                    StreamChunk::Done(FinishReason::Stop),
                ]
            });
            Ok(Box::pin(futures::stream::iter(guion)))
        }
    }

    fn pregunta() -> Vec<StreamChunk> {
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
        ]
    }

    struct Banco {
        transporte: InProcessTransport,
        store: Arc<SqliteStore>,
        parent: SessionId,
        _dir: tempfile::TempDir,
    }

    /// Transporte sobre un lanzador real; `guiones` son los pasos que servirá el proveedor.
    async fn banco(guiones: Vec<Vec<StreamChunk>>) -> Banco {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("transporte.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let parent = store.create_session("/project").await.unwrap();
        let provider: Arc<dyn Provider> = Arc::new(ColaProveedor {
            guiones: std::sync::Mutex::new(guiones.into()),
        });
        let mut herramientas = ToolRegistry::new();
        herramientas.register(Arc::new(crate::tools::ask_user::AskUserTool));
        let registro = Arc::new(AgentRegistry::new(vec![Box::new(BuiltInSource)]));
        registro.reload();
        let bus = EventBus::new(256);
        let lanzador = InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(provider)),
            store.clone(),
            Arc::new(herramientas),
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
            bus.clone(),
        )
        .with_registry(registro.clone());
        Banco {
            transporte: InProcessTransport::new(Arc::new(lanzador), Some(registro)).with_bus(bus),
            store,
            parent,
            _dir: dir,
        }
    }

    fn mensaje(texto: &str) -> Message {
        Message {
            message_id: "m-1".into(),
            context_id: None,
            task_id: None,
            role: Role::User,
            parts: vec![Part::text(texto)],
            metadata: None,
            extensions: Vec::new(),
            reference_task_ids: Vec::new(),
        }
    }

    fn peticion(banco: &Banco, texto: &str) -> SendRequest {
        SendRequest {
            parent: banco.parent,
            agent: None,
            label: "obrero".into(),
            message: mensaje(texto),
            task_id: None,
            reference_task_ids: Vec::new(),
        }
    }

    async fn espera(banco: &Banco, tarea: SessionId, estado: TaskState) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let actual = banco.transporte.get_task(banco.parent, tarea).await;
                if matches!(&actual, Ok(d) if d.status.state == estado) {
                    return;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("la tarea debería quedar en {estado:?}"));
    }

    #[tokio::test]
    async fn a_new_message_starts_a_task() {
        let banco = banco(vec![]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "haz algo"), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(tarea.parent, banco.parent);
        assert_eq!(tarea.label, "obrero");
        assert!(tarea.reference_task_ids.is_empty());
        espera(&banco, tarea.child, TaskState::Completed).await;
        let hecha = banco
            .transporte
            .get_task(banco.parent, tarea.child)
            .await
            .unwrap();
        assert_eq!(hecha.result_text().as_deref(), Some("listo"));
    }

    #[tokio::test]
    async fn a_message_with_references_starts_a_follow_up() {
        let banco = banco(vec![]).await;
        let primera = banco
            .transporte
            .send_message(peticion(&banco, "primero"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, primera.child, TaskState::Completed).await;

        let mut seguimiento = peticion(&banco, "ahora lo siguiente");
        seguimiento.reference_task_ids = vec![primera.child];
        let segunda = banco
            .transporte
            .send_message(seguimiento, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(segunda.reference_task_ids, vec![primera.child]);
        let mut dos = peticion(&banco, "x");
        dos.reference_task_ids = vec![primera.child, segunda.child];
        assert!(matches!(
            banco
                .transporte
                .send_message(dos, CancellationToken::new())
                .await,
            Err(CoreError::Tool(_))
        ));
    }

    #[tokio::test]
    async fn get_task_of_another_parent_is_not_found() {
        let banco = banco(vec![]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "haz algo"), CancellationToken::new())
            .await
            .unwrap();
        let otro = banco.store.create_session("/project").await.unwrap();

        match banco.transporte.get_task(otro, tarea.child).await {
            Err(CoreError::TaskNotFound(id)) => assert_eq!(id, tarea.child.0),
            otro => panic!("se esperaba TaskNotFound, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn list_tasks_returns_only_the_parents_children() {
        let banco = banco(vec![]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "haz algo"), CancellationToken::new())
            .await
            .unwrap();
        let otro = banco.store.create_session("/project").await.unwrap();

        let propias = banco.transporte.list_tasks(banco.parent).await.unwrap();
        assert_eq!(propias.len(), 1);
        assert_eq!(propias[0].child, tarea.child);
        assert!(banco.transporte.list_tasks(otro).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_message_resumes_a_task_waiting_for_input() {
        let banco = banco(vec![pregunta()]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "decide algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::InputRequired).await;

        let mut respuesta = peticion(&banco, "Sí");
        respuesta.task_id = Some(tarea.child);
        banco
            .transporte
            .send_message(respuesta, CancellationToken::new())
            .await
            .unwrap();

        espera(&banco, tarea.child, TaskState::Completed).await;
    }

    #[tokio::test]
    async fn a_message_to_a_task_not_waiting_is_refused_and_delivers_nothing() {
        let banco = banco(vec![]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "haz algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::Completed).await;
        let antes = banco.store.messages(tarea.child).await.unwrap().len();

        let mut respuesta = peticion(&banco, "Sí");
        respuesta.task_id = Some(tarea.child);
        match banco
            .transporte
            .send_message(respuesta, CancellationToken::new())
            .await
        {
            Err(CoreError::Tool(texto)) => {
                assert!(texto.contains("TASK_STATE_COMPLETED"), "{texto}");
            }
            otro => panic!("se esperaba un error de herramienta, y salió {otro:?}"),
        }
        assert_eq!(
            banco.store.messages(tarea.child).await.unwrap().len(),
            antes
        );
    }

    #[tokio::test]
    async fn a_resume_with_the_wrong_number_of_answers_is_refused() {
        let banco = banco(vec![pregunta()]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "decide algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::InputRequired).await;

        let mut dos = peticion(&banco, "Sí");
        dos.task_id = Some(tarea.child);
        dos.message.parts.push(Part::text("No"));
        assert!(matches!(
            banco
                .transporte
                .send_message(dos, CancellationToken::new())
                .await,
            Err(CoreError::Tool(_))
        ));
        // La pregunta sigue esperando: una respuesta correcta aún la reanuda.
        let mut una = peticion(&banco, "Sí");
        una.task_id = Some(tarea.child);
        banco
            .transporte
            .send_message(una, CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::Completed).await;
    }

    #[tokio::test]
    async fn a_resume_with_a_non_text_part_is_refused() {
        let banco = banco(vec![pregunta()]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "decide algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::InputRequired).await;

        let mut con_url = peticion(&banco, "Sí");
        con_url.task_id = Some(tarea.child);
        con_url.message.parts = vec![Part {
            content: PartContent::Url("https://e.example/x".into()),
            metadata: None,
            filename: None,
            media_type: None,
        }];
        assert!(matches!(
            banco
                .transporte
                .send_message(con_url, CancellationToken::new())
                .await,
            Err(CoreError::Tool(_))
        ));
        let sin_partes = SendRequest {
            message: Message {
                parts: Vec::new(),
                ..mensaje("x")
            },
            ..peticion(&banco, "x")
        };
        assert!(matches!(
            banco
                .transporte
                .send_message(sin_partes, CancellationToken::new())
                .await,
            Err(CoreError::Tool(_))
        ));
        let aun = banco
            .transporte
            .get_task(banco.parent, tarea.child)
            .await
            .unwrap();
        assert_eq!(aun.status.state, TaskState::InputRequired);
    }

    #[tokio::test]
    async fn a_message_cannot_resume_and_reference_at_once() {
        let banco = banco(vec![pregunta()]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "decide algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::InputRequired).await;

        let mut mezcla = peticion(&banco, "Sí");
        mezcla.task_id = Some(tarea.child);
        mezcla.reference_task_ids = vec![tarea.child];

        assert!(matches!(
            banco
                .transporte
                .send_message(mezcla, CancellationToken::new())
                .await,
            Err(CoreError::Tool(_))
        ));
    }

    #[tokio::test]
    async fn cancel_task_returns_the_canceled_task() {
        let banco = banco(vec![pregunta()]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "decide algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::InputRequired).await;

        let cancelada = banco
            .transporte
            .cancel_task(banco.parent, tarea.child)
            .await
            .unwrap();

        assert_eq!(cancelada.status.state, TaskState::Canceled);
    }

    #[tokio::test]
    async fn agent_card_of_default_and_of_unknown() {
        let banco = banco(vec![]).await;

        let tarjeta = banco.transporte.agent_card(None).await.unwrap();
        assert_eq!(tarjeta.name, AGENTE_POR_DEFECTO);
        assert!(banco
            .transporte
            .agent_card(Some("no-existe"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn the_in_process_transport_meets_the_contract() {
        let banco = banco(vec![]).await;
        let otro = banco.store.create_session("/project").await.unwrap();

        contract::transport_contract(&banco.transporte, banco.parent, otro).await;
    }

    #[tokio::test]
    async fn subscribe_to_an_unknown_task_is_not_found() {
        let banco = banco(vec![]).await;

        match banco
            .transporte
            .subscribe_task(banco.parent, SessionId(987_654))
            .await
        {
            Err(CoreError::TaskNotFound(id)) => assert_eq!(id, 987_654),
            otro => panic!("se esperaba TaskNotFound, y salió {:?}", otro.err()),
        }
    }

    #[tokio::test]
    async fn subscribe_to_a_terminal_task_is_unsupported() {
        let banco = banco(vec![]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "haz algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::Completed).await;

        match banco
            .transporte
            .subscribe_task(banco.parent, tarea.child)
            .await
        {
            Err(CoreError::UnsupportedOperation(texto)) => {
                assert!(texto.contains("TASK_STATE_COMPLETED"), "{texto}");
            }
            otro => panic!("se esperaba UnsupportedOperation, y salió {:?}", otro.err()),
        }
    }

    #[tokio::test]
    async fn a_full_run_streams_the_snapshot_then_completed() {
        use futures::StreamExt;
        let banco = banco(vec![pregunta()]).await;
        let tarea = banco
            .transporte
            .send_message(peticion(&banco, "decide algo"), CancellationToken::new())
            .await
            .unwrap();
        espera(&banco, tarea.child, TaskState::InputRequired).await;
        let mut flujo = banco
            .transporte
            .subscribe_task(banco.parent, tarea.child)
            .await
            .unwrap();
        let primero = tokio::time::timeout(Duration::from_secs(2), flujo.next())
            .await
            .unwrap();
        match primero {
            Some(StreamResponse::Task(t)) => {
                assert_eq!(t.status.state, TaskState::InputRequired);
            }
            otro => panic!("se esperaba la instantánea, y salió {otro:?}"),
        }

        let mut respuesta = peticion(&banco, "Sí");
        respuesta.task_id = Some(tarea.child);
        banco
            .transporte
            .send_message(respuesta, CancellationToken::new())
            .await
            .unwrap();
        let resto: Vec<StreamResponse> =
            tokio::time::timeout(Duration::from_secs(5), flujo.collect::<Vec<_>>())
                .await
                .expect("el flujo debería cerrarse al terminar la tarea");

        let estados: Vec<TaskState> = resto
            .iter()
            .filter_map(|e| match e {
                StreamResponse::StatusUpdate(u) => Some(u.status.state),
                _ => None,
            })
            .collect();
        assert_eq!(estados.last(), Some(&TaskState::Completed), "{estados:?}");
        assert!(resto.iter().any(|e| matches!(
            e,
            StreamResponse::ArtifactUpdate(a) if a.artifact.text() == "listo"
        )));
    }
}
