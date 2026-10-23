//! El enrutador del servidor (A8b): tarjeta pública y punto JSON-RPC autenticado.
//!
//! Cabe señalar que el orden de las comprobaciones importa y es deliberado: primero la
//! autenticación (sin credencial no se lee ni el cuerpo), luego el tope del cuerpo, después el
//! JSON-RPC y la versión, y solo al final el método. Los errores de JSON-RPC viajan con HTTP 200,
//! como es habitual; solo 401, 413 y 503 usan el estado HTTP.
//!
//! Lo que el servidor sabe hacer con una tarea lo abstrae [`TaskHost`], de modo que el enrutador
//! se prueba en memoria con un anfitrión falso, sin abrir ningún puerto.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};

use super::auth::Authenticator;
use super::card::ServedCard;
use super::config::ServeAuthScheme;
use crate::card::VERSION_SOPORTADA;
use crate::wire::{WireMessage, WireRole};

/// Tope del cuerpo de una petición.
pub const MAX_BODY_BYTES: usize = 64 * 1024;
/// Tope del texto de un mensaje.
pub const MAX_PROMPT_BYTES: usize = 32 * 1024;
const MAX_ID_BYTES: usize = 200;

// Errores de JSON-RPC.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
// Errores de A2A (§5.4).
const TASK_NOT_FOUND: i64 = -32001;
const TASK_NOT_CANCELABLE: i64 = -32002;
const PUSH_NOT_SUPPORTED: i64 = -32003;
const UNSUPPORTED_OPERATION: i64 = -32004;
const CONTENT_TYPE_NOT_SUPPORTED: i64 = -32005;
const VERSION_NOT_SUPPORTED: i64 = -32009;
/// «Servidor ocupado»: código nuestro, A2A no tiene uno (rango de errores de servidor de JSON-RPC).
const SERVER_BUSY: i64 = -32000;

/// Una tarea tal como la ve el cable, con el dato que el enrutador necesita para decidir si espera.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskView {
    /// `Task` en nombres del cable.
    pub json: Value,
    /// `true` si el estado ya es terminal.
    pub terminal: bool,
}

/// Por qué el anfitrión no pudo atender una operación.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostError {
    /// No existe, o no es de este servidor: no se distingue (§3.3.2).
    NotFound,
    NotCancelable,
    /// Demasiadas tareas vivas.
    Busy,
    /// Fallo interno; el texto no se envía al cliente.
    Other(String),
}

/// Lo que el servidor necesita saber hacer con las tareas.
#[async_trait]
pub trait TaskHost: Send + Sync {
    /// Crea una tarea con este texto.
    async fn start(&self, prompt: String) -> Result<TaskView, HostError>;
    /// Una tarea propia; `NotFound` para cualquier otra, exista o no.
    async fn get(&self, id: &str) -> Result<TaskView, HostError>;
    async fn cancel(&self, id: &str) -> Result<TaskView, HostError>;
    /// Espera a un estado terminal hasta `max`; si vence, devuelve la tarea como esté.
    async fn wait_terminal(&self, id: &str, max: Duration) -> Result<TaskView, HostError>;
}

/// Estado compartido del servidor.
pub struct ServeState {
    pub host: Arc<dyn TaskHost>,
    pub auth: Authenticator,
    pub card: ServedCard,
    pub max_wait: Duration,
}

/// Enrutador completo: tarjeta pública en `/.well-known/agent-card.json` y JSON-RPC en `/a2a`.
pub fn router(estado: Arc<ServeState>) -> Router {
    Router::new()
        .route("/.well-known/agent-card.json", get(tarjeta))
        .route("/a2a", post(rpc))
        .with_state(estado)
}

async fn tarjeta(State(estado): State<Arc<ServeState>>, cabeceras: HeaderMap) -> Response {
    let etag = HeaderValue::from_str(&estado.card.etag).expect("el ETag es ASCII");
    if cabeceras
        .get(header::IF_NONE_MATCH)
        .is_some_and(|valor| *valor == etag)
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    (
        StatusCode::OK,
        [
            (header::ETAG, etag),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("max-age=60"),
            ),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
        ],
        estado.card.json.to_string(),
    )
        .into_response()
}

