//! Puesta al día de las tarjetas de los agentes remotos del registro (A8a, T3b).
//!
//! Cabe señalar que el registro del núcleo no sabe de HTTP: guarda el resultado de consultar cada
//! tarjeta y ata la aprobación a su huella. Este módulo es quien hace la consulta. Ni descargar la
//! tarjeta ni cambiarla aprueba nada: la persona usuaria aprueba lo que ha visto.

use std::collections::BTreeMap;

use codeloom_core::event::EventBus;
use codeloom_core::registry::{AgentRegistry, Execution, RemoteFetched};
use futures::stream::{self, StreamExt};
use reqwest::Client;

use crate::config::A2aConfig;
use crate::remote::fetch_card;
use crate::security::AuthPlan;

/// Peticiones de tarjeta simultáneas como máximo.
const CONSULTAS_SIMULTANEAS: usize = 4;

/// Qué remotos se consultaron bien y cuáles no.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefreshReport {
    pub ok: Vec<String>,
    pub failed: Vec<String>,
}

/// Consulta la tarjeta de cada agente remoto de la instantánea vigente y anota el resultado en el
/// registro, que recalcula su instantánea y publica el cambio. Un fallo de un remoto no afecta a
/// los demás: queda como problema de carga de ese agente.
pub async fn refresh_remote_cards(
    registry: &AgentRegistry,
    bus: &EventBus,
    client: &Client,
    config: &A2aConfig,
) -> RefreshReport {
    refresh_remote_cards_with_env(registry, bus, client, config, &|variable| {
        std::env::var(variable).ok()
    })
    .await
}

/// Como [`refresh_remote_cards`], leyendo las credenciales con `env` (inyectable para las pruebas).
/// Además de la tarjeta, anota si el cliente podría autenticarse con lo que el perfil declara.
pub async fn refresh_remote_cards_with_env(
    registry: &AgentRegistry,
    bus: &EventBus,
    client: &Client,
    config: &A2aConfig,
    env: &(dyn Fn(&str) -> Option<String> + Sync),
) -> RefreshReport {
    let remotos: Vec<(String, String, BTreeMap<String, String>, bool)> = registry
        .snapshot()
        .entries()
        .into_iter()
        .filter_map(|entrada| match &entrada.execution {
            Execution::Remote(spec) => Some((
                entrada.name.clone(),
                spec.endpoint.clone(),
                spec.credentials.clone(),
                spec.fetched.is_some(),
            )),
            Execution::Local(_) => None,
        })
        .collect();

    let max_card_bytes = config.max_card_bytes;
    let resultados: Vec<_> = stream::iter(remotos)
        .map(|(nombre, endpoint, credenciales, tenia_buena)| async move {
            let resultado = fetch_card(client, &endpoint, max_card_bytes).await;
            (nombre, endpoint, credenciales, tenia_buena, resultado)
        })
        .buffer_unordered(CONSULTAS_SIMULTANEAS)
        .collect()
        .await;

    let mut informe = RefreshReport::default();
    for (nombre, endpoint, credenciales, tenia_buena, resultado) in resultados {
        let anotado = match resultado {
            Ok(remota) => {
                informe.ok.push(nombre.clone());
                let auth_problem = AuthPlan::for_card(
                    remota.security.as_deref(),
                    remota.requires_auth,
                    &credenciales,
                    env,
                )
                .err();
                let streaming = remota.card.capabilities.streaming == Some(true);
                Ok((
                    remota.card,
                    RemoteFetched {
                        card_hash: remota.hash,
                        jsonrpc_url: remota.jsonrpc_url,
                        tenant: remota.tenant,
                        requires_auth: remota.requires_auth,
                        security: remota.security,
                        auth_problem,
                        streaming,
                    },
                ))
            }
            Err(error) => {
                informe.failed.push(nombre.clone());
                // Un fallo de red no estropea lo que funciona: si ya había una tarjeta buena, se
                // conserva la última y el remoto sigue utilizable. Solo el que nunca se descargó
                // queda como problema de carga.
                if tenia_buena {
                    continue;
                }
                Err(error.to_string())
            }
        };
        registry.set_remote_card(bus, &nombre, &endpoint, anotado);
    }
    informe.ok.sort();
    informe.failed.sort();
    informe
}

