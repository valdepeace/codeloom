//! Acceso a un agente remoto: cliente HTTP acotado y descarga de su tarjeta (A8a, T2).
//!
//! Cabe señalar que todo lo que llega de un remoto es dato de un tercero: se pide con tiempo
//! máximo, se lee con tope de tamaño y no se sigue una redirección a otro anfitrión. Sin el rol
//! `consume` no se construye ningún cliente, de modo que no se abre ni un socket.

use std::time::Duration;

use codeloom_core::card::AgentCard;
use codeloom_core::config::validate_base_url;
use codeloom_core::registry::content_hash;
use reqwest::redirect::Policy;
use reqwest::{Client, Response, Url};

use crate::card::{WireAgentCard, VERSION_SOPORTADA};
use crate::config::A2aConfig;
use crate::security::CardSecurity;

/// Ruta de la tarjeta (§8.2).
const RUTA_TARJETA: &str = "/.well-known/agent-card.json";
/// Redirecciones máximas dentro del mismo anfitrión.
const MAX_REDIRECCIONES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoteError {
    #[error("invalid remote endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("remote request failed: {0}")]
    Network(String),
    #[error("remote request timed out")]
    Timeout,
    #[error("remote answered with HTTP status {0}")]
    Status(u16),
    #[error("remote redirected the request to another host; refused")]
    Redirect,
    #[error("remote response exceeds {max} bytes")]
    TooLarge { max: usize },
    #[error("remote card is not valid: {0}")]
    InvalidCard(String),
    #[error("remote speaks no A2A version this client implements (it offers: {})", .0.join(", "))]
    UnsupportedVersion(Vec<String>),
    #[error("the card sends tasks to another host ({0}); refused")]
    InterfaceElsewhere(String),
}

/// Tarjeta remota ya descargada, acotada y validada.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteCard {
    pub card: AgentCard,
    /// Huella del contenido (JSON normalizado): la aprobación se ata a ella.
    pub hash: String,
    /// URL de la interfaz JSON-RPC elegida, del mismo anfitrión que el punto de entrada.
    pub jsonrpc_url: String,
    pub tenant: Option<String>,
    /// La tarjeta declara requisitos de seguridad.
    pub requires_auth: bool,
    /// Esos requisitos en la forma opaca que guarda el núcleo (`CardSecurity::to_json`). `None` si
    /// no hay, o si pasan del tope y no se pueden guardar (en ese caso `requires_auth` es `true`).
    pub security: Option<String>,
}

