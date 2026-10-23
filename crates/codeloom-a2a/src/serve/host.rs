//! Adaptador del servidor sobre el lanzador de subagentes (A8b).
//!
//! Cabe señalar que servir no inventa un ejecutor nuevo: cada tarea entrante es un subagente local
//! lanzado bajo una sesión padre propia del servidor. Así hereda el estado A2A, la cancelación, el
//! almacén y la observabilidad que ya existen. Lo que sí añade es lo propio de servir: un tope de
//! tareas vivas, un tiempo máximo por tarea, un tope del texto devuelto y la certeza de que solo
//! se ven las tareas de esta sesión.

use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use codeloom_core::error::CoreError;
use codeloom_core::event::{EventBus, LiveEvent};
use codeloom_core::session::{SessionId, SessionStore, SubagentDescriptor};
use codeloom_core::subagent::SubagentLauncher;
use codeloom_core::tools::glob::GlobTool;
use codeloom_core::tools::grep::GrepTool;
use codeloom_core::tools::view::ViewTool;
use codeloom_core::tools::ToolRegistry;
use tokio_util::sync::CancellationToken;

use super::config::ServeConfig;
use super::confine::ConfinedView;
use super::rpc::{HostError, TaskHost, TaskView};
use crate::wire::WireTask;

/// Cada cuánto se consulta el estado mientras se espera.
const SONDEO: Duration = Duration::from_millis(50);
/// Texto máximo que se devuelve por tarea (decálogo: nada sin cota llega al cliente).
pub const MAX_RESULT_BYTES: usize = 32 * 1024;
/// Etiqueta de las tareas entrantes en el almacén.
const ETIQUETA: &str = "A2A entrante";

/// Herramientas del agente servido: solo lectura, y la lectura de ficheros confinada al proyecto.
/// Ni escritura, ni `bash`, ni jobs, ni subagentes, ni preguntas (D4 de la spec).
pub fn served_tools() -> ToolRegistry {
    let mut herramientas = ToolRegistry::new();
    herramientas.register(Arc::new(ConfinedView(ViewTool)));
    herramientas.register(Arc::new(GlobTool));
    herramientas.register(Arc::new(GrepTool));
    herramientas
}

/// Cotas del anfitrión.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostLimits {
    pub max_concurrent: usize,
    pub max_task: Duration,
    /// Bytes de texto que se devuelven como máximo por tarea.
    pub max_result_bytes: usize,
}

impl HostLimits {
    pub fn from_config(serve: &ServeConfig, max_result_bytes: usize) -> Self {
        HostLimits {
            max_concurrent: serve.max_concurrent_tasks,
            max_task: Duration::from_secs(serve.max_task_secs),
            max_result_bytes,
        }
    }
}

/// Anfitrión de las tareas entrantes.
pub struct LauncherHost {
    launcher: Arc<dyn SubagentLauncher>,
    parent: SessionId,
    agent: String,
    limits: HostLimits,
    bus: Option<EventBus>,
    /// Serializa «contar tareas vivas y arrancar»: sin esto dos envíos simultáneos pasarían el tope.
    arranque: tokio::sync::Mutex<()>,
}

impl LauncherHost {
    /// Crea la sesión padre del servidor (una por proceso) y devuelve el anfitrión.
    pub async fn new(
        launcher: Arc<dyn SubagentLauncher>,
        store: &dyn SessionStore,
        cwd: &str,
        agent: String,
        limits: HostLimits,
    ) -> codeloom_core::error::Result<Self> {
        let parent = store.create_session(cwd).await?;
        Ok(LauncherHost {
            launcher,
            parent,
            agent,
            limits,
            bus: None,
            arranque: tokio::sync::Mutex::new(()),
        })
    }

    /// Publica en el bus cada cambio de estado de una tarea entrante, para que la persona usuaria
    /// la vea en la conversación.
    pub fn with_bus(mut self, bus: EventBus) -> Self {
        self.bus = Some(bus);
        self
    }

