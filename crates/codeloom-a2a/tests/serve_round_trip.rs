//! Servidor A2A propio de extremo a extremo (A8b): configuración → arranque → HTTP real por bucle
//! local → cliente de A8a.
//!
//! Cabe señalar que no hay red exterior: el proveedor del modelo es de prueba y el único socket es
//! el del bucle local, en un puerto que asigna el sistema.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use codeloom_a2a::client::JsonRpcClient;
use codeloom_a2a::security::{AuthPlan, CardSecurity};
use codeloom_a2a::serve::config::{ServeAuthConfig, ServeAuthKind, ServeConfig};
use codeloom_a2a::serve::launch::{start, ServeDeps};
use codeloom_core::a2a::{Message, PartContent};
use codeloom_core::config::{CodeLoomConfig, ProviderConfig, ProviderKind};
use codeloom_core::error::Result as CoreResult;
use codeloom_core::event::EventBus;
use codeloom_core::message::FinishReason;
use codeloom_core::provider::{
    Capabilities, Provider, ProviderRequest, ProviderStream, StreamChunk,
};
use codeloom_core::registry::{AgentRegistry, BuiltInSource};
use codeloom_core::remote_task::{RemoteReply, RemoteTask, RemoteTaskClient, RemoteTaskError};
use codeloom_core::session::sqlite::SqliteStore;
use codeloom_core::task::TaskState;
use futures::stream;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

const TOKEN: &str = "token-de-ida-y-vuelta-123";

