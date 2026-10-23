//! Cliente JSON-RPC de un agente remoto (A8a, T4): `SendMessage`, `GetTask` y `CancelTask`.
//!
//! Cabe señalar que habla con la interfaz que la tarjeta aprobada declaró, envía siempre
//! `A2A-Version: 1.0`, acota el cuerpo de cada respuesta y no afirma nada que el remoto no diga:
//! un error de A2A se traduce a su equivalente y cualquier otra cosa es un error visible.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use codeloom_core::a2a::{Artifact, Message};
use codeloom_core::registry::RemoteSpec;
use codeloom_core::remote_task::{
    RemoteClientFactory, RemoteReply, RemoteTask, RemoteTaskClient, RemoteTaskError,
};
use codeloom_core::task::TaskStatus;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::card::VERSION_SOPORTADA;
use crate::config::A2aConfig;
use crate::remote::{http_client, leer_acotado, mapear, RemoteError};
use crate::wire::{state_from_wire, timestamp_from_wire, WireMessage, WireTask};

/// Cota de los textos de error de un tercero que se copian a un mensaje nuestro.
const MAX_TEXTO_ERROR: usize = 300;

// Códigos de error de A2A en JSON-RPC (§5.4).
const TASK_NOT_FOUND: i64 = -32001;
const TASK_NOT_CANCELABLE: i64 = -32002;
const UNSUPPORTED_OPERATION: i64 = -32004;

#[derive(Debug, Deserialize)]
struct RespuestaRpc {
    #[serde(default)]
    id: Value,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<ErrorRpc>,
}

#[derive(Debug, Deserialize)]
struct ErrorRpc {
    code: i64,
    #[serde(default)]
    message: String,
}

/// `SendMessageResponse`: exactamente una de `task` y `message`.
#[derive(Debug, Deserialize)]
struct RespuestaDeEnvio {
    #[serde(default)]
    task: Option<WireTask>,
    #[serde(default)]
    message: Option<WireMessage>,
}

/// Cliente de un remoto concreto.
pub struct JsonRpcClient {
    http: Client,
    url: String,
    tenant: Option<String>,
    max_result_bytes: usize,
    siguiente_id: AtomicU64,
}

impl JsonRpcClient {
    /// `url` es la de la interfaz JSON-RPC elegida en la tarjeta y `tenant` el que esa interfaz
    /// declara (se envía tal cual en cada petición, §8.3.2).
    pub fn new(
        http: Client,
        url: impl Into<String>,
        tenant: Option<String>,
        max_result_bytes: usize,
    ) -> Self {
        JsonRpcClient {
            http,
            url: url.into(),
            tenant,
            max_result_bytes,
            siguiente_id: AtomicU64::new(1),
        }
    }

    fn con_tenant(&self, mut params: Value) -> Value {
        if let Some(tenant) = &self.tenant {
            params["tenant"] = json!(tenant);
        }
        params
    }

    async fn llamar(&self, metodo: &str, params: Value) -> Result<Value, RemoteTaskError> {
        let id = self.siguiente_id.fetch_add(1, Ordering::Relaxed);
        let cuerpo = json!({ "jsonrpc": "2.0", "id": id, "method": metodo, "params": params });
        let respuesta = self
            .http
            .post(&self.url)
            .header("A2A-Version", VERSION_SOPORTADA)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&cuerpo)
            .send()
            .await
            .map_err(|e| traducir_fallo(mapear(e)))?;
        let estado = respuesta.status();
        let bytes = leer_acotado(respuesta, self.max_result_bytes)
            .await
            .map_err(traducir_fallo)?;

        let rpc: RespuestaRpc = match serde_json::from_slice(&bytes) {
            Ok(rpc) => rpc,
            Err(_) if !estado.is_success() => {
                return Err(RemoteTaskError::Other(format!(
                    "HTTP status {}",
                    estado.as_u16()
                )))
            }
            Err(e) => {
                return Err(RemoteTaskError::Other(format!(
                    "response is not JSON-RPC: {e}"
                )))
            }
        };
        if rpc.id != json!(id) {
            return Err(RemoteTaskError::Other(
                "response id does not match the request".into(),
            ));
        }
        match (rpc.result, rpc.error) {
            (Some(resultado), None) => Ok(resultado),
            (None, Some(error)) => Err(traducir_error_rpc(error)),
            _ => Err(RemoteTaskError::Other(
                "response must carry exactly one of result and error".into(),
            )),
        }
    }
}

/// Fábrica de clientes de los remotos del registro: la que se entrega al lanzador. Solo existe con
/// el rol `consume` (o `both`); con A2A apagado no se construye y los remotos no se ejecutan.
pub struct A2aClientFactory {
    http: Client,
    max_result_bytes: usize,
}