fn mismo_origen(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn politica_de_redireccion() -> Policy {
    Policy::custom(|intento| {
        if intento.previous().len() >= MAX_REDIRECCIONES {
            return intento.error("too many redirects");
        }
        let mismo = intento
            .previous()
            .last()
            .is_some_and(|anterior| mismo_origen(anterior, intento.url()));
        if mismo {
            intento.follow()
        } else {
            // Se devuelve la propia respuesta 3xx: quien llama la reporta como `Redirect`.
            intento.stop()
        }
    })
}

/// Cliente HTTP para hablar con remotos. `None` si el rol no es de consumo: con A2A apagado no se
/// construye nada.
pub fn http_client(config: &A2aConfig) -> Result<Option<Client>, RemoteError> {
    if !config.consumes() {
        return Ok(None);
    }
    Client::builder()
        .timeout(Duration::from_secs(config.request_timeout_secs))
        .redirect(politica_de_redireccion())
        .user_agent(concat!("codeloom-a2a/", env!("CARGO_PKG_VERSION")))
        .build()
        .map(Some)
        .map_err(|e| RemoteError::Network(e.without_url().to_string()))
}

pub(crate) fn mapear(error: reqwest::Error) -> RemoteError {
    if error.is_timeout() {
        RemoteError::Timeout
    } else {
        RemoteError::Network(error.without_url().to_string())
    }
}

/// Lee el cuerpo sin pasar de `max` bytes: se corta en cuanto se supera, sin acumularlo.
pub(crate) async fn leer_acotado(
    mut respuesta: Response,
    max: usize,
) -> Result<Vec<u8>, RemoteError> {
    if respuesta
        .content_length()
        .is_some_and(|longitud| longitud > max as u64)
    {
        return Err(RemoteError::TooLarge { max });
    }
    let mut cuerpo = Vec::new();
    while let Some(trozo) = respuesta.chunk().await.map_err(mapear)? {
        if cuerpo.len() + trozo.len() > max {
            return Err(RemoteError::TooLarge { max });
        }
        cuerpo.extend_from_slice(&trozo);
    }
    Ok(cuerpo)
}

/// Descarga y valida la tarjeta de `endpoint` (la raíz del remoto, `https://…` o bucle local).
pub async fn fetch_card(
    client: &Client,
    endpoint: &str,
    max_card_bytes: usize,
) -> Result<RemoteCard, RemoteError> {
    validate_base_url(endpoint).map_err(RemoteError::InvalidEndpoint)?;
    let origen = Url::parse(endpoint).map_err(|e| RemoteError::InvalidEndpoint(e.to_string()))?;
    let destino = origen
        .join(RUTA_TARJETA)
        .map_err(|e| RemoteError::InvalidEndpoint(e.to_string()))?;

    let respuesta = client
        .get(destino)
        .header("A2A-Version", VERSION_SOPORTADA)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(mapear)?;
    let estado = respuesta.status();
    if estado.is_redirection() {
        return Err(RemoteError::Redirect);
    }
    if !estado.is_success() {
        return Err(RemoteError::Status(estado.as_u16()));
    }

    let cuerpo = leer_acotado(respuesta, max_card_bytes).await?;
    let valor: serde_json::Value =
        serde_json::from_slice(&cuerpo).map_err(|e| RemoteError::InvalidCard(e.to_string()))?;
    // El JSON normalizado (claves ordenadas, sin espacios) da una huella que no cambia porque el
    // remoto reformatee la tarjeta, y sí cuando cambia su contenido.
    let hash = content_hash(&valor.to_string());
    let seguridad = CardSecurity::from_card_json(&valor);
    let tarjeta: WireAgentCard =
        serde_json::from_value(valor).map_err(|e| RemoteError::InvalidCard(e.to_string()))?;

    let interfaz = tarjeta
        .interfaz_soportada()
        .ok_or_else(|| RemoteError::UnsupportedVersion(tarjeta.ofrecidas()))?;
    validate_base_url(&interfaz.url)
        .map_err(|m| RemoteError::InvalidCard(format!("interface url: {m}")))?;
    let url_interfaz = Url::parse(&interfaz.url)
        .map_err(|e| RemoteError::InvalidCard(format!("interface url: {e}")))?;
    if url_interfaz.host_str() != origen.host_str()
        || url_interfaz.port_or_known_default() != origen.port_or_known_default()
    {
        return Err(RemoteError::InterfaceElsewhere(
            url_interfaz.host_str().unwrap_or("?").to_string(),
        ));
    }

    Ok(RemoteCard {
        card: tarjeta.into_core(),
        hash,
        jsonrpc_url: interfaz.url.clone(),
        tenant: interfaz.tenant.clone(),
        requires_auth: tarjeta.requires_auth(),
        security: seguridad.to_json(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::A2aRole;
    use serde_json::{json, Value};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TARJETA: &str = include_str!("../tests/fixtures/agent-card.sample.json");

    fn config(segundos: u64) -> A2aConfig {
        A2aConfig {
            role: A2aRole::Consume,
            request_timeout_secs: segundos,
            ..A2aConfig::default()
        }
    }

    fn cliente() -> Client {
        http_client(&config(2)).unwrap().unwrap()
    }

    /// La tarjeta de la spec con sus interfaces apuntando al servidor de prueba.
    fn tarjeta_para(servidor: &MockServer) -> Value {
        let mut valor: Value = serde_json::from_str(TARJETA).unwrap();
        for interfaz in valor["supportedInterfaces"].as_array_mut().unwrap() {
            interfaz["url"] = json!(format!("{}/a2a/v1", servidor.uri()));
        }
        valor
    }

    async fn servir(servidor: &MockServer, cuerpo: Value) {
        Mock::given(method("GET"))
            .and(path(RUTA_TARJETA))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo))
            .mount(servidor)
            .await;
    }

    #[test]
    fn with_the_role_off_no_client_is_built() {
        assert!(http_client(&A2aConfig::default()).unwrap().is_none());
        assert!(http_client(&config(5)).unwrap().is_some());
    }

    #[tokio::test]
    async fn a_valid_card_is_fetched_converted_and_hashed() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_para(&servidor)).await;

        let remota = fetch_card(&cliente(), &servidor.uri(), 64 * 1024)
            .await
            .unwrap();

        assert_eq!(remota.card.name, "GeoSpatial Route Planner Agent");
        assert_eq!(remota.jsonrpc_url, format!("{}/a2a/v1", servidor.uri()));
        assert_eq!(remota.hash.len(), 64);
        assert!(remota.requires_auth);
    }

    #[tokio::test]
    async fn a_changed_card_changes_the_hash() {
        let servidor = MockServer::start().await;
        let tarjeta = tarjeta_para(&servidor);
        servir(&servidor, tarjeta.clone()).await;
        let antes = fetch_card(&cliente(), &servidor.uri(), 64 * 1024)
            .await
            .unwrap();

        servidor.reset().await;
        let mut cambiada = tarjeta;
        cambiada["description"] = json!("otra cosa");
        servir(&servidor, cambiada).await;
        let despues = fetch_card(&cliente(), &servidor.uri(), 64 * 1024)
            .await
            .unwrap();

        assert_ne!(antes.hash, despues.hash);
    }

    #[tokio::test]
    async fn reformatting_the_same_card_keeps_the_hash() {
        let servidor = MockServer::start().await;
        let tarjeta = tarjeta_para(&servidor);
        servir(&servidor, tarjeta.clone()).await;
        let compacta = fetch_card(&cliente(), &servidor.uri(), 64 * 1024)
            .await
            .unwrap();

        servidor.reset().await;
        Mock::given(method("GET"))
            .and(path(RUTA_TARJETA))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                serde_json::to_vec_pretty(&tarjeta).unwrap(),
                "application/json",
            ))
            .mount(&servidor)
            .await;
        let bonita = fetch_card(&cliente(), &servidor.uri(), 64 * 1024)
            .await
            .unwrap();

        assert_eq!(compacta.hash, bonita.hash);
    }

    #[tokio::test]
    async fn a_card_larger_than_the_cap_is_refused() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_para(&servidor)).await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 100).await;

        assert_eq!(resultado, Err(RemoteError::TooLarge { max: 100 }));
    }

    #[tokio::test]
    async fn a_body_that_is_not_json_is_an_invalid_card() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(RUTA_TARJETA))
            .respond_with(ResponseTemplate::new(200).set_body_string("{ esto no es json"))
            .mount(&servidor)
            .await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 64 * 1024).await;

        assert!(
            matches!(resultado, Err(RemoteError::InvalidCard(_))),
            "{resultado:?}"
        );
    }

    #[tokio::test]
    async fn a_redirect_to_another_host_is_refused_and_not_followed() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(RUTA_TARJETA))
            .respond_with(ResponseTemplate::new(302).insert_header(
                "location",
                "http://otro.invalid/.well-known/agent-card.json",
            ))
            .mount(&servidor)
            .await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 64 * 1024).await;

        assert_eq!(resultado, Err(RemoteError::Redirect));
    }

    #[tokio::test]
    async fn a_redirect_within_the_same_host_is_followed() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(RUTA_TARJETA))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/movida.json"))
            .mount(&servidor)
            .await;
        Mock::given(method("GET"))
            .and(path("/movida.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(tarjeta_para(&servidor)))
            .mount(&servidor)
            .await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 64 * 1024).await;

        assert!(resultado.is_ok(), "{resultado:?}");
    }

    #[tokio::test]
    async fn a_card_offering_only_an_unsupported_version_is_a_visible_error() {
        let servidor = MockServer::start().await;
        let mut tarjeta = tarjeta_para(&servidor);
        for interfaz in tarjeta["supportedInterfaces"].as_array_mut().unwrap() {
            interfaz["protocolVersion"] = json!("0.3");
        }
        servir(&servidor, tarjeta).await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 64 * 1024).await;

        match resultado {
            Err(RemoteError::UnsupportedVersion(ofrecidas)) => {
                assert!(
                    ofrecidas.contains(&"JSONRPC 0.3".to_string()),
                    "{ofrecidas:?}"
                );
            }
            otro => panic!("se esperaba UnsupportedVersion, y salió {otro:?}"),
        }
    }

    #[tokio::test]
    async fn a_card_pointing_tasks_to_another_host_is_refused() {
        let servidor = MockServer::start().await;
        // La tarjeta de la spec trae sus URL en `georoute-agent.example.com`, no en este servidor.
        servir(&servidor, serde_json::from_str(TARJETA).unwrap()).await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 64 * 1024).await;

        assert_eq!(
            resultado,
            Err(RemoteError::InterfaceElsewhere(
                "georoute-agent.example.com".into()
            ))
        );
    }

    #[tokio::test]
    async fn a_missing_card_reports_the_http_status() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&servidor)
            .await;

        let resultado = fetch_card(&cliente(), &servidor.uri(), 64 * 1024).await;

        assert_eq!(resultado, Err(RemoteError::Status(404)));
    }

    #[tokio::test]
    async fn a_remote_that_does_not_answer_in_time_times_out() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(tarjeta_para(&servidor))
                    .set_delay(Duration::from_secs(4)),
            )
            .mount(&servidor)
            .await;
        let cliente = http_client(&config(1)).unwrap().unwrap();

        let resultado = fetch_card(&cliente, &servidor.uri(), 64 * 1024).await;

        assert_eq!(resultado, Err(RemoteError::Timeout));
    }

    #[tokio::test]
    async fn a_plain_http_endpoint_outside_loopback_is_refused_without_any_request() {
        let resultado = fetch_card(&cliente(), "http://example.com", 64 * 1024).await;

        assert!(
            matches!(resultado, Err(RemoteError::InvalidEndpoint(_))),
            "{resultado:?}"
        );
    }
}
