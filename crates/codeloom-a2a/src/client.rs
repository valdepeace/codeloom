//! Cliente JSON-RPC de un agente remoto (A8a, T4): `SendMessage`, `GetTask` y `CancelTask`.
//!
//! Cabe señalar que habla con la interfaz que la tarjeta aprobada declaró, envía siempre
//! `A2A-Version: 1.0`, acota el cuerpo de cada respuesta y no afirma nada que el remoto no diga:
//! un error de A2A se traduce a su equivalente y cualquier otra cosa es un error visible.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use codeloom_core::a2a::{Artifact, Message};
use codeloom_core::registry::RemoteSpec;
use codeloom_core::remote_task::{
    RemoteClientFactory, RemoteReply, RemoteTask, RemoteTaskClient, RemoteTaskError,
};
use codeloom_core::task::TaskStatus;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::card::VERSION_SOPORTADA;
use crate::config::A2aConfig;
use crate::remote::{http_client, http_client_with_identity, leer_acotado, mapear, RemoteError};
use crate::security::AuthPlan;
use crate::sse::SseDecoder;
use crate::wire::{
    state_from_wire, timestamp_from_wire, WireMessage, WireStreamResponse, WireTask,
};

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
    auth: AuthPlan,
    /// La tarjeta declara streaming: solo entonces se intenta `SubscribeToTask` (A8c).
    streaming: bool,
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
            auth: AuthPlan::open(),
            streaming: false,
        }
    }

    /// Indica que la tarjeta del remoto declara `capabilities.streaming = true`.
    pub fn with_streaming(mut self, streaming: bool) -> Self {
        self.streaming = streaming;
        self
    }

    /// Autentica cada petición con `auth`. Sin esto el cliente habla a un remoto abierto.
    pub fn with_auth(mut self, auth: AuthPlan) -> Self {
        self.auth = auth;
        self
    }

    /// Envía una petición autenticada. Si el remoto rechaza un token (401), se olvida y se pide otro
    /// una sola vez: un token revocado o caducado antes de tiempo no debe tumbar la tarea.
    async fn enviar(
        &self,
        construir: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, RemoteTaskError> {
        let mut reintentado = false;
        loop {
            let peticion = self
                .auth
                .apply(construir())
                .await
                .map_err(RemoteTaskError::AuthFailed)?;
            let respuesta = peticion
                .send()
                .await
                .map_err(|e| traducir_fallo(mapear(e)))?;
            if respuesta.status() == reqwest::StatusCode::UNAUTHORIZED
                && !reintentado
                && self.auth.uses_token()
            {
                reintentado = true;
                self.auth.invalidate().await;
                continue;
            }
            return Ok(respuesta);
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
            .enviar(|| {
                self.http
                    .post(&self.url)
                    .header("A2A-Version", VERSION_SOPORTADA)
                    .header(reqwest::header::ACCEPT, "application/json")
                    .json(&cuerpo)
            })
            .await?;
        let estado = respuesta.status();
        // El cuerpo de un rechazo no se lee ni se repite: podría citar la propia credencial.
        if estado == reqwest::StatusCode::UNAUTHORIZED || estado == reqwest::StatusCode::FORBIDDEN {
            return Err(RemoteTaskError::AuthFailed(format!(
                "HTTP {}; revisa la credencial declarada en `a2a.credentials`",
                estado.as_u16()
            )));
        }
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

/// Lee una variable de entorno: el entorno del proceso, o el de una prueba.
type LectorDeEntorno = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Fábrica de clientes de los remotos del registro: la que se entrega al lanzador. Solo existe con
/// el rol `consume` (o `both`); con A2A apagado no se construye y los remotos no se ejecutan.
pub struct A2aClientFactory {
    http: Client,
    config: A2aConfig,
    max_result_bytes: usize,
    env: LectorDeEntorno,
}

impl A2aClientFactory {
    /// Lee las credenciales de otra fuente que no sea el entorno del proceso (pruebas).
    pub fn with_env(
        mut self,
        env: impl Fn(&str) -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        self.env = Arc::new(env);
        self
    }

    /// Cliente HTTP para un plan: el compartido, o uno propio si el plan exige una identidad TLS.
    fn http_for(&self, plan: &AuthPlan) -> Result<Client, String> {
        match plan.client_identity() {
            None => Ok(self.http.clone()),
            Some(identidad) => http_client_with_identity(&self.config, identidad.pem())
                .map_err(|error| error.to_string()),
        }
    }

    /// Plan de autenticación de un remoto con tarjeta descargada, o el motivo de que no lo haya.
    fn plan_for(&self, spec: &RemoteSpec) -> Option<Result<AuthPlan, String>> {
        let descargada = spec.fetched.as_ref()?;
        Some(AuthPlan::for_card(
            descargada.security.as_deref(),
            descargada.requires_auth,
            &spec.credentials,
            &*self.env,
        ))
    }

    /// `None` si el rol no es de consumo: no se construye ningún cliente HTTP.
    pub fn from_config(config: &A2aConfig) -> Result<Option<Self>, RemoteError> {
        Ok(http_client(config)?.map(|http| A2aClientFactory {
            http,
            config: config.clone(),
            max_result_bytes: config.max_result_bytes,
            env: Arc::new(|variable| std::env::var(variable).ok()),
        }))
    }
}

impl RemoteClientFactory for A2aClientFactory {
    /// Cliente de la interfaz que la tarjeta descargada declaró; sin tarjeta, ninguno.
    fn client_for(&self, _name: &str, spec: &RemoteSpec) -> Option<Arc<dyn RemoteTaskClient>> {
        let descargada = spec.fetched.as_ref()?;
        // Sin un plan de autenticación válido no hay cliente: jamás se habla sin credencial a un
        // remoto que la pide.
        let plan = self.plan_for(spec)?.ok()?;
        // Un remoto con mTLS usa un cliente HTTP propio con su identidad; si no se puede construir,
        // no hay cliente: jamás se cae a hablarle sin ella.
        let http = self.http_for(&plan).ok()?;
        Some(Arc::new(
            JsonRpcClient::new(
                http,
                descargada.jsonrpc_url.clone(),
                descargada.tenant.clone(),
                self.max_result_bytes,
            )
            .with_auth(plan)
            .with_streaming(descargada.streaming),
        ))
    }

    fn unavailable_reason(&self, _name: &str, spec: &RemoteSpec) -> Option<String> {
        match self.plan_for(spec)? {
            Err(motivo) => Some(motivo),
            Ok(plan) => self.http_for(&plan).err(),
        }
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

/// Tiempo máximo de una conexión de flujo. Quien sigue la tarea la suelta antes (`max_follow`); esto
/// solo evita que el tope de petición del cliente (pensado para llamadas cortas) corte un flujo legítimo.
const MAX_CONEXION_FLUJO: Duration = Duration::from_secs(6 * 3600);
/// Artefactos que se acumulan como máximo al seguir un flujo.
const MAX_ARTEFACTOS_FLUJO: usize = 64;

/// Estado de la lectura de un flujo de tarea.
struct LecturaFlujo {
    cuerpo: BoxStream<'static, Result<bytes::Bytes, reqwest::Error>>,
    decodificador: SseDecoder,
    pendientes: VecDeque<String>,
    actual: Option<RemoteTask>,
    tarea: String,
    rpc_id: u64,
    max_bytes: usize,
    terminado: bool,
}

/// Aplica un evento (`data:`) del flujo a la tarea que se sigue. Todo lo que no cuadra corta el flujo:
/// es dato de un tercero y no se interpreta a medias.
fn aplicar_evento(lectura: &mut LecturaFlujo, datos: &str) -> Result<RemoteTask, RemoteTaskError> {
    let rpc: RespuestaRpc = serde_json::from_str(datos).map_err(invalida)?;
    if rpc.id != json!(lectura.rpc_id) {
        return Err(RemoteTaskError::Other(
            "stream event id does not match the request".into(),
        ));
    }
    let resultado = match (rpc.result, rpc.error) {
        (Some(resultado), None) => resultado,
        (None, Some(error)) => return Err(traducir_error_rpc(error)),
        _ => {
            return Err(RemoteTaskError::Other(
                "stream event must carry exactly one of result and error".into(),
            ))
        }
    };
    let evento: WireStreamResponse = serde_json::from_value(resultado).map_err(invalida)?;
    evento.validate().map_err(invalida)?;
    let propia = |id: &str| -> Result<(), RemoteTaskError> {
        if id == lectura.tarea {
            Ok(())
        } else {
            Err(RemoteTaskError::Other(
                "the stream carries events of a different task".into(),
            ))
        }
    };
    if let Some(tarea) = evento.task {
        propia(&tarea.id)?;
        lectura.actual = Some(tarea_desde_wire(tarea)?);
    } else if let Some(cambio) = evento.status_update {
        propia(&cambio.task_id)?;
        let actual = lectura
            .actual
            .as_mut()
            .ok_or_else(|| RemoteTaskError::Other("the stream must start with the task".into()))?;
        let temporal = WireTask {
            id: cambio.task_id,
            context_id: None,
            status: cambio.status,
            artifacts: Vec::new(),
        };
        actual.status = tarea_desde_wire(temporal)?.status;
    } else if let Some(cambio) = evento.artifact_update {
        propia(&cambio.task_id)?;
        let actual = lectura
            .actual
            .as_mut()
            .ok_or_else(|| RemoteTaskError::Other("the stream must start with the task".into()))?;
        let artefacto = Artifact::try_from(cambio.artifact).map_err(invalida)?;
        match actual
            .artifacts
            .iter_mut()
            .find(|existente| existente.artifact_id == artefacto.artifact_id)
        {
            Some(existente) if cambio.append => existente.parts.extend(artefacto.parts),
            Some(existente) => *existente = artefacto,
            None => actual.artifacts.push(artefacto),
        }
        if actual.artifacts.len() > MAX_ARTEFACTOS_FLUJO
            || codeloom_core::a2a::artifacts_text(&actual.artifacts).len() > lectura.max_bytes
        {
            return Err(RemoteTaskError::Other(
                "the stream result is larger than allowed".into(),
            ));
        }
    } else {
        return Err(RemoteTaskError::Other(
            "unexpected message in a task stream".into(),
        ));
    }
    lectura
        .actual
        .clone()
        .ok_or_else(|| RemoteTaskError::Other("the stream must start with the task".into()))
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

    async fn watch(
        &self,
        id: &str,
    ) -> Result<BoxStream<'static, Result<RemoteTask, RemoteTaskError>>, RemoteTaskError> {
        // Se comprueba la tarjeta antes de intentarlo (§3.3.4): sin streaming declarado, ni se pregunta.
        if !self.streaming {
            return Err(RemoteTaskError::Unsupported(
                "the remote card does not declare streaming".into(),
            ));
        }
        let rpc_id = self.siguiente_id.fetch_add(1, Ordering::Relaxed);
        let params = self.con_tenant(json!({ "id": id }));
        let cuerpo = json!({ "jsonrpc": "2.0", "id": rpc_id, "method": "SubscribeToTask", "params": params });
        let respuesta = self
            .enviar(|| {
                self.http
                    .post(&self.url)
                    .header("A2A-Version", VERSION_SOPORTADA)
                    .header(reqwest::header::ACCEPT, "text/event-stream")
                    .timeout(MAX_CONEXION_FLUJO)
                    .json(&cuerpo)
            })
            .await?;
        let estado = respuesta.status();
        if estado == reqwest::StatusCode::UNAUTHORIZED || estado == reqwest::StatusCode::FORBIDDEN {
            return Err(RemoteTaskError::AuthFailed(format!(
                "HTTP {}; revisa la credencial declarada en `a2a.credentials`",
                estado.as_u16()
            )));
        }
        let es_flujo = respuesta
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|valor| valor.to_str().ok())
            .is_some_and(|valor| valor.starts_with("text/event-stream"));
        if !es_flujo {
            // Un error previo al flujo viaja como JSON-RPC normal (§9.4.6).
            let bytes = leer_acotado(respuesta, self.max_result_bytes)
                .await
                .map_err(traducir_fallo)?;
            return match serde_json::from_slice::<RespuestaRpc>(&bytes) {
                Ok(RespuestaRpc {
                    error: Some(error), ..
                }) => Err(traducir_error_rpc(error)),
                _ => Err(RemoteTaskError::Other(
                    "the remote did not answer with an event stream".into(),
                )),
            };
        }
        let lectura = LecturaFlujo {
            cuerpo: respuesta.bytes_stream().boxed(),
            decodificador: SseDecoder::new(),
            pendientes: VecDeque::new(),
            actual: None,
            tarea: id.to_string(),
            rpc_id,
            max_bytes: self.max_result_bytes,
            terminado: false,
        };
        Ok(futures::stream::unfold(lectura, |mut lectura| async move {
            loop {
                if lectura.terminado {
                    return None;
                }
                if let Some(datos) = lectura.pendientes.pop_front() {
                    return match aplicar_evento(&mut lectura, &datos) {
                        Ok(tarea) => Some((Ok(tarea), lectura)),
                        Err(error) => {
                            lectura.terminado = true;
                            Some((Err(error), lectura))
                        }
                    };
                }
                match lectura.cuerpo.next().await {
                    Some(Ok(trozo)) => match lectura.decodificador.push(&trozo) {
                        Ok(eventos) => lectura.pendientes.extend(eventos),
                        Err(error) => {
                            lectura.terminado = true;
                            return Some((Err(invalida(error)), lectura));
                        }
                    },
                    Some(Err(error)) => {
                        lectura.terminado = true;
                        return Some((Err(traducir_fallo(mapear(error))), lectura));
                    }
                    None => return None,
                }
            }
        })
        .boxed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{A2aConfig, A2aRole};
    use crate::remote::http_client;
    use crate::security::CardSecurity;
    use codeloom_core::registry::RemoteFetched;
    use codeloom_core::task::TaskState;
    use std::collections::BTreeMap;
    use std::time::Duration;
    use wiremock::matchers::{body_partial_json, header, method, path, query_param};
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

    // ---- A8c: seguimiento por flujo ----

    fn sobre(evento: Value) -> String {
        format!(
            "data: {}\n\n",
            json!({ "jsonrpc": "2.0", "id": 1, "result": evento })
        )
    }

    fn flujo_sse(cuerpo: impl Into<String>) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_raw(cuerpo.into().into_bytes(), "text/event-stream")
    }

    fn tarea_wire(estado: &str) -> Value {
        json!({ "task": { "id": "t-1", "contextId": "c-1", "status": { "state": estado } } })
    }

    fn estado_wire(estado: &str) -> Value {
        json!({ "statusUpdate": { "taskId": "t-1", "contextId": "c-1",
                                  "status": { "state": estado } } })
    }

    fn artefacto_wire(id: &str, texto: &str, append: bool) -> Value {
        json!({ "artifactUpdate": { "taskId": "t-1", "contextId": "c-1", "append": append,
                "artifact": { "artifactId": id, "parts": [{ "text": texto }] } } })
    }

    async fn recoger(
        cliente: &JsonRpcClient,
    ) -> Vec<Result<codeloom_core::remote_task::RemoteTask, RemoteTaskError>> {
        use futures::StreamExt;
        cliente.watch("t-1").await.unwrap().collect().await
    }

    #[tokio::test]
    async fn watch_yields_the_whole_task_after_each_event_and_applies_updates() {
        let servidor = MockServer::start().await;
        let cuerpo = [
            sobre(tarea_wire("TASK_STATE_WORKING")),
            ": latido\n\n".to_string(),
            sobre(artefacto_wire("a1", "hola ", false)),
            sobre(artefacto_wire("a1", "mundo", true)),
            sobre(estado_wire("TASK_STATE_COMPLETED")),
        ]
        .concat();
        montar(&servidor, "SubscribeToTask", flujo_sse(cuerpo)).await;

        let tareas = recoger(&cliente(&servidor, Some("t")).with_streaming(true)).await;

        let tareas: Vec<_> = tareas.into_iter().map(|t| t.unwrap()).collect();
        assert_eq!(tareas.len(), 4);
        assert_eq!(tareas[0].status.state, TaskState::Working);
        assert_eq!(tareas[1].artifacts[0].text(), "hola ");
        // `append` añade una parte al artefacto; `text()` une las partes con salto de línea.
        assert_eq!(tareas[2].artifacts[0].parts.len(), 2);
        assert_eq!(
            tareas[2].artifacts[0].text(),
            "hola 
mundo"
        );
        assert_eq!(tareas[3].status.state, TaskState::Completed);
        assert_eq!(tareas[3].artifacts.len(), 1);
        let recibidas = servidor.received_requests().await.unwrap();
        let cuerpo: Value = serde_json::from_slice(&recibidas[0].body).unwrap();
        assert_eq!(cuerpo["params"]["id"], "t-1");
        assert_eq!(cuerpo["params"]["tenant"], "t");
        assert_eq!(recibidas[0].headers["accept"], "text/event-stream");
    }

    #[tokio::test]
    async fn without_declared_streaming_watch_does_not_even_touch_the_network() {
        let servidor = MockServer::start().await;

        let error = cliente(&servidor, None).watch("t-1").await.err().unwrap();

        assert!(
            matches!(error, RemoteTaskError::Unsupported(_)),
            "{error:?}"
        );
        assert!(servidor.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn errors_before_the_stream_keep_their_meaning_and_a_401_is_an_auth_failure() {
        let servidor = MockServer::start().await;
        let no_encontrada: Value = serde_json::from_str(NO_ENCONTRADA).unwrap();
        montar(
            &servidor,
            "SubscribeToTask",
            ResponseTemplate::new(200).set_body_json(con_id_1(no_encontrada)),
        )
        .await;
        let error = cliente(&servidor, None)
            .with_streaming(true)
            .watch("t-1")
            .await
            .err()
            .unwrap();
        assert_eq!(error, RemoteTaskError::NotFound);

        let cerrado = MockServer::start().await;
        montar(
            &cerrado,
            "SubscribeToTask",
            ResponseTemplate::new(401).set_body_string("token-secreto-eco"),
        )
        .await;
        let error = cliente(&cerrado, None)
            .with_streaming(true)
            .watch("t-1")
            .await
            .err()
            .unwrap();
        assert!(matches!(error, RemoteTaskError::AuthFailed(_)), "{error:?}");
        assert!(!error.to_string().contains("token-secreto-eco"));
    }

    #[tokio::test]
    async fn a_non_stream_answer_is_refused() {
        let servidor = MockServer::start().await;
        montar(
            &servidor,
            "SubscribeToTask",
            ResponseTemplate::new(200).set_body_string("hola"),
        )
        .await;

        let error = cliente(&servidor, None)
            .with_streaming(true)
            .watch("t-1")
            .await
            .err()
            .unwrap();

        assert!(matches!(error, RemoteTaskError::Other(_)), "{error:?}");
    }

    #[tokio::test]
    async fn events_of_another_task_out_of_order_or_with_a_wrong_id_cut_the_stream() {
        let ajena =
            json!({ "task": { "id": "otra", "status": { "state": "TASK_STATE_COMPLETED" } } });
        let casos = [
            // La tarea de otro: nunca se da por buena.
            sobre(ajena),
            // Un estado antes de que llegue la tarea.
            sobre(estado_wire("TASK_STATE_COMPLETED")),
            // Un mensaje en un flujo de tarea.
            sobre(json!({ "message": { "messageId": "m", "role": "ROLE_AGENT",
                                       "parts": [{ "text": "x" }] } })),
            // Una respuesta a otra petición.
            format!(
                "data: {}\n\n",
                json!({ "jsonrpc": "2.0", "id": 99, "result": tarea_wire("TASK_STATE_WORKING") })
            ),
            // Una carga vacía y un estado desconocido.
            sobre(json!({})),
            sobre(tarea_wire("TASK_STATE_INVENTADO")),
            // JSON roto.
            "data: {no es json\n\n".to_string(),
        ];
        for cuerpo in casos {
            let servidor = MockServer::start().await;
            montar(&servidor, "SubscribeToTask", flujo_sse(cuerpo.clone())).await;

            let tareas = recoger(&cliente(&servidor, None).with_streaming(true)).await;

            assert_eq!(tareas.len(), 1, "{cuerpo}");
            assert!(tareas[0].is_err(), "{cuerpo}: {:?}", tareas[0]);
        }
    }

    #[tokio::test]
    async fn an_error_event_in_the_stream_is_translated_and_ends_it() {
        let servidor = MockServer::start().await;
        let cuerpo = [
            sobre(tarea_wire("TASK_STATE_WORKING")),
            format!(
                "data: {}\n\n",
                json!({ "jsonrpc": "2.0", "id": 1,
                        "error": { "code": -32001, "message": "task not found" } })
            ),
            sobre(estado_wire("TASK_STATE_COMPLETED")),
        ]
        .concat();
        montar(&servidor, "SubscribeToTask", flujo_sse(cuerpo)).await;

        let tareas = recoger(&cliente(&servidor, None).with_streaming(true)).await;

        assert_eq!(tareas.len(), 2);
        assert!(tareas[0].is_ok());
        assert_eq!(tareas[1], Err(RemoteTaskError::NotFound));
    }

    #[tokio::test]
    async fn an_oversized_line_or_result_cuts_the_stream() {
        let servidor = MockServer::start().await;
        let gigante = format!("data: {}\n\n", "x".repeat(crate::sse::MAX_LINEA + 10));
        montar(&servidor, "SubscribeToTask", flujo_sse(gigante)).await;
        let tareas = recoger(&cliente(&servidor, None).with_streaming(true)).await;
        assert_eq!(tareas.len(), 1);
        assert!(tareas[0].is_err());

        // El resultado acumulado no pasa del tope de resultado del cliente (64 KiB en las pruebas).
        let servidor = MockServer::start().await;
        let cuerpo = [
            sobre(tarea_wire("TASK_STATE_WORKING")),
            sobre(artefacto_wire("a1", &"y".repeat(40_000), false)),
            sobre(artefacto_wire("a2", &"y".repeat(40_000), false)),
        ]
        .concat();
        montar(&servidor, "SubscribeToTask", flujo_sse(cuerpo)).await;
        let tareas = recoger(&cliente(&servidor, None).with_streaming(true)).await;
        assert_eq!(tareas.len(), 3);
        assert!(tareas[1].is_ok());
        assert!(tareas[2].is_err());
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

    const SECRETO: &str = "s3cr3t0-muy-largo";

    fn tarjeta_segura(nombre: &str, esquema: Value) -> Value {
        json!({
            "securitySchemes": { nombre: esquema },
            "securityRequirements": [{ "schemes": { nombre: { "list": [] } } }],
        })
    }

    fn plan(nombre: &str, esquema: Value, valor: &'static str) -> AuthPlan {
        let seguridad = CardSecurity::from_card_json(&tarjeta_segura(nombre, esquema));
        let credenciales = BTreeMap::from([(nombre.to_string(), "V".to_string())]);
        AuthPlan::choose(&seguridad, &credenciales, &|v| {
            (v == "V").then(|| valor.to_string())
        })
        .unwrap()
    }

    async fn enviar_con(
        servidor: &MockServer,
        auth: AuthPlan,
    ) -> Result<RemoteReply, RemoteTaskError> {
        cliente(servidor, None)
            .with_auth(auth)
            .send(Message::user_text("hola"))
            .await
    }

    fn respuesta_de_envio() -> ResponseTemplate {
        let valor: Value = serde_json::from_str(ENVIO).unwrap();
        ResponseTemplate::new(200).set_body_json(con_id_1(valor))
    }

    #[tokio::test]
    async fn sends_the_api_key_header() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(header("x-api-key", SECRETO))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;
        let auth = plan(
            "claveApi",
            json!({ "apiKeySecurityScheme": { "location": "header", "name": "X-Api-Key" } }),
            SECRETO,
        );

        let respuesta = enviar_con(&servidor, auth).await.unwrap();

        assert!(matches!(respuesta, RemoteReply::Task(_)));
    }

    #[tokio::test]
    async fn sends_the_api_key_in_the_query() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(query_param("key", SECRETO))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;
        let auth = plan(
            "claveApi",
            json!({ "apiKeySecurityScheme": { "location": "query", "name": "key" } }),
            SECRETO,
        );

        assert!(enviar_con(&servidor, auth).await.is_ok());
    }

    /// Contesta al envío copiando el `id` de la petición (varias llamadas seguidas llevan `id` distinto).
    struct EnvioConId;

    impl wiremock::Respond for EnvioConId {
        fn respond(&self, peticion: &wiremock::Request) -> ResponseTemplate {
            let cuerpo: Value = serde_json::from_slice(&peticion.body).unwrap();
            let mut valor: Value = serde_json::from_str(ENVIO).unwrap();
            valor["id"] = cuerpo["id"].clone();
            ResponseTemplate::new(200).set_body_json(valor)
        }
    }

    fn esquema_oauth(servidor: &MockServer) -> Value {
        json!({ "oauth2SecurityScheme": { "flows": { "clientCredentials": {
            "tokenUrl": format!("{}/token", servidor.uri()),
            "scopes": { "leer": "leer" }
        } } } })
    }

    fn token(valor: &str) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(
            json!({ "access_token": valor, "token_type": "Bearer", "expires_in": 3600 }),
        )
    }

    #[tokio::test]
    async fn an_oauth2_remote_is_called_with_a_token_obtained_from_the_card_token_url() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token("tok-1"))
            .expect(1)
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(header("authorization", "Bearer tok-1"))
            .respond_with(EnvioConId)
            .expect(2)
            .mount(&servidor)
            .await;
        let auth = plan("oa", esquema_oauth(&servidor), "cliente:secreto");
        let c = cliente(&servidor, None).with_auth(auth);

        // Dos llamadas, un solo token: el segundo sale de la caché.
        c.send(Message::user_text("a")).await.unwrap();
        c.send(Message::user_text("b")).await.unwrap();

        servidor.verify().await;
    }

    #[tokio::test]
    async fn a_rejected_token_is_renewed_once_and_the_call_retried() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token("tok-1"))
            .up_to_n_times(1)
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token("tok-2"))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(header("authorization", "Bearer tok-1"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(header("authorization", "Bearer tok-2"))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;
        let auth = plan("oa", esquema_oauth(&servidor), "cliente:secreto");

        assert!(enviar_con(&servidor, auth).await.is_ok());
    }

    #[tokio::test]
    async fn a_remote_that_keeps_rejecting_the_token_fails_after_one_retry_only() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(token("tok"))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .respond_with(ResponseTemplate::new(401).set_body_string("tok cliente:secreto"))
            .expect(2)
            .mount(&servidor)
            .await;
        let auth = plan("oa", esquema_oauth(&servidor), "cliente:secreto");

        let error = enviar_con(&servidor, auth).await.unwrap_err();

        assert!(matches!(error, RemoteTaskError::AuthFailed(_)), "{error:?}");
        assert!(!error.to_string().contains("secreto"));
        servidor.verify().await;
    }

    #[tokio::test]
    async fn when_no_token_can_be_obtained_the_call_is_never_sent_without_credentials() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&servidor)
            .await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .respond_with(respuesta_de_envio())
            .expect(0)
            .mount(&servidor)
            .await;
        let auth = plan("oa", esquema_oauth(&servidor), "cliente:secreto");

        let error = enviar_con(&servidor, auth).await.unwrap_err();

        assert!(matches!(error, RemoteTaskError::AuthFailed(_)), "{error:?}");
        servidor.verify().await;
    }

    #[tokio::test]
    async fn sends_bearer_and_basic() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header(
                "authorization",
                format!("Bearer {SECRETO}").as_str(),
            ))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;
        let bearer = plan(
            "t",
            json!({ "httpAuthSecurityScheme": { "scheme": "Bearer" } }),
            SECRETO,
        );
        assert!(enviar_con(&servidor, bearer).await.is_ok());

        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header("authorization", "Basic dXN1YXJpbzpjbGF2ZQ=="))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;
        let basic = plan(
            "b",
            json!({ "httpAuthSecurityScheme": { "scheme": "Basic" } }),
            "usuario:clave",
        );
        assert!(enviar_con(&servidor, basic).await.is_ok());
    }

    #[tokio::test]
    async fn an_open_remote_gets_no_credential_at_all() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;

        cliente(&servidor, None)
            .send(Message::user_text("hola"))
            .await
            .unwrap();

        let recibidas = servidor.received_requests().await.unwrap();
        assert_eq!(recibidas.len(), 1);
        assert!(recibidas[0].headers.get("authorization").is_none());
        assert!(recibidas[0].url.query().is_none());
    }

    #[tokio::test]
    async fn a_401_or_403_is_an_auth_failure_that_never_repeats_the_credential() {
        for estado in [401, 403] {
            let servidor = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(
                    ResponseTemplate::new(estado)
                        .set_body_string(format!("token inválido: {SECRETO}")),
                )
                .mount(&servidor)
                .await;
            let auth = plan(
                "t",
                json!({ "httpAuthSecurityScheme": { "scheme": "Bearer" } }),
                SECRETO,
            );

            let error = enviar_con(&servidor, auth).await.unwrap_err();

            assert!(
                matches!(error, RemoteTaskError::AuthFailed(_)),
                "{estado}: {error:?}"
            );
            assert!(!error.to_string().contains(SECRETO), "{error}");
            assert!(!format!("{error:?}").contains(SECRETO), "{error:?}");
        }
    }

    fn remoto_con_seguridad(servidor: &MockServer) -> RemoteSpec {
        let seguridad = CardSecurity::from_card_json(&tarjeta_segura(
            "claveApi",
            json!({ "apiKeySecurityScheme": { "location": "header", "name": "X-Api-Key" } }),
        ));
        RemoteSpec {
            endpoint: servidor.uri(),
            fetched: Some(RemoteFetched {
                card_hash: "h".into(),
                jsonrpc_url: format!("{}/a2a/v1", servidor.uri()),
                tenant: None,
                requires_auth: true,
                security: seguridad.to_json(),
                auth_problem: None,
                streaming: false,
            }),
            credentials: BTreeMap::from([("claveApi".to_string(), "AGENTE_KEY".to_string())]),
        }
    }

    fn fabrica(valor: Option<&'static str>) -> A2aClientFactory {
        A2aClientFactory::from_config(&A2aConfig {
            role: A2aRole::Consume,
            ..A2aConfig::default()
        })
        .unwrap()
        .unwrap()
        .with_env(move |variable| {
            if variable == "AGENTE_KEY" {
                valor.map(str::to_string)
            } else {
                None
            }
        })
    }

    #[tokio::test]
    async fn the_factory_names_the_missing_variable_and_builds_no_client_without_it() {
        let servidor = MockServer::start().await;
        let spec = remoto_con_seguridad(&servidor);
        let sin = fabrica(None);

        let motivo = sin.unavailable_reason("lejano", &spec).unwrap();

        assert!(motivo.contains("AGENTE_KEY"), "{motivo}");
        assert!(sin.client_for("lejano", &spec).is_none());
    }

    #[tokio::test]
    async fn the_factory_builds_an_authenticated_client_when_the_variable_is_set() {
        let servidor = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/a2a/v1"))
            .and(header("x-api-key", SECRETO))
            .respond_with(respuesta_de_envio())
            .mount(&servidor)
            .await;
        let spec = remoto_con_seguridad(&servidor);
        let con = fabrica(Some(SECRETO));

        assert!(con.unavailable_reason("lejano", &spec).is_none());
        let cliente = con.client_for("lejano", &spec).unwrap();

        assert!(cliente.send(Message::user_text("hola")).await.is_ok());
    }

    fn remoto_mtls(servidor: &MockServer) -> RemoteSpec {
        let seguridad = CardSecurity::from_card_json(&tarjeta_segura(
            "tls",
            json!({ "mtlsSecurityScheme": {} }),
        ));
        RemoteSpec {
            endpoint: servidor.uri(),
            fetched: Some(RemoteFetched {
                card_hash: "h".into(),
                jsonrpc_url: format!("{}/a2a/v1", servidor.uri()),
                tenant: None,
                requires_auth: true,
                security: seguridad.to_json(),
                auth_problem: None,
                streaming: false,
            }),
            credentials: BTreeMap::from([("tls".to_string(), "AGENTE_TLS".to_string())]),
        }
    }

    fn fabrica_con_ruta(ruta: &'static str) -> A2aClientFactory {
        A2aClientFactory::from_config(&A2aConfig {
            role: A2aRole::Consume,
            ..A2aConfig::default()
        })
        .unwrap()
        .unwrap()
        .with_env(move |variable| (variable == "AGENTE_TLS").then(|| ruta.to_string()))
    }

    #[tokio::test]
    async fn the_factory_builds_a_client_with_its_own_identity_for_an_mtls_remote() {
        let servidor = MockServer::start().await;
        let spec = remoto_mtls(&servidor);
        let ruta = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/mtls-test-identity.pem"
        );

        let con = fabrica_con_ruta(ruta);

        assert!(con.unavailable_reason("lejano", &spec).is_none());
        assert!(con.client_for("lejano", &spec).is_some());
    }

    #[tokio::test]
    async fn an_mtls_remote_without_a_usable_identity_has_no_client_and_says_why() {
        let servidor = MockServer::start().await;
        let spec = remoto_mtls(&servidor);

        let sin = fabrica_con_ruta("/no/existe.pem");

        let motivo = sin.unavailable_reason("lejano", &spec).unwrap();
        assert!(
            motivo.contains("identidad TLS") && motivo.contains("AGENTE_TLS"),
            "{motivo}"
        );
        assert!(sin.client_for("lejano", &spec).is_none());
    }

    #[tokio::test]
    async fn the_factory_refuses_a_remote_whose_security_it_cannot_read() {
        let servidor = MockServer::start().await;
        let mut spec = remoto_con_seguridad(&servidor);
        spec.fetched.as_mut().unwrap().security = None;

        let motivo = fabrica(Some(SECRETO))
            .unavailable_reason("lejano", &spec)
            .unwrap();

        assert!(motivo.contains("demasiado grande"), "{motivo}");
    }
}