impl A2aClientFactory {
    /// `None` si el rol no es de consumo: no se construye ningún cliente HTTP.
    pub fn from_config(config: &A2aConfig) -> Result<Option<Self>, RemoteError> {
        Ok(http_client(config)?.map(|http| A2aClientFactory {
            http,
            max_result_bytes: config.max_result_bytes,
        }))
    }
}

impl RemoteClientFactory for A2aClientFactory {
    /// Cliente de la interfaz que la tarjeta descargada declaró; sin tarjeta, ninguno.
    fn client_for(&self, _name: &str, spec: &RemoteSpec) -> Option<Arc<dyn RemoteTaskClient>> {
        let descargada = spec.fetched.as_ref()?;
        Some(Arc::new(JsonRpcClient::new(
            self.http.clone(),
            descargada.jsonrpc_url.clone(),
            descargada.tenant.clone(),
            self.max_result_bytes,
        )))
    }
}

fn acotar(texto: &str) -> String {
    if texto.len() <= MAX_TEXTO_ERROR {
        return texto.to_string();
    }
    let mut fin = MAX_TEXTO_ERROR;
    while !texto.is_char_boundary(fin) {
        fin -= 1;
    }
    format!("{}…", &texto[..fin])
}

fn traducir_fallo(error: RemoteError) -> RemoteTaskError {
    match error {
        RemoteError::Timeout => RemoteTaskError::Timeout,
        otro => RemoteTaskError::Other(otro.to_string()),
    }
}

fn traducir_error_rpc(error: ErrorRpc) -> RemoteTaskError {
    match error.code {
        TASK_NOT_FOUND => RemoteTaskError::NotFound,
        TASK_NOT_CANCELABLE => RemoteTaskError::NotCancelable,
        UNSUPPORTED_OPERATION => RemoteTaskError::Unsupported(acotar(&error.message)),
        codigo => RemoteTaskError::Other(format!("code {codigo}: {}", acotar(&error.message))),
    }
}

fn invalida(error: impl std::fmt::Display) -> RemoteTaskError {
    RemoteTaskError::Other(format!("invalid response: {error}"))
}

/// Convierte una tarea del cable a la vista del núcleo. El estado ha de ser uno de los ocho.
fn tarea_desde_wire(wire: WireTask) -> Result<RemoteTask, RemoteTaskError> {
    let estado = state_from_wire(&wire.status.state).map_err(invalida)?;
    let mensaje = wire
        .status
        .message
        .map(Message::try_from)
        .transpose()
        .map_err(invalida)?;
    // Sin marca en el cable, la del momento de recibirla (la que pone `TaskStatus::new`).
    let mut status = TaskStatus::new(estado);
    status.message = mensaje;
    if let Some(texto) = &wire.status.timestamp {
        status.timestamp_ms = timestamp_from_wire(texto).map_err(invalida)?;
    }
    let artifacts = wire
        .artifacts
        .into_iter()
        .map(Artifact::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalida)?;
    Ok(RemoteTask {
        id: wire.id,
        context_id: wire.context_id,
        status,
        artifacts,
    })
}

fn tarea_de_resultado(resultado: Value) -> Result<RemoteTask, RemoteTaskError> {
    let wire: WireTask = serde_json::from_value(resultado).map_err(invalida)?;
    tarea_desde_wire(wire)
}

#[async_trait]
impl RemoteTaskClient for JsonRpcClient {
    async fn send(&self, message: Message) -> Result<RemoteReply, RemoteTaskError> {
        // Sin `returnImmediately` el remoto bloquea hasta un estado terminal o interrumpido (§3.2.2).
        let params = self.con_tenant(json!({
            "message": WireMessage::from(&message),
            "configuration": { "returnImmediately": true },
        }));
        let resultado = self.llamar("SendMessage", params).await?;
        let respuesta: RespuestaDeEnvio = serde_json::from_value(resultado).map_err(invalida)?;
        match (respuesta.task, respuesta.message) {
            (Some(tarea), None) => Ok(RemoteReply::Task(tarea_desde_wire(tarea)?)),
            (None, Some(mensaje)) => Ok(RemoteReply::Message(
                Message::try_from(mensaje).map_err(invalida)?,
            )),
            _ => Err(RemoteTaskError::Other(
                "SendMessage must answer with exactly one of task and message".into(),
            )),
        }
    }

    async fn get(&self, id: &str) -> Result<RemoteTask, RemoteTaskError> {
        // `historyLength: 0`: no se pide historial, que no se usa y puede ser enorme.
        let params = self.con_tenant(json!({ "id": id, "historyLength": 0 }));
        tarea_de_resultado(self.llamar("GetTask", params).await?)
    }