    /// Sesión padre bajo la que viven las tareas entrantes.
    pub fn parent(&self) -> SessionId {
        self.parent
    }

    async fn descriptor(&self, id: &str) -> Result<SubagentDescriptor, HostError> {
        // Un id que no es un número es, para el cliente, una tarea que no existe.
        let hijo = id.parse::<i64>().map_err(|_| HostError::NotFound)?;
        self.launcher
            .list(self.parent)
            .await
            .map_err(otro)?
            .into_iter()
            .find(|descriptor| descriptor.child.0 == hijo)
            .ok_or(HostError::NotFound)
    }

    fn vista(&self, descriptor: &SubagentDescriptor) -> TaskView {
        let mut tarea = WireTask::from(descriptor);
        acotar_texto(&mut tarea, self.limits.max_result_bytes);
        TaskView {
            json: serde_json::to_value(&tarea).unwrap_or(serde_json::Value::Null),
            terminal: descriptor.status.state.is_terminal(),
        }
    }
}

fn otro(error: CoreError) -> HostError {
    HostError::Other(error.to_string())
}

/// Acota el texto total de los artefactos y del mensaje de estado a `max` bytes.
fn acotar_texto(tarea: &mut WireTask, max: usize) {
    let mut presupuesto = max;
    let mensajes = tarea
        .status
        .message
        .iter_mut()
        .flat_map(|mensaje| mensaje.parts.iter_mut());
    let artefactos = tarea
        .artifacts
        .iter_mut()
        .flat_map(|artefacto| artefacto.parts.iter_mut());
    for parte in artefactos.chain(mensajes) {
        let Some(texto) = parte.text.as_mut() else {
            continue;
        };
        if texto.len() <= presupuesto {
            presupuesto -= texto.len();
            continue;
        }
        let mut fin = presupuesto;
        while !texto.is_char_boundary(fin) {
            fin -= 1;
        }
        texto.truncate(fin);
        texto.push_str("… [truncated]");
        presupuesto = 0;
    }
}

#[async_trait]
impl TaskHost for LauncherHost {
    async fn start(&self, prompt: String) -> Result<TaskView, HostError> {
        let _turno = self.arranque.lock().await;
        let vivas = self
            .launcher
            .list(self.parent)
            .await
            .map_err(otro)?
            .iter()
            .filter(|descriptor| !descriptor.status.state.is_terminal())
            .count();
        if vivas >= self.limits.max_concurrent {
            return Err(HostError::Busy);
        }
        let (hijo, _) = self
            .launcher
            .start(
                self.parent,
                ETIQUETA.to_string(),
                prompt,
                Some(self.agent.clone()),
                CancellationToken::new(),
            )
            .await
            .map_err(otro)?;

        // Una tarea no puede durar más que el máximo: pasado el tiempo, se cancela.
        let (lanzador, padre, maximo) = (self.launcher.clone(), self.parent, self.limits.max_task);
        let (bus, agente) = (self.bus.clone(), self.agent.clone());
        let aviso = move |d: &SubagentDescriptor| {
            if let Some(bus) = &bus {
                bus.publish(LiveEvent::A2aServed {
                    task: d.child.0.to_string(),
                    agent: agente.clone(),
                    state: d.status.state.proto_name().to_string(),
                });
            }
        };
        tokio::spawn(async move {
            let limite = Instant::now() + maximo;
            let mut ultimo = None;
            loop {
                tokio::time::sleep(SONDEO.max(Duration::from_millis(100))).await;
                let actual = lanzador
                    .list(padre)
                    .await
                    .ok()
                    .and_then(|lista| lista.into_iter().find(|d| d.child == hijo));
                if let Some(descriptor) = &actual {
                    // Solo se avisa cuando el estado cambia: la conversación no es un registro de sondeos.
                    if ultimo != Some(descriptor.status.state) {
                        ultimo = Some(descriptor.status.state);
                        aviso(descriptor);
                    }
                }
                if actual.is_none_or(|d| d.status.state.is_terminal()) {
                    return;
                }
                if Instant::now() >= limite {
                    // Si ya terminó entre medias, `cancel` falla con `TaskNotCancelable`: es lo esperado.
                    if let Ok(cancelada) = lanzador.cancel(padre, hijo).await {
                        aviso(&cancelada);
                    }
                    return;
                }
            }
        });

        let descriptor = self.descriptor(&hijo.0.to_string()).await?;
        Ok(self.vista(&descriptor))
    }

