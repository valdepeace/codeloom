//! Consumo de un agente remoto de extremo a extremo (A8a): perfil `.md` → tarjeta descargada →
//! aprobación → lanzador → espejo → servidor simulado con `wiremock`.
//!
//! Cabe señalar que no hay red real: el «remoto» es un servidor de bucle local que habla JSON-RPC
//! de A2A y devuelve a cada petición el `id` que trae.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use codeloom_a2a::client::A2aClientFactory;
use codeloom_a2a::config::{A2aConfig, A2aRole};
use codeloom_a2a::remote::http_client;
use codeloom_a2a::sync::refresh_remote_cards;
use codeloom_core::config::{CodeLoomConfig, ProviderConfig, ProviderKind};
use codeloom_core::error::Result as CoreResult;
use codeloom_core::event::EventBus;
use codeloom_core::job::LocalJobRegistry;
use codeloom_core::permission::PermissionService;
use codeloom_core::provider::{Capabilities, Provider, ProviderRequest, ProviderStream};
use codeloom_core::registry::{
    AgentRegistry, Approval, BuiltInSource, FileSource, ProfileLevel, TrustStore,
};
use codeloom_core::session::sqlite::SqliteStore;
use codeloom_core::session::{SessionId, SessionStore};
use codeloom_core::subagent::{InProcessSubagentLauncher, SubagentLauncher};
use codeloom_core::task::TaskState;
use codeloom_core::tools::ToolRegistry;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const TARJETA: &str = include_str!("fixtures/agent-card.sample.json");

/// El proveedor no se usa: el agente es remoto.
struct ProveedorInerte;

#[async_trait]
impl Provider for ProveedorInerte {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            tool_use: false,
            reasoning: false,
            streaming: false,
        }
    }
    async fn stream(
        &self,
        _req: ProviderRequest,
        _cancel: CancellationToken,
    ) -> CoreResult<ProviderStream> {
        unreachable!("un agente remoto no llama al proveedor local")
    }
}

/// Remoto simulado: contesta a cada método según un guion y devuelve el `id` de la petición.
struct RemotoSimulado {
    consultas: AtomicUsize,
    /// A partir de esta consulta (1-based) la tarea figura como completada; 0 = nunca.
    completa_en: usize,
}

impl Respond for RemotoSimulado {
    fn respond(&self, peticion: &Request) -> ResponseTemplate {
        let cuerpo: Value = serde_json::from_slice(&peticion.body).unwrap();
        let id = cuerpo["id"].clone();
        let resultado = match cuerpo["method"].as_str().unwrap() {
            "SendMessage" => json!({"task": {"id": "t-remota", "contextId": "c",
                "status": {"state": "TASK_STATE_WORKING"}}}),
            "GetTask" => {
                let n = self.consultas.fetch_add(1, Ordering::SeqCst) + 1;
                if self.completa_en != 0 && n >= self.completa_en {
                    json!({"id": "t-remota", "status": {"state": "TASK_STATE_COMPLETED"},
                        "artifacts": [{"artifactId": "a", "parts": [{"text": "hecho en remoto"}]}]})
                } else {
                    json!({"id": "t-remota", "status": {"state": "TASK_STATE_WORKING"}})
                }
            }
            "CancelTask" => {
                json!({"id": "t-remota", "status": {"state": "TASK_STATE_CANCELED"}})
            }
            otro => panic!("método inesperado: {otro}"),
        };
        ResponseTemplate::new(200)
            .set_body_json(json!({"jsonrpc": "2.0", "id": id, "result": resultado}))
    }
}