    async fn cancel(&self, id: &str) -> Result<RemoteTask, RemoteTaskError> {
        let params = self.con_tenant(json!({ "id": id }));
        tarea_de_resultado(self.llamar("CancelTask", params).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{A2aConfig, A2aRole};
    use crate::remote::http_client;
    use codeloom_core::task::TaskState;
    use std::time::Duration;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const ENVIO: &str = include_str!("../tests/fixtures/jsonrpc-send-message.response.json");
    const NO_ENCONTRADA: &str = include_str!("../tests/fixtures/jsonrpc-error-task-not-found.json");

    fn http(segundos: u64) -> Client {
        http_client(&A2aConfig {
            role: A2aRole::Consume,
            request_timeout_secs: segundos,
            ..A2aConfig::default()
        })
        .unwrap()
        .unwrap()
    }

    fn cliente(servidor: &MockServer, tenant: Option<&str>) -> JsonRpcClient {
        JsonRpcClient::new(
            http(2),
            format!("{}/a2a/v1", servidor.uri()),
            tenant.map(str::to_string),
            64 * 1024,
        )
    }

    /// Respuesta JSON-RPC con el `id` de la petición 1 (el primero que emite cada cliente).
    fn con_id_1(mut valor: Value) -> Value {
        valor["id"] = json!(1);
        valor
    }

    async fn montar(servidor: &MockServer, metodo: &str, respuesta: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(header("A2A-Version", "1.0"))
            .and(body_partial_json(
                json!({ "jsonrpc": "2.0", "method": metodo }),
            ))
            .respond_with(respuesta)
            .mount(servidor)
            .await;
    }

    #[tokio::test]
    async fn send_returns_the_task_of_the_spec_example_and_asks_not_to_block() {
        let servidor = MockServer::start().await;
        let respuesta: Value = serde_json::from_str(ENVIO).unwrap();
        montar(
            &servidor,
            "SendMessage",
            ResponseTemplate::new(200).set_body_json(con_id_1(respuesta)),
        )
        .await;

        let respuesta = cliente(&servidor, None)
            .send(Message::user_text("What is the weather today?"))
            .await
            .unwrap();

        match respuesta {
            RemoteReply::Task(tarea) => {
                assert_eq!(tarea.id, "task-uuid");
                assert_eq!(tarea.context_id.as_deref(), Some("context-uuid"));
                assert_eq!(tarea.status.state, TaskState::Completed);
                assert_eq!(
                    tarea.artifacts[0].text(),
                    "Today will be sunny with a high of 75°F"
                );
            }
            otro => panic!("se esperaba una tarea, y salió {otro:?}"),
        }
        let recibidas = servidor.received_requests().await.unwrap();
        let cuerpo: Value = serde_json::from_slice(&recibidas[0].body).unwrap();
        assert_eq!(cuerpo["params"]["configuration"]["returnImmediately"], true);
        assert_eq!(cuerpo["params"]["message"]["role"], "ROLE_USER");
        assert_eq!(
            cuerpo["params"]["message"]["parts"][0]["text"],
            "What is the weather today?"
        );
        assert!(cuerpo["params"].get("tenant").is_none());
    }

    #[tokio::test]
    async fn the_tenant_of_the_selected_interface_travels_in_every_request() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "GetTask",
            ResponseTemplate::new(200).set_body_json(con_id_1(
                json!({"jsonrpc":"2.0","result":{"id":"t","status":{"state":"TASK_STATE_WORKING"}}}),
            )),
        )
        .await;

        cliente(&servidor, Some("equipo-a")).get("t").await.unwrap();

        let recibidas = servidor.received_requests().await.unwrap();
        let cuerpo: Value = serde_json::from_slice(&recibidas[0].body).unwrap();
        assert_eq!(cuerpo["params"]["tenant"], "equipo-a");
        assert_eq!(cuerpo["params"]["id"], "t");
        assert_eq!(cuerpo["params"]["historyLength"], 0);
    }