/// Responde con un texto fijo.
struct ConTexto(&'static str);

/// No responde nunca: la tarea queda en `working` hasta que la cancelan.
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
        Ok(Box::pin(stream::iter(vec![
            StreamChunk::Started,
            StreamChunk::TextDelta(self.0.to_string()),
            StreamChunk::Done(FinishReason::Stop),
        ])))
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

fn config_servidor() -> ServeConfig {
    ServeConfig {
        listen: "127.0.0.1:0".into(),
        auth: Some(ServeAuthConfig {
            scheme: ServeAuthKind::Bearer,
            name: None,
            env: "TOKEN_LOOM_PRUEBA".into(),
        }),
        ..ServeConfig::default()
    }
}

fn entorno(nombre: &str) -> Option<String> {
    (nombre == "TOKEN_LOOM_PRUEBA").then(|| TOKEN.to_string())
}

struct Servidor {
    addr: SocketAddr,
    apagado: CancellationToken,
    _dir: tempfile::TempDir,
}

impl Drop for Servidor {
    fn drop(&mut self) {
        self.apagado.cancel();
    }
}

async fn deps(provider: Arc<dyn Provider>, dir: &tempfile::TempDir) -> ServeDeps {
    let store = SqliteStore::open(dir.path().join("serve.db").to_str().unwrap())
        .await
        .unwrap();
    ServeDeps {
        registry: Arc::new(AgentRegistry::new(vec![Box::new(BuiltInSource)])),
        provider,
        store: Arc::new(store),
        config: CodeLoomConfig {
            provider: ProviderConfig {
                kind: ProviderKind::OpenAiCompat,
                base_url: "http://localhost".into(),
                api_key_env: "SIN_USO".into(),
            },
            model: "m".into(),
            max_steps: 3,
            effort: None,
        },
        cwd: dir.path().to_path_buf(),
        bus: EventBus::new(64),
    }
}

async fn arrancar(provider: Arc<dyn Provider>, config: ServeConfig) -> Servidor {
    let dir = tempfile::tempdir().unwrap();
    let apagado = CancellationToken::new();
    let addr = start(
        &config,
        deps(provider, &dir).await,
        &entorno,
        apagado.clone(),
    )
    .await
    .expect("el servidor de prueba debía arrancar");
    Servidor {
        addr,
        apagado,
        _dir: dir,
    }
}

/// Cliente de A8a que se autentica como lo haría contra una tarjeta real: lee la seguridad de la
/// tarjeta servida y la cumple con el secreto de `token`.
async fn cliente(servidor: &Servidor, token: &str) -> JsonRpcClient {
    let tarjeta: Value = reqwest::get(format!(
        "http://{}/.well-known/agent-card.json",
        servidor.addr
    ))
    .await
    .unwrap()
    .json()
    .await
    .unwrap();
    let seguridad = CardSecurity::from_card_json(&tarjeta).to_json();
    let credenciales: BTreeMap<String, String> =
        [("codeloom".to_string(), "VAR_DEL_CLIENTE".to_string())].into();
    let token = token.to_string();
    let plan = AuthPlan::for_card(seguridad.as_deref(), true, &credenciales, &move |nombre| {
        (nombre == "VAR_DEL_CLIENTE").then(|| token.clone())
    })
    .expect("el cliente debía poder cumplir la seguridad de la tarjeta servida");
    JsonRpcClient::new(
        reqwest::Client::new(),
        format!("http://{}/a2a", servidor.addr),
        None,
        64 * 1024,
    )
    .with_auth(plan)
}

async fn hasta_estado(cliente: &JsonRpcClient, id: &str, estado: TaskState) -> RemoteTask {
    let limite = Instant::now() + Duration::from_secs(15);
    loop {
        let tarea = cliente.get(id).await.expect("la tarea debía existir");
        if tarea.status.state == estado {
            return tarea;
        }
        assert!(
            Instant::now() < limite,
            "la tarea quedó en {:?}, no en {estado:?}",
            tarea.status.state
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn tarea(respuesta: RemoteReply) -> RemoteTask {
    match respuesta {
        RemoteReply::Task(tarea) => tarea,
        otra => panic!("se esperaba una tarea y llegó {otra:?}"),
    }
}

#[tokio::test]
async fn our_own_client_runs_a_task_on_our_own_server_end_to_end() {
    let servidor = arrancar(
        Arc::new(ConTexto("hola desde el servidor")),
        config_servidor(),
    )
    .await;
    let cliente = cliente(&servidor, TOKEN).await;

    let inicial = tarea(cliente.send(Message::user_text("saluda")).await.unwrap());
    let fin = hasta_estado(&cliente, &inicial.id, TaskState::Completed).await;

    let texto: Vec<&str> = fin
        .artifacts
        .iter()
        .flat_map(|a| a.parts.iter())
        .filter_map(|p| match &p.content {
            PartContent::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texto, ["hola desde el servidor"]);
}

#[tokio::test]
async fn the_served_card_is_public_but_the_rpc_endpoint_is_not() {
    let servidor = arrancar(Arc::new(ConTexto("x")), config_servidor()).await;
    let http = reqwest::Client::new();

    let tarjeta = http
        .get(format!(
            "http://{}/.well-known/agent-card.json",
            servidor.addr
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(tarjeta.status(), 200);
    let tarjeta: Value = tarjeta.json().await.unwrap();
    assert_eq!(tarjeta["name"], "default");
    assert!(tarjeta["securitySchemes"].is_object());
    assert!(!tarjeta.to_string().contains(TOKEN));

    let cuerpo = json!({"jsonrpc": "2.0", "id": 1, "method": "GetTask", "params": {"id": "1"}});
    let url = format!("http://{}/a2a", servidor.addr);
    let sin = http.post(&url).json(&cuerpo).send().await.unwrap();
    assert_eq!(sin.status(), 401);
    let mala = http
        .post(&url)
        .bearer_auth("otro-secreto")
        .json(&cuerpo)
        .send()
        .await
        .unwrap();
    assert_eq!(mala.status(), 401);
}

#[tokio::test]
async fn a_client_with_the_wrong_secret_is_rejected_without_echoing_either_secret() {
    let servidor = arrancar(Arc::new(ConTexto("x")), config_servidor()).await;
    let cliente = cliente(&servidor, "secreto-equivocado-456").await;

    let error = cliente
        .send(Message::user_text("hola"))
        .await
        .expect_err("con otro secreto no debía pasar");

    assert!(matches!(error, RemoteTaskError::AuthFailed(_)), "{error:?}");
    let texto = error.to_string();
    assert!(!texto.contains(TOKEN) && !texto.contains("secreto-equivocado-456"));
}

#[tokio::test]
async fn a_running_task_can_be_canceled_by_the_client() {
    let servidor = arrancar(Arc::new(Colgado), config_servidor()).await;
    let cliente = cliente(&servidor, TOKEN).await;
    let inicial = tarea(cliente.send(Message::user_text("larga")).await.unwrap());

    let cancelada = cliente.cancel(&inicial.id).await.unwrap();

    assert_eq!(cancelada.status.state, TaskState::Canceled);
}

#[tokio::test]
async fn an_unknown_task_is_reported_as_not_found() {
    let servidor = arrancar(Arc::new(ConTexto("x")), config_servidor()).await;
    let cliente = cliente(&servidor, TOKEN).await;

    let error = cliente.get("424242").await.unwrap_err();

    assert!(matches!(error, RemoteTaskError::NotFound), "{error:?}");
}

#[tokio::test]
async fn the_server_refuses_to_start_without_authentication_or_its_secret() {
    let dir = tempfile::tempdir().unwrap();

    let sin_auth = ServeConfig {
        auth: None,
        ..config_servidor()
    };
    let error = start(
        &sin_auth,
        deps(Arc::new(Colgado), &dir).await,
        &entorno,
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("auth"), "{error}");

    let error = start(
        &config_servidor(),
        deps(Arc::new(Colgado), &dir).await,
        &|_| None,
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("TOKEN_LOOM_PRUEBA"), "{error}");
}

#[tokio::test]
async fn the_server_refuses_a_public_address_and_an_unknown_served_agent() {
    let dir = tempfile::tempdir().unwrap();

    let publica = ServeConfig {
        listen: "0.0.0.0:7077".into(),
        ..config_servidor()
    };
    let error = start(
        &publica,
        deps(Arc::new(Colgado), &dir).await,
        &entorno,
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("bucle local"), "{error}");

    let inexistente = ServeConfig {
        agent: "no-existe".into(),
        ..config_servidor()
    };
    let error = start(
        &inexistente,
        deps(Arc::new(Colgado), &dir).await,
        &entorno,
        CancellationToken::new(),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("no-existe"), "{error}");
}
