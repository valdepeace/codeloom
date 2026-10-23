//! Servir CodeLoom como agente A2A (A8b).
//!
//! Cabe señalar que servir invierte el riesgo de consumir: es un tercero quien dicta a un modelo
//! local lo que ha de hacer, sin persona que apruebe permisos. Por eso la configuración
//! (`config`) valida primero que haya autenticación y que se escuche solo en bucle local, y el
//! resto del servidor se construye sobre esa base.

pub mod auth;
pub mod card;
pub mod config;
pub mod confine;
pub mod host;
pub mod launch;
pub mod rpc;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

/// Enlaza `addr`, atiende el enrutador en segundo plano y devuelve la dirección real (útil con
/// el puerto 0 en las pruebas). El servidor se detiene en cuanto `apagado` se cancela.
pub async fn serve(
    estado: Arc<rpc::ServeState>,
    addr: SocketAddr,
    apagado: CancellationToken,
) -> std::io::Result<SocketAddr> {
    let escucha = tokio::net::TcpListener::bind(addr).await?;
    let real = escucha.local_addr()?;
    let app = rpc::router(estado);
    tokio::spawn(async move {
        // Un fallo de E/S al atender ya no tiene a quién avisar: el proceso sigue y el cliente
        // verá la conexión cerrada.
        let _ = axum::serve(escucha, app)
            .with_graceful_shutdown(async move { apagado.cancelled().await })
            .await;
    });
    Ok(real)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::auth::Authenticator;
    use crate::serve::card::served_card;
    use crate::serve::config::{ServeAuthConfig, ServeAuthKind, ServeConfig};
    use crate::serve::rpc::{HostError, ServeState, TaskHost, TaskView};
    use async_trait::async_trait;
    use codeloom_core::card::AgentCard;
    use std::time::Duration;

    /// Anfitrión que no atiende nada: aquí solo se prueba el enlace y la parada.
    struct SinTareas;

    #[async_trait]
    impl TaskHost for SinTareas {
        async fn start(&self, _prompt: String) -> Result<TaskView, HostError> {
            Err(HostError::Busy)
        }
        async fn get(&self, _id: &str) -> Result<TaskView, HostError> {
            Err(HostError::NotFound)
        }
        async fn cancel(&self, _id: &str) -> Result<TaskView, HostError> {
            Err(HostError::NotFound)
        }
        async fn wait_terminal(&self, _id: &str, _max: Duration) -> Result<TaskView, HostError> {
            Err(HostError::NotFound)
        }
    }

    fn estado() -> Arc<rpc::ServeState> {
        let config = ServeConfig {
            auth: Some(ServeAuthConfig {
                scheme: ServeAuthKind::Bearer,
                name: None,
                env: "TOKEN_LOOM".into(),
            }),
            ..ServeConfig::default()
        };
        let auth = config
            .validate(&|_| Some("secreto".to_string()))
            .expect("la configuración de prueba es válida");
        let tarjeta = served_card(
            &AgentCard::local("default", "agente de prueba", "1"),
            "http://127.0.0.1:0/a2a",
            &auth.scheme,
        );
        Arc::new(ServeState {
            host: Arc::new(SinTareas),
            auth: Authenticator::new(auth),
            card: tarjeta,
            max_wait: Duration::from_secs(1),
        })
    }

    #[tokio::test]
    async fn serve_binds_the_loopback_port_answers_and_stops_on_shutdown() {
        let apagado = CancellationToken::new();
        let addr = serve(estado(), "127.0.0.1:0".parse().unwrap(), apagado.clone())
            .await
            .unwrap();
        assert!(addr.ip().is_loopback());
        assert_ne!(addr.port(), 0);

        let url = format!("http://{addr}/.well-known/agent-card.json");
        let respuesta = reqwest::get(&url).await.unwrap();
        assert_eq!(respuesta.status(), 200);
        let tarjeta: serde_json::Value = respuesta.json().await.unwrap();
        assert_eq!(tarjeta["name"], "default");

        // Sin credencial, el JSON-RPC no se atiende.
        let cliente = reqwest::Client::new();
        let sin_credencial = cliente
            .post(format!("http://{addr}/a2a"))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(sin_credencial.status(), 401);

        apagado.cancel();
        let mut cerrado = false;
        for _ in 0..50 {
            if reqwest::get(&url).await.is_err() {
                cerrado = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            cerrado,
            "el servidor debía dejar de atender tras el apagado"
        );
    }

    #[tokio::test]
    async fn serve_reports_a_port_already_in_use() {
        let ocupado = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = ocupado.local_addr().unwrap();

        let error = serve(estado(), addr, CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
    }
}