fn respuesta_json(valor: Value) -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        valor.to_string(),
    )
        .into_response()
}

fn error_rpc(id: Value, codigo: i64, mensaje: &str) -> Response {
    respuesta_json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": codigo, "message": mensaje },
    }))
}

fn exito(id: Value, resultado: Value) -> Response {
    respuesta_json(json!({ "jsonrpc": "2.0", "id": id, "result": resultado }))
}

/// 401 sin cuerpo informativo: no se dice si faltaba la credencial o era incorrecta.
fn no_autorizado(esquema: &ServeAuthScheme) -> Response {
    let mut respuesta = StatusCode::UNAUTHORIZED.into_response();
    if matches!(esquema, ServeAuthScheme::Bearer) {
        respuesta
            .headers_mut()
            .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
    }
    respuesta
}

fn desde_anfitrion(id: Value, error: HostError) -> Response {
    match error {
        HostError::NotFound => error_rpc(id, TASK_NOT_FOUND, "task not found"),
        HostError::NotCancelable => error_rpc(id, TASK_NOT_CANCELABLE, "task is not cancelable"),
        HostError::Busy => {
            let mut respuesta = error_rpc(id, SERVER_BUSY, "server busy: too many live tasks");
            *respuesta.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
            respuesta
        }
        // El detalle interno no se entrega al cliente.
        HostError::Other(_) => error_rpc(id, INTERNAL_ERROR, "internal error"),
    }
}