async fn servidor(completa_en: usize) -> MockServer {
    let servidor = MockServer::start().await;
    let mut tarjeta: Value = serde_json::from_str(TARJETA).unwrap();
    for interfaz in tarjeta["supportedInterfaces"].as_array_mut().unwrap() {
        interfaz["url"] = json!(format!("{}/a2a/v1", servidor.uri()));
    }
    // Esta tarjeta de ejemplo declara requisitos de seguridad; para consumir sin autenticación
    // (lo único soportado hoy) se quitan.
    tarjeta
        .as_object_mut()
        .unwrap()
        .remove("securityRequirements");
    Mock::given(method("GET"))
        .and(path("/.well-known/agent-card.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tarjeta))
        .mount(&servidor)
        .await;
    Mock::given(method("POST"))
        .and(path("/a2a/v1"))
        .and(header("A2A-Version", "1.0"))
        .respond_with(RemotoSimulado {
            consultas: AtomicUsize::new(0),
            completa_en,
        })
        .mount(&servidor)
        .await;
    servidor
}

struct Montaje {
    lanzador: InProcessSubagentLauncher,
    registro: Arc<AgentRegistry>,
    store: Arc<SqliteStore>,
    parent: SessionId,
    bus: EventBus,
    servidor: MockServer,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

async fn montar(completa_en: usize) -> Montaje {
    let servidor = servidor(completa_en).await;
    let dir = tempfile::tempdir().unwrap();
    let datos = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("lejano.md"),
        format!(
            "---\nname: lejano\ndescription: Alta manual\ntransport: a2a-remote\na2a:\n  endpoint: {}\n---\n",
            servidor.uri()
        ),
    )
    .unwrap();
    let registro = Arc::new(
        AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(
                dir.path().to_path_buf(),
                ProfileLevel::Global,
            )),
        ])
        .with_trust(
            TrustStore::open(datos.path().to_path_buf()).unwrap(),
            dir.path().to_path_buf(),
            false,
        ),
    );
    let config_a2a = A2aConfig {
        role: A2aRole::Consume,
        request_timeout_secs: 2,
        ..A2aConfig::default()
    };
    let bus = EventBus::new(256);
    let cliente = http_client(&config_a2a).unwrap().unwrap();
    refresh_remote_cards(&registro, &bus, &cliente, &config_a2a).await;

    let store = Arc::new(
        SqliteStore::open(dir.path().join("e2e.db").to_str().unwrap())
            .await
            .unwrap(),
    );
    let parent = store.create_session("/project").await.unwrap();
    let provider: Arc<dyn Provider> = Arc::new(ProveedorInerte);
    let config = CodeLoomConfig {
        provider: ProviderConfig {
            kind: ProviderKind::OpenAiCompat,
            base_url: "http://localhost".into(),
            api_key_env: "SIN_USO".into(),
        },
        model: "m".into(),
        max_steps: 5,
        effort: None,
    };
    let fabrica = A2aClientFactory::from_config(&config_a2a).unwrap().unwrap();
    let lanzador = InProcessSubagentLauncher::new(
        Arc::new(RwLock::new(provider)),
        store.clone(),
        Arc::new(ToolRegistry::new()),
        Arc::new(PermissionService::allow_all()),
        Arc::new(RwLock::new(config)),
        dir.path().to_path_buf(),
        Arc::new(LocalJobRegistry::default()),
        bus.clone(),
    )
    .with_registry(registro.clone())
    .with_remote_clients(Arc::new(fabrica))
    .with_remote_timing(Duration::from_millis(10), Duration::from_secs(30));
    Montaje {
        lanzador,
        registro,
        store,
        parent,
        bus,
        servidor,
        _dirs: (dir, datos),
    }
}

async fn espera(montaje: &Montaje, hijo: SessionId, estado: TaskState) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let actual = montaje
                .store
                .list_subagents(montaje.parent)
                .await
                .unwrap()
                .into_iter()
                .find(|d| d.child == hijo)
                .map(|d| d.status.state);
            if actual == Some(estado) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("el hijo debería llegar a {estado:?}"));
}

async fn metodos_recibidos(servidor: &MockServer) -> Vec<String> {
    servidor
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.method.as_str() == "POST")
        .map(|r| {
            let cuerpo: Value = serde_json::from_slice(&r.body).unwrap();
            cuerpo["method"].as_str().unwrap().to_string()
        })
        .collect()
}

#[tokio::test]
async fn a_remote_agent_is_pending_then_approved_then_runs_to_completion() {
    let montaje = montar(3).await;
    assert_eq!(
        montaje.registro.snapshot().approval("lejano"),
        Some(Approval::Pending)
    );
    // Sin aprobar, no se le manda nada.
    let rechazo = montaje
        .lanzador
        .start(
            montaje.parent,
            "tarea".into(),
            "hazlo".into(),
            Some("lejano".into()),
            CancellationToken::new(),
        )
        .await
        .unwrap_err();
    assert!(
        rechazo.to_string().contains("pendiente de aprobación"),
        "{rechazo}"
    );
    assert!(metodos_recibidos(&montaje.servidor).await.is_empty());

    montaje.registro.approve("lejano").unwrap();
    let (hijo, _) = montaje
        .lanzador
        .start(
            montaje.parent,
            "tarea".into(),
            "hazlo".into(),
            Some("lejano".into()),
            CancellationToken::new(),
        )
        .await
        .unwrap();

    espera(&montaje, hijo, TaskState::Completed).await;
    let descriptor = montaje
        .store
        .list_subagents(montaje.parent)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.child == hijo)
        .unwrap();
    assert_eq!(descriptor.artifacts[0].text(), "hecho en remoto");
    assert_eq!(descriptor.agent.as_deref(), Some("lejano"));
    let metodos = metodos_recibidos(&montaje.servidor).await;
    assert_eq!(metodos[0], "SendMessage");
    assert!(metodos[1..].iter().all(|m| m == "GetTask"), "{metodos:?}");
    drop(montaje.bus);
}

#[tokio::test]
async fn cancelling_the_mirror_sends_cancel_task_to_the_remote() {
    let montaje = montar(0).await;
    montaje.registro.approve("lejano").unwrap();
    let (hijo, _) = montaje
        .lanzador
        .start(
            montaje.parent,
            "tarea".into(),
            "hazlo".into(),
            Some("lejano".into()),
            CancellationToken::new(),
        )
        .await
        .unwrap();
    espera(&montaje, hijo, TaskState::Working).await;

    let devuelto = montaje.lanzador.cancel(montaje.parent, hijo).await.unwrap();

    assert_eq!(devuelto.status.state, TaskState::Canceled);
    assert!(metodos_recibidos(&montaje.servidor)
        .await
        .contains(&"CancelTask".to_string()));
}

#[tokio::test]
async fn with_the_role_off_there_is_no_client_factory() {
    assert!(A2aClientFactory::from_config(&A2aConfig::default())
        .unwrap()
        .is_none());
}