/// Refresca las tarjetas cada `intervalo` mientras viva el proceso. La primera pasada es al arrancar
/// (la hace quien llama), así que la espera va primero. Es una tarea para lanzar con `tokio::spawn`.
pub async fn refresh_loop(
    registry: std::sync::Arc<AgentRegistry>,
    bus: EventBus,
    client: Client,
    config: A2aConfig,
    intervalo: std::time::Duration,
) {
    loop {
        tokio::time::sleep(intervalo).await;
        let _ = refresh_remote_cards(&registry, &bus, &client, &config).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::A2aRole;
    use crate::remote::http_client;
    use codeloom_core::registry::{
        Approval, BuiltInSource, FileSource, LoadProblem, ProfileLevel, TrustStore,
    };
    use serde_json::{json, Value};
    use std::path::Path;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TARJETA: &str = include_str!("../tests/fixtures/agent-card.sample.json");

    fn config() -> A2aConfig {
        A2aConfig {
            role: A2aRole::Consume,
            request_timeout_secs: 2,
            ..A2aConfig::default()
        }
    }

    fn tarjeta_para(servidor: &MockServer, descripcion: &str) -> Value {
        let mut valor: Value = serde_json::from_str(TARJETA).unwrap();
        valor["description"] = json!(descripcion);
        for interfaz in valor["supportedInterfaces"].as_array_mut().unwrap() {
            interfaz["url"] = json!(format!("{}/a2a/v1", servidor.uri()));
        }
        valor
    }

    async fn servir(servidor: &MockServer, cuerpo: Value) {
        Mock::given(method("GET"))
            .and(path("/.well-known/agent-card.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(cuerpo))
            .mount(servidor)
            .await;
    }

    fn perfil(dir: &Path, nombre: &str, endpoint: &str) {
        std::fs::write(
            dir.join(format!("{nombre}.md")),
            format!(
                "---\nname: {nombre}\ndescription: Alta manual\ntransport: a2a-remote\na2a:\n  endpoint: {endpoint}\n---\n"
            ),
        )
        .unwrap();
    }

    fn registro(dir: &Path, datos: &Path) -> AgentRegistry {
        AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(dir.to_path_buf(), ProfileLevel::Global)),
        ])
        .with_trust(
            TrustStore::open(datos.to_path_buf()).unwrap(),
            dir.to_path_buf(),
            false,
        )
    }

    #[tokio::test]
    async fn the_card_of_each_remote_is_fetched_and_left_pending_until_approved() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_para(&servidor, "Rutas")).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil(dir.path(), "rutas", &servidor.uri());
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();

        let informe = refresh_remote_cards(&registro, &bus, &cliente, &config()).await;

        assert_eq!(informe.ok, vec!["rutas".to_string()]);
        assert!(informe.failed.is_empty());
        let instantanea = registro.snapshot();
        assert_eq!(
            instantanea.get("rutas").unwrap().card.name,
            "GeoSpatial Route Planner Agent"
        );
        assert_eq!(instantanea.approval("rutas"), Some(Approval::Pending));

        registro.approve("rutas").unwrap();
        assert_eq!(
            registro.snapshot().approval("rutas"),
            Some(Approval::Approved)
        );
    }

    #[tokio::test]
    async fn a_card_that_changes_between_refreshes_returns_to_pending() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_para(&servidor, "Rutas")).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil(dir.path(), "rutas", &servidor.uri());
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();
        refresh_remote_cards(&registro, &bus, &cliente, &config()).await;
        registro.approve("rutas").unwrap();
        assert_eq!(
            registro.snapshot().approval("rutas"),
            Some(Approval::Approved)
        );

        servidor.reset().await;
        servir(&servidor, tarjeta_para(&servidor, "Ahora hago otra cosa")).await;
        refresh_remote_cards(&registro, &bus, &cliente, &config()).await;

        assert_eq!(
            registro.snapshot().approval("rutas"),
            Some(Approval::Pending)
        );
    }

    #[tokio::test]
    async fn a_failed_refresh_keeps_the_last_good_card_and_its_approval() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_para(&servidor, "Rutas")).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil(dir.path(), "rutas", &servidor.uri());
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();
        refresh_remote_cards(&registro, &bus, &cliente, &config()).await;
        registro.approve("rutas").unwrap();

        // El remoto se cae: el refresco falla, pero lo que ya funcionaba sigue funcionando.
        servidor.reset().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&servidor)
            .await;
        let informe = refresh_remote_cards(&registro, &bus, &cliente, &config()).await;

        assert_eq!(informe.failed, vec!["rutas".to_string()]);
        let instantanea = registro.snapshot();
        assert_eq!(instantanea.approval("rutas"), Some(Approval::Approved));
        assert!(instantanea.get("rutas").is_some());
        assert!(!instantanea
            .problems()
            .iter()
            .any(|p| matches!(p, LoadProblem::Remote { name, .. } if name == "rutas")));
    }

    #[tokio::test]
    async fn the_periodic_loop_refreshes_again_and_again() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_para(&servidor, "Rutas")).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil(dir.path(), "rutas", &servidor.uri());
        let registro = std::sync::Arc::new(registro(dir.path(), datos.path()));
        let cliente = http_client(&config()).unwrap().unwrap();

        let tarea = tokio::spawn(refresh_loop(
            registro.clone(),
            EventBus::new(16),
            cliente,
            config(),
            std::time::Duration::from_millis(40),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        tarea.abort();

        let peticiones = servidor.received_requests().await.unwrap().len();
        assert!(peticiones >= 3, "solo {peticiones} peticiones");
        assert!(registro.snapshot().get("rutas").is_some());
    }

    #[tokio::test]
    async fn one_failing_remote_does_not_affect_the_others() {
        let bueno = MockServer::start().await;
        servir(&bueno, tarjeta_para(&bueno, "Bueno")).await;
        let malo = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&malo)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil(dir.path(), "bueno", &bueno.uri());
        perfil(dir.path(), "malo", &malo.uri());
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();

        let informe = refresh_remote_cards(&registro, &bus, &cliente, &config()).await;

        assert_eq!(informe.ok, vec!["bueno".to_string()]);
        assert_eq!(informe.failed, vec!["malo".to_string()]);
        let instantanea = registro.snapshot();
        assert!(instantanea.problems().contains(&LoadProblem::Remote {
            name: "malo".into(),
            error: "remote answered with HTTP status 500".into(),
        }));
        registro.approve("bueno").unwrap();
        assert_eq!(
            registro.snapshot().approval("bueno"),
            Some(Approval::Approved)
        );
        assert_eq!(
            registro.snapshot().approval("malo"),
            Some(Approval::Pending)
        );
    }

    #[tokio::test]
    async fn a_registry_without_remotes_makes_no_request() {
        let servidor = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&servidor)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();

        let informe = refresh_remote_cards(&registro, &bus, &cliente, &config()).await;

        assert_eq!(informe, RefreshReport::default());
    }

    fn perfil_con_credenciales(dir: &Path, nombre: &str, endpoint: &str, esquema: &str) {
        std::fs::write(
            dir.join(format!("{nombre}.md")),
            format!(
                "---\nname: {nombre}\ndescription: Alta manual\ntransport: a2a-remote\na2a:\n  endpoint: {endpoint}\n  credentials:\n    {esquema}: AGENTE_KEY\n---\n"
            ),
        )
        .unwrap();
    }

    fn tarjeta_con_clave_de_api(servidor: &MockServer) -> Value {
        let mut valor = tarjeta_para(servidor, "Rutas");
        valor["securitySchemes"] = json!({
            "claveApi": { "apiKeySecurityScheme": { "location": "header", "name": "X-Api-Key" } }
        });
        valor["securityRequirements"] = json!([{ "schemes": { "claveApi": { "list": [] } } }]);
        valor
    }

    fn problema_de(registro: &AgentRegistry, nombre: &str) -> Option<String> {
        match &registro.snapshot().get(nombre).unwrap().execution {
            Execution::Remote(spec) => spec.fetched.as_ref().unwrap().auth_problem.clone(),
            Execution::Local(_) => panic!("no es remoto"),
        }
    }

    #[tokio::test]
    async fn refresh_records_why_a_secured_remote_cannot_be_used() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_con_clave_de_api(&servidor)).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil_con_credenciales(dir.path(), "rutas", &servidor.uri(), "claveApi");
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();

        refresh_remote_cards_with_env(&registro, &bus, &cliente, &config(), &|_| None).await;
        let sin_variable = problema_de(&registro, "rutas");
        refresh_remote_cards_with_env(&registro, &bus, &cliente, &config(), &|v| {
            (v == "AGENTE_KEY").then(|| "valor".to_string())
        })
        .await;
        let con_variable = problema_de(&registro, "rutas");

        assert!(sin_variable.unwrap().contains("AGENTE_KEY"));
        assert_eq!(con_variable, None);
    }

    #[tokio::test]
    async fn refresh_flags_a_scheme_the_client_does_not_support() {
        let servidor = MockServer::start().await;
        // La tarjeta de ejemplo de la especificación pide OpenID Connect.
        servir(&servidor, tarjeta_para(&servidor, "Rutas")).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil(dir.path(), "rutas", &servidor.uri());
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();

        refresh_remote_cards_with_env(&registro, &bus, &cliente, &config(), &|_| None).await;

        let problema = problema_de(&registro, "rutas").unwrap();
        assert!(problema.contains("OpenID Connect"), "{problema}");
    }

    #[tokio::test]
    async fn a_card_that_changes_its_security_returns_to_pending() {
        let servidor = MockServer::start().await;
        servir(&servidor, tarjeta_con_clave_de_api(&servidor)).await;
        let dir = tempfile::tempdir().unwrap();
        let datos = tempfile::tempdir().unwrap();
        perfil_con_credenciales(dir.path(), "rutas", &servidor.uri(), "claveApi");
        let registro = registro(dir.path(), datos.path());
        let bus = EventBus::new(16);
        let cliente = http_client(&config()).unwrap().unwrap();
        refresh_remote_cards(&registro, &bus, &cliente, &config()).await;
        registro.approve("rutas").unwrap();

        servidor.reset().await;
        let mut otra = tarjeta_con_clave_de_api(&servidor);
        otra["securitySchemes"]["claveApi"]["apiKeySecurityScheme"]["name"] = json!("X-Otra");
        servir(&servidor, otra).await;
        refresh_remote_cards(&registro, &bus, &cliente, &config()).await;

        assert_eq!(
            registro.snapshot().approval("rutas"),
            Some(Approval::Pending)
        );
    }
}