    #[tokio::test]
    async fn send_can_be_answered_with_a_message_instead_of_a_task() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "SendMessage",
            ResponseTemplate::new(200).set_body_json(con_id_1(json!({
                "jsonrpc": "2.0",
                "result": {"message": {"messageId": "m", "role": "ROLE_AGENT", "parts": [{"text": "hola"}]}}
            }))),
        )
        .await;

        let respuesta = cliente(&servidor, None)
            .send(Message::user_text("hola"))
            .await
            .unwrap();

        match respuesta {
            RemoteReply::Message(mensaje) => assert_eq!(mensaje.text(), "hola"),
            otro => panic!("se esperaba un mensaje, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_reports_what_the_remote_says_and_not_what_was_asked() {
        let servidor = MockServer::start().await;
        // El remoto contesta a la cancelación con la tarea todavía trabajando.
        montar(
            &servidor,
            "CancelTask",
            ResponseTemplate::new(200).set_body_json(con_id_1(
                json!({"jsonrpc":"2.0","result":{"id":"t","status":{"state":"TASK_STATE_WORKING"}}}),
            )),
        )
        .await;

        let tarea = cliente(&servidor, None).cancel("t").await.unwrap();

        assert_eq!(tarea.status.state, TaskState::Working);
    }

    #[tokio::test]
    async fn the_a2a_error_codes_are_translated() {
        for (codigo, esperado) in [
            (-32001, RemoteTaskError::NotFound),
            (-32002, RemoteTaskError::NotCancelable),
            (-32004, RemoteTaskError::Unsupported("ya terminada".into())),
            (
                -32009,
                RemoteTaskError::Other("code -32009: versión".into()),
            ),
        ] {
            let servidor = MockServer::start().await;
            let mensaje = match codigo {
                -32004 => "ya terminada",
                -32009 => "versión",
                _ => "x",
            };
            montar(
                &servidor,
                "GetTask",
                ResponseTemplate::new(200).set_body_json(
                    json!({"jsonrpc":"2.0","id":1,"error":{"code":codigo,"message":mensaje}}),
                ),
            )
            .await;

            let resultado = cliente(&servidor, None).get("t").await;

            assert_eq!(resultado, Err(esperado), "código {codigo}");
        }
    }

    #[tokio::test]
    async fn the_spec_task_not_found_example_is_translated() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "GetTask",
            ResponseTemplate::new(200).set_body_raw(NO_ENCONTRADA, "application/json"),
        )
        .await;

        let resultado = cliente(&servidor, None).get("nonexistent-task-id").await;

        assert_eq!(resultado, Err(RemoteTaskError::NotFound));
    }

    #[tokio::test]
    async fn a_response_with_the_wrong_id_or_with_both_result_and_error_is_rejected() {
        for cuerpo in [
            json!({"jsonrpc":"2.0","id":99,"result":{"id":"t","status":{"state":"TASK_STATE_WORKING"}}}),
            json!({"jsonrpc":"2.0","id":1,"result":{"id":"t","status":{"state":"TASK_STATE_WORKING"}},"error":{"code":-1,"message":"x"}}),
            json!({"jsonrpc":"2.0","id":1}),
        ] {
            let servidor = MockServer::start().await;
            montar(
                &servidor,
                "GetTask",
                ResponseTemplate::new(200).set_body_json(cuerpo.clone()),
            )
            .await;

            let resultado = cliente(&servidor, None).get("t").await;

            assert!(
                matches!(resultado, Err(RemoteTaskError::Other(_))),
                "{cuerpo}: {resultado:?}"
            );
        }
    }

    #[tokio::test]
    async fn an_unknown_task_state_is_an_error_and_not_a_guess() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "GetTask",
            ResponseTemplate::new(200).set_body_json(con_id_1(
                json!({"jsonrpc":"2.0","result":{"id":"t","status":{"state":"TASK_STATE_DORMIDA"}}}),
            )),
        )
        .await;

        let resultado = cliente(&servidor, None).get("t").await;

        assert!(
            matches!(resultado, Err(RemoteTaskError::Other(_))),
            "{resultado:?}"
        );
    }

    #[tokio::test]
    async fn a_response_larger_than_the_cap_is_refused() {
        let servidor = MockServer::start().await;
        let enorme = "x".repeat(10_000);
        montar(
            &servidor,
            "GetTask",
            ResponseTemplate::new(200).set_body_json(con_id_1(json!({
                "jsonrpc":"2.0",
                "result":{"id":"t","status":{"state":"TASK_STATE_COMPLETED"},
                          "artifacts":[{"artifactId":"a","parts":[{"text": enorme}]}]}
            }))),
        )
        .await;
        let pequeno =
            JsonRpcClient::new(http(2), format!("{}/a2a/v1", servidor.uri()), None, 1_000);

        let resultado = pequeno.get("t").await;

        match resultado {
            Err(RemoteTaskError::Other(texto)) => assert!(texto.contains("exceeds"), "{texto}"),
            otro => panic!("se esperaba un error de tamaño, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn a_remote_that_does_not_answer_in_time_is_a_timeout() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "GetTask",
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(Duration::from_secs(4)),
        )
        .await;
        let lento = JsonRpcClient::new(http(1), format!("{}/a2a/v1", servidor.uri()), None, 1_000);

        let resultado = lento.get("t").await;

        assert_eq!(resultado, Err(RemoteTaskError::Timeout));
    }

    #[tokio::test]
    async fn a_plain_http_failure_without_json_reports_the_status() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "GetTask",
            ResponseTemplate::new(502).set_body_string("bad gateway"),
        )
        .await;

        let resultado = cliente(&servidor, None).get("t").await;

        assert_eq!(
            resultado,
            Err(RemoteTaskError::Other("HTTP status 502".into()))
        );
    }
}