async fn rpc(State(estado): State<Arc<ServeState>>, peticion: Request) -> Response {
    let (partes, cuerpo) = peticion.into_parts();

    // 1. Autenticación, antes de leer nada.
    let autenticada = estado.auth.check(&|nombre| {
        partes
            .headers
            .get(nombre)
            .and_then(|valor| valor.to_str().ok())
            .map(str::to_string)
    });
    if !autenticada {
        return no_autorizado(estado.auth.scheme());
    }

    // 2. Tope del cuerpo, sin leerlo entero si ya se sabe que pasa.
    let declarado = partes
        .headers
        .get(header::CONTENT_LENGTH)
        .and_then(|valor| valor.to_str().ok())
        .and_then(|valor| valor.parse::<usize>().ok());
    if declarado.is_some_and(|largo| largo > MAX_BODY_BYTES) {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let bytes = match to_bytes(Body::new(cuerpo), MAX_BODY_BYTES).await {
        Ok(bytes) => bytes,
        Err(_) => return StatusCode::PAYLOAD_TOO_LARGE.into_response(),
    };

    // 3. JSON-RPC.
    let Ok(sobre) = serde_json::from_slice::<Value>(&bytes) else {
        return error_rpc(Value::Null, PARSE_ERROR, "parse error");
    };
    let id = match sobre.get("id") {
        Some(id @ (Value::String(_) | Value::Number(_))) => id.clone(),
        _ => Value::Null,
    };
    let metodo = sobre.get("method").and_then(Value::as_str);
    if sobre.get("jsonrpc") != Some(&json!("2.0")) || id.is_null() || metodo.is_none() {
        return error_rpc(Value::Null, INVALID_REQUEST, "invalid request");
    }
    let metodo = metodo.unwrap_or_default();

    // 4. Versión de A2A: solo 1.0, sin degradar (§3.6).
    let version = partes
        .headers
        .get("a2a-version")
        .and_then(|valor| valor.to_str().ok())
        .map(str::trim)
        .unwrap_or("");
    if version != VERSION_SOPORTADA {
        return error_rpc(
            id,
            VERSION_NOT_SUPPORTED,
            "A2A version not supported: this server speaks 1.0",
        );
    }

    let params = sobre.get("params").cloned().unwrap_or(Value::Null);
    match metodo {
        "SendMessage" => enviar(&estado, id, &params).await,
        "GetTask" => consultar(&estado, id, &params, false).await,
        "CancelTask" => consultar(&estado, id, &params, true).await,
        "SendStreamingMessage" | "SubscribeToTask" | "ListTasks" | "GetExtendedAgentCard" => {
            error_rpc(id, UNSUPPORTED_OPERATION, "operation not supported")
        }
        "CreateTaskPushNotificationConfig"
        | "GetTaskPushNotificationConfig"
        | "ListTaskPushNotificationConfigs"
        | "DeleteTaskPushNotificationConfig" => error_rpc(
            id,
            PUSH_NOT_SUPPORTED,
            "push notifications are not supported",
        ),
        _ => error_rpc(id, METHOD_NOT_FOUND, "method not found"),
    }
}

async fn consultar(estado: &ServeState, id: Value, params: &Value, cancelar: bool) -> Response {
    let tarea = params
        .get("id")
        .and_then(Value::as_str)
        .filter(|tarea| !tarea.is_empty() && tarea.len() <= MAX_ID_BYTES);
    let Some(tarea) = tarea else {
        return error_rpc(id, INVALID_PARAMS, "params.id must be a task id");
    };
    let resultado = if cancelar {
        estado.host.cancel(tarea).await
    } else {
        estado.host.get(tarea).await
    };
    match resultado {
        Ok(vista) => exito(id, vista.json),
        Err(error) => desde_anfitrion(id, error),
    }
}

async fn enviar(estado: &ServeState, id: Value, params: &Value) -> Response {
    let invalido = |motivo: &str| error_rpc(id.clone(), INVALID_PARAMS, motivo);
    let Some(mensaje) = params.get("message") else {
        return invalido("params.message is required");
    };
    let Ok(mensaje) = serde_json::from_value::<WireMessage>(mensaje.clone()) else {
        return invalido("params.message is not a valid Message");
    };
    if mensaje.role != WireRole::User {
        return invalido("params.message.role must be ROLE_USER");
    }
    // El agente servido nunca pide entrada (solo lee), así que no hay tarea que reanudar.
    if mensaje.task_id.is_some() {
        return error_rpc(
            id,
            UNSUPPORTED_OPERATION,
            "continuing an existing task is not supported",
        );
    }
    if mensaje.parts.is_empty() {
        return invalido("params.message.parts must not be empty");
    }
    let mut textos = Vec::with_capacity(mensaje.parts.len());
    for parte in &mensaje.parts {
        match (&parte.text, &parte.raw, &parte.url, &parte.data) {
            (Some(texto), None, None, None) => textos.push(texto.as_str()),
            _ => {
                return error_rpc(
                    id,
                    CONTENT_TYPE_NOT_SUPPORTED,
                    "only text parts are supported",
                )
            }
        }
    }
    let prompt = textos.join("\n");
    if prompt.trim().is_empty() {
        return invalido("params.message has no text");
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return invalido("params.message is too large");
    }
    let sin_esperar = params
        .get("configuration")
        .and_then(|configuracion| configuracion.get("returnImmediately"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut vista = match estado.host.start(prompt).await {
        Ok(vista) => vista,
        Err(error) => return desde_anfitrion(id, error),
    };
    if !sin_esperar && !vista.terminal {
        let tarea = vista.json.get("id").and_then(Value::as_str).unwrap_or("");
        match estado.host.wait_terminal(tarea, estado.max_wait).await {
            Ok(esperada) => vista = esperada,
            Err(error) => return desde_anfitrion(id, error),
        }
    }
    exito(id, json!({ "task": vista.json }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::card::served_card;
    use crate::serve::config::{ServeAuthConfig, ServeAuthKind, ServeConfig};
    use codeloom_core::card::AgentCard;
    use std::sync::Mutex;
    use tower::ServiceExt;

    const SECRETO: &str = "s3cr3to-de-prueba";

    /// Anfitrión falso: guarda lo que se le pide y contesta según un guion.
    #[derive(Default)]
    struct Falso {
        iniciadas: Mutex<Vec<String>>,
        esperas: Mutex<usize>,
        ocupado: bool,
        termina_al_instante: bool,
    }

    fn tarea(estado: &str, terminal: bool) -> TaskView {
        TaskView {
            json: json!({ "id": "t-1", "status": { "state": estado } }),
            terminal,
        }
    }

    #[async_trait]
    impl TaskHost for Falso {
        async fn start(&self, prompt: String) -> Result<TaskView, HostError> {
            if self.ocupado {
                return Err(HostError::Busy);
            }
            self.iniciadas.lock().unwrap().push(prompt);
            Ok(if self.termina_al_instante {
                tarea("TASK_STATE_COMPLETED", true)
            } else {
                tarea("TASK_STATE_WORKING", false)
            })
        }
        async fn get(&self, id: &str) -> Result<TaskView, HostError> {
            if id == "t-1" {
                Ok(tarea("TASK_STATE_WORKING", false))
            } else {
                Err(HostError::NotFound)
            }
        }
        async fn cancel(&self, id: &str) -> Result<TaskView, HostError> {
            match id {
                "t-1" => Ok(tarea("TASK_STATE_CANCELED", true)),
                "terminada" => Err(HostError::NotCancelable),
                _ => Err(HostError::NotFound),
            }
        }
        async fn wait_terminal(&self, _id: &str, _max: Duration) -> Result<TaskView, HostError> {
            *self.esperas.lock().unwrap() += 1;
            Ok(tarea("TASK_STATE_COMPLETED", true))
        }
    }

    fn estado(host: Falso, esquema: ServeAuthKind) -> (Arc<ServeState>, Arc<Falso>) {
        let host = Arc::new(host);
        let config = ServeConfig {
            auth: Some(ServeAuthConfig {
                scheme: esquema,
                name: Some("X-Api-Key".into()),
                env: "S".into(),
            }),
            ..ServeConfig::default()
        };
        let resuelta = config.validate(&|_| Some(SECRETO.to_string())).unwrap();
        let tarjeta = served_card(
            &AgentCard::local("default", "Agente de CodeLoom", "1.0.0"),
            "http://127.0.0.1:7077/a2a",
            &resuelta.scheme,
        );
        let estado = Arc::new(ServeState {
            host: host.clone(),
            auth: Authenticator::new(resuelta),
            card: tarjeta,
            max_wait: Duration::from_secs(5),
        });
        (estado, host)
    }

    fn bearer(host: Falso) -> (Arc<ServeState>, Arc<Falso>) {
        estado(host, ServeAuthKind::Bearer)
    }

    fn rpc_peticion(cuerpo: &str, auth: Option<&str>, version: Option<&str>) -> Request {
        let mut constructor = Request::builder().method("POST").uri("/a2a");
        if let Some(auth) = auth {
            constructor = constructor.header("authorization", auth);
        }
        if let Some(version) = version {
            constructor = constructor.header("a2a-version", version);
        }
        constructor.body(Body::from(cuerpo.to_string())).unwrap()
    }

    fn buena(cuerpo: Value) -> Request {
        rpc_peticion(
            &cuerpo.to_string(),
            Some(&format!("Bearer {SECRETO}")),
            Some("1.0"),
        )
    }

    async fn enviar_req(
        estado: &Arc<ServeState>,
        req: Request,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let respuesta = router(estado.clone()).oneshot(req).await.unwrap();
        let estado_http = respuesta.status();
        let cabeceras = respuesta.headers().clone();
        let bytes = to_bytes(respuesta.into_body(), usize::MAX).await.unwrap();
        (estado_http, cabeceras, bytes.to_vec())
    }

    async fn llamar(estado: &Arc<ServeState>, cuerpo: Value) -> (StatusCode, Value) {
        let (http, _, bytes) = enviar_req(estado, buena(cuerpo)).await;
        (http, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    fn mensaje(texto: &str) -> Value {
        json!({ "messageId": "m1", "role": "ROLE_USER", "parts": [{ "text": texto }] })
    }

    fn envio(texto: &str) -> Value {
        json!({ "jsonrpc": "2.0", "id": 1, "method": "SendMessage",
                "params": { "message": mensaje(texto) } })
    }

    #[tokio::test]
    async fn without_a_correct_credential_the_answer_is_an_empty_401() {
        let (estado, host) = bearer(Falso::default());
        let cuerpo = envio("hola").to_string();
        for auth in [
            None,
            Some(""),
            Some("Bearer "),
            Some("Bearer s3cr3to-de-pru"),
            Some("Bearer s3cr3to-de-prueba-y-mas"),
            Some("Basic s3cr3to-de-prueba"),
        ] {
            let (http, cabeceras, bytes) =
                enviar_req(&estado, rpc_peticion(&cuerpo, auth, Some("1.0"))).await;

            assert_eq!(http, StatusCode::UNAUTHORIZED, "{auth:?}");
            assert!(bytes.is_empty(), "{auth:?}: el cuerpo no debe informar");
            assert_eq!(cabeceras[header::WWW_AUTHENTICATE], "Bearer");
        }
        assert!(host.iniciadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_api_key_server_reads_its_own_header() {
        let (estado, _) = estado(Falso::default(), ServeAuthKind::ApiKey);
        let peticion = Request::builder()
            .method("POST")
            .uri("/a2a")
            .header("x-api-key", SECRETO)
            .header("a2a-version", "1.0")
            .body(Body::from(envio("hola").to_string()))
            .unwrap();

        let (http, _, _) = enviar_req(&estado, peticion).await;
        assert_eq!(http, StatusCode::OK);

        let (http, cabeceras, _) = enviar_req(&estado, buena(envio("hola"))).await;
        assert_eq!(http, StatusCode::UNAUTHORIZED);
        assert!(cabeceras.get(header::WWW_AUTHENTICATE).is_none());
    }

    #[tokio::test]
    async fn the_agent_card_is_public_and_cacheable() {
        let (estado, _) = bearer(Falso::default());
        let peticion = Request::builder()
            .uri("/.well-known/agent-card.json")
            .body(Body::empty())
            .unwrap();

        let (http, cabeceras, bytes) = enviar_req(&estado, peticion).await;

        assert_eq!(http, StatusCode::OK);
        assert_eq!(cabeceras[header::CACHE_CONTROL], "max-age=60");
        let etag = cabeceras[header::ETAG].to_str().unwrap().to_string();
        let tarjeta: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(tarjeta["name"], "default");

        let condicional = Request::builder()
            .uri("/.well-known/agent-card.json")
            .header("if-none-match", etag)
            .body(Body::empty())
            .unwrap();
        let (http, _, bytes) = enviar_req(&estado, condicional).await;
        assert_eq!(http, StatusCode::NOT_MODIFIED);
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn an_oversized_body_is_refused_with_413() {
        let (estado, host) = bearer(Falso::default());
        let enorme = format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"SendMessage","params":{{"x":"{}"}}}}"#,
            "a".repeat(MAX_BODY_BYTES)
        );

        let (http, _, _) = enviar_req(
            &estado,
            rpc_peticion(&enorme, Some(&format!("Bearer {SECRETO}")), Some("1.0")),
        )
        .await;

        assert_eq!(http, StatusCode::PAYLOAD_TOO_LARGE);
        assert!(host.iniciadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn malformed_json_rpc_gets_a_json_rpc_error_not_a_500() {
        let (estado, _) = bearer(Falso::default());
        for (cuerpo, codigo) in [
            ("{no es json".to_string(), PARSE_ERROR),
            (json!([1, 2]).to_string(), INVALID_REQUEST),
            (
                json!({"jsonrpc": "1.0", "id": 1, "method": "GetTask"}).to_string(),
                INVALID_REQUEST,
            ),
            (
                json!({"jsonrpc": "2.0", "method": "GetTask"}).to_string(),
                INVALID_REQUEST,
            ),
            (
                json!({"jsonrpc": "2.0", "id": [1], "method": "GetTask"}).to_string(),
                INVALID_REQUEST,
            ),
            (
                json!({"jsonrpc": "2.0", "id": 1, "method": 7}).to_string(),
                INVALID_REQUEST,
            ),
        ] {
            let (http, _, bytes) = enviar_req(
                &estado,
                rpc_peticion(&cuerpo, Some(&format!("Bearer {SECRETO}")), Some("1.0")),
            )
            .await;
            let valor: Value = serde_json::from_slice(&bytes).unwrap();

            assert_eq!(http, StatusCode::OK, "{cuerpo}");
            assert_eq!(valor["error"]["code"], codigo, "{cuerpo}");
            assert_eq!(valor["id"], Value::Null, "{cuerpo}");
        }
    }

    #[tokio::test]
    async fn only_a2a_version_1_0_is_served() {
        let (estado, host) = bearer(Falso::default());
        let cuerpo = envio("hola").to_string();
        for version in [None, Some(""), Some("0.3"), Some("1.1")] {
            let (_, _, bytes) = enviar_req(
                &estado,
                rpc_peticion(&cuerpo, Some(&format!("Bearer {SECRETO}")), version),
            )
            .await;
            let valor: Value = serde_json::from_slice(&bytes).unwrap();

            assert_eq!(valor["error"]["code"], VERSION_NOT_SUPPORTED, "{version:?}");
            assert_eq!(valor["id"], 1);
        }
        assert!(host.iniciadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_blocking_send_waits_and_return_immediately_does_not() {
        let (estado, host) = bearer(Falso::default());

        let (_, valor) = llamar(&estado, envio("hola")).await;
        assert_eq!(
            valor["result"]["task"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );
        assert_eq!(*host.esperas.lock().unwrap(), 1);
        assert_eq!(host.iniciadas.lock().unwrap().as_slice(), ["hola"]);

        let mut inmediato = envio("otra");
        inmediato["params"]["configuration"] = json!({ "returnImmediately": true });
        let (_, valor) = llamar(&estado, inmediato).await;
        assert_eq!(
            valor["result"]["task"]["status"]["state"],
            "TASK_STATE_WORKING"
        );
        assert_eq!(*host.esperas.lock().unwrap(), 1, "no debe esperar");
    }

    #[tokio::test]
    async fn a_task_already_terminal_is_not_waited_for() {
        let (estado, host) = bearer(Falso {
            termina_al_instante: true,
            ..Falso::default()
        });

        let (_, valor) = llamar(&estado, envio("hola")).await;

        assert_eq!(
            valor["result"]["task"]["status"]["state"],
            "TASK_STATE_COMPLETED"
        );
        assert_eq!(*host.esperas.lock().unwrap(), 0);
    }

    #[tokio::test]
    async fn non_text_parts_are_refused_and_no_task_is_created() {
        let (estado, host) = bearer(Falso::default());
        for parte in [
            json!({ "raw": "aGVsbG8=" }),
            json!({ "url": "https://x.example/f" }),
            json!({ "data": { "a": 1 } }),
        ] {
            let cuerpo = json!({ "jsonrpc": "2.0", "id": 1, "method": "SendMessage",
                "params": { "message": { "messageId": "m", "role": "ROLE_USER", "parts": [parte] } } });

            let (_, valor) = llamar(&estado, cuerpo).await;

            assert_eq!(valor["error"]["code"], CONTENT_TYPE_NOT_SUPPORTED);
        }
        assert!(host.iniciadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn invalid_messages_are_invalid_params() {
        let (estado, host) = bearer(Falso::default());
        let enorme = "x".repeat(MAX_PROMPT_BYTES + 1);
        let mut agente = envio("hola");
        agente["params"]["message"]["role"] = json!("ROLE_AGENT");
        let casos = vec![
            json!({ "jsonrpc": "2.0", "id": 1, "method": "SendMessage", "params": {} }),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "SendMessage", "params": { "message": 3 } }),
            agente,
            envio(""),
            envio("   "),
            envio(&enorme),
            json!({ "jsonrpc": "2.0", "id": 1, "method": "SendMessage",
                "params": { "message": { "messageId": "m", "role": "ROLE_USER", "parts": [] } } }),
        ];
        for caso in casos {
            let (_, valor) = llamar(&estado, caso.clone()).await;

            assert_eq!(valor["error"]["code"], INVALID_PARAMS, "{caso}");
        }
        assert!(host.iniciadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn continuing_a_task_is_unsupported() {
        let (estado, host) = bearer(Falso::default());
        let mut continuacion = envio("hola");
        continuacion["params"]["message"]["taskId"] = json!("t-1");

        let (_, valor) = llamar(&estado, continuacion).await;

        assert_eq!(valor["error"]["code"], UNSUPPORTED_OPERATION);
        assert!(host.iniciadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_busy_server_answers_503_with_a_json_rpc_error() {
        let (estado, _) = bearer(Falso {
            ocupado: true,
            ..Falso::default()
        });

        let (http, valor) = llamar(&estado, envio("hola")).await;

        assert_eq!(http, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(valor["error"]["code"], SERVER_BUSY);
    }

    #[tokio::test]
    async fn get_and_cancel_map_the_host_answers() {
        let (estado, _) = bearer(Falso::default());
        let con = |metodo: &str, id: &str| json!({ "jsonrpc": "2.0", "id": "x", "method": metodo, "params": { "id": id } });

        let (_, valor) = llamar(&estado, con("GetTask", "t-1")).await;
        assert_eq!(valor["result"]["id"], "t-1");
        assert_eq!(valor["id"], "x");

        let (_, valor) = llamar(&estado, con("CancelTask", "t-1")).await;
        assert_eq!(valor["result"]["status"]["state"], "TASK_STATE_CANCELED");

        let (_, valor) = llamar(&estado, con("CancelTask", "terminada")).await;
        assert_eq!(valor["error"]["code"], TASK_NOT_CANCELABLE);

        let (_, valor) = llamar(&estado, con("GetTask", "inventada")).await;
        assert_eq!(valor["error"]["code"], TASK_NOT_FOUND);
    }

    #[tokio::test]
    async fn an_unknown_and_a_foreign_task_look_exactly_the_same() {
        let (estado, _) = bearer(Falso::default());
        let con = |id: &str| json!({ "jsonrpc": "2.0", "id": 7, "method": "GetTask", "params": { "id": id } });

        let (_, _, a) = enviar_req(&estado, buena(con("no-existe"))).await;
        let (_, _, b) = enviar_req(&estado, buena(con("de-otra-sesion"))).await;

        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn a_missing_or_huge_task_id_is_invalid_params() {
        let (estado, _) = bearer(Falso::default());
        let largo = "a".repeat(MAX_ID_BYTES + 1);
        for params in [
            json!({}),
            json!({ "id": "" }),
            json!({ "id": 3 }),
            json!({ "id": largo }),
        ] {
            let cuerpo =
                json!({ "jsonrpc": "2.0", "id": 1, "method": "GetTask", "params": params });

            let (_, valor) = llamar(&estado, cuerpo).await;

            assert_eq!(valor["error"]["code"], INVALID_PARAMS, "{params}");
        }
    }

    #[tokio::test]
    async fn unsupported_and_unknown_methods_have_their_own_codes() {
        let (estado, _) = bearer(Falso::default());
        for (metodo, codigo) in [
            ("SendStreamingMessage", UNSUPPORTED_OPERATION),
            ("SubscribeToTask", UNSUPPORTED_OPERATION),
            ("ListTasks", UNSUPPORTED_OPERATION),
            ("GetExtendedAgentCard", UNSUPPORTED_OPERATION),
            ("CreateTaskPushNotificationConfig", PUSH_NOT_SUPPORTED),
            ("GetTaskPushNotificationConfig", PUSH_NOT_SUPPORTED),
            ("ListTaskPushNotificationConfigs", PUSH_NOT_SUPPORTED),
            ("DeleteTaskPushNotificationConfig", PUSH_NOT_SUPPORTED),
            ("message/send", METHOD_NOT_FOUND),
            ("Inventado", METHOD_NOT_FOUND),
        ] {
            let cuerpo = json!({ "jsonrpc": "2.0", "id": 1, "method": metodo, "params": {} });

            let (_, valor) = llamar(&estado, cuerpo).await;

            assert_eq!(valor["error"]["code"], codigo, "{metodo}");
        }
    }

    #[tokio::test]
    async fn an_internal_host_failure_never_leaks_its_detail() {
        struct Roto;
        #[async_trait]
        impl TaskHost for Roto {
            async fn start(&self, _: String) -> Result<TaskView, HostError> {
                Err(HostError::Other("ruta /home/secreta falló".into()))
            }
            async fn get(&self, _: &str) -> Result<TaskView, HostError> {
                Err(HostError::Other("detalle".into()))
            }
            async fn cancel(&self, _: &str) -> Result<TaskView, HostError> {
                Err(HostError::Other("detalle".into()))
            }
            async fn wait_terminal(&self, _: &str, _: Duration) -> Result<TaskView, HostError> {
                Err(HostError::Other("detalle".into()))
            }
        }
        let (mut estado, _) = bearer(Falso::default());
        Arc::get_mut(&mut estado).unwrap().host = Arc::new(Roto);

        let (_, _, bytes) = enviar_req(&estado, buena(envio("hola"))).await;
        let texto = String::from_utf8(bytes).unwrap();

        assert!(texto.contains("internal error"));
        assert!(!texto.contains("secreta"), "{texto}");
    }
}
