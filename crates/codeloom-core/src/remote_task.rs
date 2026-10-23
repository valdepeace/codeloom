//! Contrato mínimo con un agente remoto: lo que el espejo del lanzador necesita (A8a).
//!
//! Cabe señalar que el núcleo no conoce el cable ni HTTP: define aquí lo que pide a un remoto
//! (enviar un mensaje, consultar una tarea, cancelarla) y el crate de A2A lo implementa. Las
//! tareas remotas se identifican por el texto que el remoto asigna, no por una `SessionId`.

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;

use crate::a2a::{Artifact, Message};
use crate::registry::RemoteSpec;
use crate::task::TaskStatus;

/// Tarea tal como la describe el remoto.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteTask {
    pub id: String,
    pub context_id: Option<String>,
    pub status: TaskStatus,
    pub artifacts: Vec<Artifact>,
}

/// Respuesta a un envío: el remoto contesta con una tarea o, directamente, con un mensaje.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteReply {
    Task(RemoteTask),
    Message(Message),
}

/// Fallos al hablar con un remoto. Los cuatro primeros equivalen a errores de A2A.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteTaskError {
    #[error("remote task not found")]
    NotFound,
    #[error("remote task is not cancelable")]
    NotCancelable,
    #[error("remote does not support this operation: {0}")]
    Unsupported(String),
    #[error("remote request timed out")]
    Timeout,
    /// El remoto rechazó las credenciales (HTTP 401 o 403). El texto no repite la credencial.
    #[error("remote rejected the credentials: {0}")]
    AuthFailed(String),
    #[error("remote error: {0}")]
    Other(String),
}

/// Construye el cliente de un agente remoto del registro. Lo implementa el crate de A2A; sin
/// fábrica configurada, el lanzador no ejecuta remotos.
pub trait RemoteClientFactory: Send + Sync {
    /// Cliente del remoto `name`, o `None` si no se puede construir (p. ej. sin tarjeta descargada).
    fn client_for(&self, name: &str, spec: &RemoteSpec) -> Option<Arc<dyn RemoteTaskClient>>;

    /// Motivo, ya redactado, por el que el remoto no se puede usar aunque haya cliente (p. ej. le
    /// falta una credencial). `None` si se puede usar. Se consulta justo antes de delegar, de modo
    /// que una variable de entorno recién definida se nota sin reiniciar.
    fn unavailable_reason(&self, _name: &str, _spec: &RemoteSpec) -> Option<String> {
        None
    }
}

/// Cliente de un agente remoto concreto (un punto de entrada y una interfaz ya elegida).
#[async_trait]
pub trait RemoteTaskClient: Send + Sync {
    /// Envía un mensaje y vuelve sin esperar a que la tarea termine. Con `task_id` en el mensaje
    /// continúa esa tarea (p. ej. la responde si pedía entrada).
    async fn send(&self, message: Message) -> Result<RemoteReply, RemoteTaskError>;

    /// Estado actual de la tarea `id`, sin historial.
    async fn get(&self, id: &str) -> Result<RemoteTask, RemoteTaskError>;

    /// Pide cancelar la tarea `id`. Devuelve lo que el remoto dice que ha pasado: quien llama no
    /// debe dar por cancelada una tarea cuyo estado devuelto no lo sea.
    async fn cancel(&self, id: &str) -> Result<RemoteTask, RemoteTaskError>;

    /// Sigue la tarea `id` por flujo (A8c). Cada elemento es la tarea completa tras aplicar un evento
    /// del remoto; el flujo empieza por la tarea actual y se cierra al estado terminal. Un error en el
    /// flujo es el último elemento. Quien llama debe volver al sondeo (`get`) ante `Err` al abrir, un
    /// error en el flujo o un cierre antes de un estado terminal: el seguimiento nunca se pierde.
    async fn watch(
        &self,
        _id: &str,
    ) -> Result<BoxStream<'static, Result<RemoteTask, RemoteTaskError>>, RemoteTaskError> {
        Err(RemoteTaskError::Unsupported(
            "task streaming is not available".into(),
        ))
    }
}