    async fn get(&self, id: &str) -> Result<TaskView, HostError> {
        Ok(self.vista(&self.descriptor(id).await?))
    }

    async fn cancel(&self, id: &str) -> Result<TaskView, HostError> {
        let hijo = id.parse::<i64>().map_err(|_| HostError::NotFound)?;
        match self.launcher.cancel(self.parent, SessionId(hijo)).await {
            Ok(descriptor) => Ok(self.vista(&descriptor)),
            Err(CoreError::TaskNotFound(_)) => Err(HostError::NotFound),
            Err(CoreError::TaskNotCancelable { .. }) => Err(HostError::NotCancelable),
            Err(error) => Err(otro(error)),
        }
    }

    async fn wait_terminal(&self, id: &str, max: Duration) -> Result<TaskView, HostError> {
        let limite = Instant::now() + max;
        loop {
            let vista = self.get(id).await?;
            if vista.terminal || Instant::now() >= limite {
                return Ok(vista);
            }
            tokio::time::sleep(SONDEO).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeloom_core::config::{CodeLoomConfig, ProviderConfig, ProviderKind};
    use codeloom_core::error::Result as CoreResult;
    use codeloom_core::event::EventBus;
    use codeloom_core::job::LocalJobRegistry;
    use codeloom_core::message::FinishReason;
    use codeloom_core::permission::PermissionService;
    use codeloom_core::provider::{
        Capabilities, Provider, ProviderRequest, ProviderStream, StreamChunk,
    };
    use codeloom_core::registry::{AgentRegistry, BuiltInSource};
    use codeloom_core::session::sqlite::SqliteStore;
    use codeloom_core::subagent::InProcessSubagentLauncher;
    use futures::stream;
    use std::sync::RwLock;

    /// Responde siempre con el mismo texto.
    struct ConTexto(String);

    /// No responde nunca: la tarea se queda en `working` hasta que la cancelen.
    struct Colgado;

    fn capacidades() -> Capabilities {
        Capabilities {
            tool_use: true,
            reasoning: false,
            streaming: true,
        }
    }

    #[async_trait]
    impl Provider for ConTexto {
        fn capabilities(&self) -> Capabilities {
            capacidades()
        }
        async fn stream(
            &self,
            _req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> CoreResult<ProviderStream> {
            let trozos = vec![
                StreamChunk::Started,
                StreamChunk::TextDelta(self.0.clone()),
                StreamChunk::Done(FinishReason::Stop),
            ];
            Ok(Box::pin(stream::iter(trozos)))
        }
    }

    #[async_trait]
    impl Provider for Colgado {
        fn capabilities(&self) -> Capabilities {
            capacidades()
        }
        async fn stream(
            &self,
            _req: ProviderRequest,
            _cancel: CancellationToken,
        ) -> CoreResult<ProviderStream> {
            Ok(Box::pin(stream::pending::<StreamChunk>()))
        }
    }

    struct Montaje {
        host: LauncherHost,
        launcher: Arc<InProcessSubagentLauncher>,
        otro_padre: SessionId,
        bus: EventBus,
        _dir: tempfile::TempDir,
    }

    async fn montar(provider: Arc<dyn Provider>, limites: HostLimits) -> Montaje {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(
            SqliteStore::open(dir.path().join("host.db").to_str().unwrap())
                .await
                .unwrap(),
        );
        let config = CodeLoomConfig {
            provider: ProviderConfig {
                kind: ProviderKind::OpenAiCompat,
                base_url: "http://localhost".into(),
                api_key_env: "SIN_USO".into(),
            },
            model: "m".into(),
            max_steps: 3,
            effort: None,
        };
        let registro = Arc::new(AgentRegistry::new(vec![Box::new(BuiltInSource)]));
        let bus = EventBus::new(64);
        let launcher = Arc::new(
            InProcessSubagentLauncher::new(
                Arc::new(RwLock::new(provider)),
                store.clone(),
                Arc::new(served_tools()),
                Arc::new(PermissionService::allow_all()),
                Arc::new(RwLock::new(config)),
                dir.path().to_path_buf(),
                Arc::new(LocalJobRegistry::default()),
                bus.clone(),
            )
            .with_registry(registro),
        );
        let host = LauncherHost::new(
            launcher.clone(),
            store.as_ref(),
            "/servidor",
            "default".into(),
            limites,
        )
        .await
        .unwrap()
        .with_bus(bus.clone());
        let otro_padre = store.create_session("/otro").await.unwrap();
        Montaje {
            host,
            launcher,
            otro_padre,
            bus,
            _dir: dir,
        }
    }

    fn limites() -> HostLimits {
        HostLimits {
            max_concurrent: 2,
            max_task: Duration::from_secs(60),
            max_result_bytes: 1024,
        }
    }

    fn id_de(vista: &TaskView) -> String {
        vista.json["id"].as_str().unwrap().to_string()
    }

    #[test]
    fn the_served_agent_can_only_view_glob_and_grep() {
        let mut nombres: Vec<String> = served_tools().specs().into_iter().map(|s| s.name).collect();
        nombres.sort();

        assert_eq!(nombres, ["glob", "grep", "view"]);
    }

    #[tokio::test]
    async fn a_task_runs_to_completion_and_returns_its_text() {
        let montaje = montar(
            Arc::new(ConTexto("hola desde el servidor".into())),
            limites(),
        )
        .await;

        let inicial = montaje.host.start("saluda".into()).await.unwrap();
        let fin = montaje
            .host
            .wait_terminal(&id_de(&inicial), Duration::from_secs(10))
            .await
            .unwrap();

        assert!(fin.terminal, "{:?}", fin.json);
        assert_eq!(fin.json["status"]["state"], "TASK_STATE_COMPLETED");
        assert_eq!(
            fin.json["artifacts"][0]["parts"][0]["text"],
            "hola desde el servidor"
        );
    }

    #[tokio::test]
    async fn every_state_change_of_an_incoming_task_is_announced_on_the_bus() {
        let montaje = montar(Arc::new(ConTexto("ok".into())), limites()).await;
        let mut eventos = montaje.bus.subscribe();

        let inicial = montaje.host.start("saluda".into()).await.unwrap();
        let id = id_de(&inicial);

        let mut estados = Vec::new();
        let limite = Instant::now() + Duration::from_secs(10);
        while Instant::now() < limite {
            let Ok(Ok(evento)) =
                tokio::time::timeout(Duration::from_millis(500), eventos.recv()).await
            else {
                continue;
            };
            if let LiveEvent::A2aServed { task, agent, state } = evento {
                assert_eq!(task, id);
                assert_eq!(agent, "default");
                let terminal = state == "TASK_STATE_COMPLETED";
                estados.push(state);
                if terminal {
                    break;
                }
            }
        }

        assert_eq!(
            estados.last().map(String::as_str),
            Some("TASK_STATE_COMPLETED")
        );
        let mut sin_repetir = estados.clone();
        sin_repetir.dedup();
        assert_eq!(
            estados, sin_repetir,
            "no debe avisar dos veces del mismo estado seguido"
        );
    }

    #[tokio::test]
    async fn the_returned_text_is_bounded() {
        let montaje = montar(
            Arc::new(ConTexto("x".repeat(5_000))),
            HostLimits {
                max_result_bytes: 100,
                ..limites()
            },
        )
        .await;

        let inicial = montaje.host.start("habla mucho".into()).await.unwrap();
        let fin = montaje
            .host
            .wait_terminal(&id_de(&inicial), Duration::from_secs(10))
            .await
            .unwrap();

        let texto = fin.json["artifacts"][0]["parts"][0]["text"]
            .as_str()
            .unwrap();
        assert!(texto.len() < 140, "{}", texto.len());
        assert!(texto.ends_with("[truncated]"));
    }

    #[tokio::test]
    async fn the_task_cap_refuses_the_extra_one_and_creates_nothing() {
        let montaje = montar(Arc::new(Colgado), limites()).await;
        montaje.host.start("uno".into()).await.unwrap();
        montaje.host.start("dos".into()).await.unwrap();

        let error = montaje.host.start("tres".into()).await.unwrap_err();

        assert_eq!(error, HostError::Busy);
        let hijos = montaje.launcher.list(montaje.host.parent()).await.unwrap();
        assert_eq!(hijos.len(), 2, "la tercera no debe crear tarea");
    }

    #[tokio::test]
    async fn a_finished_task_frees_its_slot() {
        let montaje = montar(
            Arc::new(ConTexto("ok".into())),
            HostLimits {
                max_concurrent: 1,
                ..limites()
            },
        )
        .await;
        let primera = montaje.host.start("a".into()).await.unwrap();
        montaje
            .host
            .wait_terminal(&id_de(&primera), Duration::from_secs(10))
            .await
            .unwrap();

        assert!(montaje.host.start("b".into()).await.is_ok());
    }

    #[tokio::test]
    async fn a_task_that_overruns_its_time_is_canceled() {
        let montaje = montar(
            Arc::new(Colgado),
            HostLimits {
                max_task: Duration::from_millis(300),
                ..limites()
            },
        )
        .await;

        let inicial = montaje.host.start("eterna".into()).await.unwrap();
        let fin = montaje
            .host
            .wait_terminal(&id_de(&inicial), Duration::from_secs(10))
            .await
            .unwrap();

        assert!(fin.terminal, "{:?}", fin.json);
        assert_eq!(fin.json["status"]["state"], "TASK_STATE_CANCELED");
    }

    #[tokio::test]
    async fn cancel_returns_the_task_canceled_and_a_second_cancel_is_not_cancelable() {
        let montaje = montar(Arc::new(Colgado), limites()).await;
        let inicial = montaje.host.start("larga".into()).await.unwrap();
        let id = id_de(&inicial);

        let cancelada = montaje.host.cancel(&id).await.unwrap();
        assert_eq!(cancelada.json["status"]["state"], "TASK_STATE_CANCELED");

        assert_eq!(
            montaje.host.cancel(&id).await.unwrap_err(),
            HostError::NotCancelable
        );
    }

    #[tokio::test]
    async fn another_sessions_task_and_garbage_ids_are_just_not_found() {
        let montaje = montar(Arc::new(ConTexto("ok".into())), limites()).await;
        // Una tarea de otra sesión padre, en el mismo almacén.
        let (ajeno, _) = montaje
            .launcher
            .start(
                montaje.otro_padre,
                "ajena".into(),
                "hola".into(),
                Some("default".into()),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        for id in [
            ajeno.0.to_string(),
            "999999".into(),
            "abc".into(),
            "-1".into(),
            String::new(),
        ] {
            assert_eq!(
                montaje.host.get(&id).await.unwrap_err(),
                HostError::NotFound,
                "{id:?}"
            );
            assert_eq!(
                montaje.host.cancel(&id).await.unwrap_err(),
                HostError::NotFound,
                "{id:?}"
            );
        }
    }

    #[tokio::test]
    async fn waiting_returns_the_current_state_when_time_runs_out() {
        let montaje = montar(Arc::new(Colgado), limites()).await;
        let inicial = montaje.host.start("larga".into()).await.unwrap();

        let vista = montaje
            .host
            .wait_terminal(&id_de(&inicial), Duration::from_millis(200))
            .await
            .unwrap();

        assert!(!vista.terminal);
        assert_eq!(vista.json["status"]["state"], "TASK_STATE_WORKING");
    }
}
