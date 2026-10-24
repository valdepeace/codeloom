//! Prueba contra un servidor A2A real (A8a): se ejecuta a mano, no en la suite normal.
//!
//! Cabe señalar que estas pruebas llevan `#[ignore]` porque necesitan un servidor externo, y la
//! regla del proyecto es que la suite no toque la red. Se lanzan con
//! `A2A_LIVE_ENDPOINT=http://127.0.0.1:9999 cargo test -p codeloom-a2a --test live_remote -- --ignored`
//! frente a un servidor construido con el SDK oficial (`a2a-sdk`), es decir, una implementación
//! independiente de la nuestra.
//!
//! Las pruebas `a_real_secured_server_*` (A8d) necesitan además un servidor protegido con `Bearer`
//! y su token en `A2A_LIVE_TOKEN`, p. ej.
//! `A2A_LIVE_ENDPOINT=http://127.0.0.1:9998 A2A_LIVE_TOKEN=... cargo test -p codeloom-a2a --test live_remote secured -- --ignored`.

use std::time::Duration;

use codeloom_a2a::client::A2aClientFactory;
use codeloom_a2a::config::{A2aConfig, A2aRole};
use codeloom_a2a::remote::{fetch_card, http_client};
use codeloom_core::a2a::Message;
use codeloom_core::registry::{RemoteFetched, RemoteSpec};
use codeloom_core::remote_task::{
    RemoteClientFactory, RemoteReply, RemoteTaskClient, RemoteTaskError,
};
use codeloom_core::task::TaskState;

/// Remoto con la tarjeta ya descargada; la credencial del esquema `token` se lee de
/// `A2A_LIVE_TOKEN`.
async fn remoto(streaming: bool) -> (A2aConfig, RemoteSpec) {
    let endpoint = std::env::var("A2A_LIVE_ENDPOINT").expect("falta A2A_LIVE_ENDPOINT");
    let config = A2aConfig {
        role: A2aRole::Consume,
        ..A2aConfig::default()
    };
    let http = http_client(&config).unwrap().unwrap();
    let tarjeta = fetch_card(&http, &endpoint, config.max_card_bytes)
        .await
        .expect("la tarjeta debería descargarse");
    println!("tarjeta: {} ({})", tarjeta.card.name, tarjeta.jsonrpc_url);
    let spec = RemoteSpec {
        endpoint,
        fetched: Some(RemoteFetched {
            card_hash: tarjeta.hash,
            jsonrpc_url: tarjeta.jsonrpc_url,
            tenant: tarjeta.tenant,
            requires_auth: tarjeta.requires_auth,
            security: tarjeta.security,
            auth_problem: None,
            streaming,
        }),
        credentials: [("token".to_string(), "A2A_LIVE_TOKEN".to_string())].into(),
    };
    (config, spec)
}

async fn cliente() -> std::sync::Arc<dyn RemoteTaskClient> {
    cliente_con(false).await
}

async fn cliente_con(streaming: bool) -> std::sync::Arc<dyn RemoteTaskClient> {
    let (config, spec) = remoto(streaming).await;
    A2aClientFactory::from_config(&config)
        .unwrap()
        .unwrap()
        .client_for("vivo", &spec)
        .expect("con tarjeta descargada hay cliente")
}

#[tokio::test]
#[ignore = "necesita un servidor A2A real en A2A_LIVE_ENDPOINT"]
async fn a_real_server_completes_a_task() {
    let cliente = cliente().await;

    let RemoteReply::Task(tarea) = cliente.send(Message::user_text("hola")).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    println!("enviada: {} en {:?}", tarea.id, tarea.status.state);
    let inicio = std::time::Instant::now();
    let fin = loop {
        let actual = cliente.get(&tarea.id).await.unwrap();
        println!("  {:?}", actual.status.state);
        if actual.status.state.is_terminal() {
            break actual;
        }
        assert!(inicio.elapsed() < Duration::from_secs(20), "no terminó");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    assert_eq!(fin.status.state, TaskState::Completed);
    assert_eq!(fin.artifacts[0].text(), "ECO: HOLA");
}

#[tokio::test]
#[ignore = "necesita un servidor A2A real con streaming en A2A_LIVE_ENDPOINT"]
async fn a_real_server_streams_a_task_until_it_is_canceled() {
    use futures::StreamExt;
    let cliente = cliente_con(true).await;
    let RemoteReply::Task(tarea) = cliente.send(Message::user_text("larga")).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    let mut flujo = cliente
        .watch(&tarea.id)
        .await
        .expect("el servidor debía aceptar la suscripción");
    let otro = cliente.clone();
    let id = tarea.id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        let _ = otro.cancel(&id).await;
    });

    let mut estados = Vec::new();
    while let Some(elemento) = tokio::time::timeout(Duration::from_secs(20), flujo.next())
        .await
        .expect("el flujo se quedó mudo")
    {
        let actual = elemento.expect("el flujo no debía fallar");
        println!("  flujo: {:?}", actual.status.state);
        estados.push(actual.status.state);
    }

    assert_eq!(estados.last(), Some(&TaskState::Canceled), "{estados:?}");
}

#[tokio::test]
#[ignore = "necesita un servidor A2A real con streaming en A2A_LIVE_ENDPOINT"]
async fn a_real_server_streams_a_task_to_completion_with_its_artifact() {
    use futures::StreamExt;
    let cliente = cliente_con(true).await;
    let RemoteReply::Task(tarea) = cliente.send(Message::user_text("hola")).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    let flujo = cliente
        .watch(&tarea.id)
        .await
        .expect("el servidor debía aceptar la suscripción");

    let tareas: Vec<_> = tokio::time::timeout(Duration::from_secs(20), flujo.collect::<Vec<_>>())
        .await
        .expect("el flujo no terminó");

    let ultima = tareas
        .last()
        .unwrap()
        .as_ref()
        .expect("el flujo no debía fallar");
    assert_eq!(ultima.status.state, TaskState::Completed);
    assert_eq!(ultima.artifacts[0].text(), "ECO: HOLA");
}

#[tokio::test]
#[ignore = "necesita un servidor A2A real en A2A_LIVE_ENDPOINT"]
async fn a_real_server_cancels_a_task() {
    let cliente = cliente().await;
    let RemoteReply::Task(tarea) = cliente.send(Message::user_text("larga")).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    tokio::time::sleep(Duration::from_millis(600)).await;

    let cancelada = cliente.cancel(&tarea.id).await.unwrap();
    println!("cancelación: {:?}", cancelada.status.state);

    assert_eq!(cancelada.status.state, TaskState::Canceled);
}

#[tokio::test]
#[ignore = "necesita un servidor A2A protegido con Bearer en A2A_LIVE_ENDPOINT y su token en A2A_LIVE_TOKEN"]
async fn a_real_secured_server_accepts_the_bearer_token() {
    let cliente = cliente().await;

    let RemoteReply::Task(tarea) = cliente.send(Message::user_text("hola")).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    let inicio = std::time::Instant::now();
    let fin = loop {
        let actual = cliente.get(&tarea.id).await.unwrap();
        if actual.status.state.is_terminal() {
            break actual;
        }
        assert!(inicio.elapsed() < Duration::from_secs(20), "no terminó");
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    assert_eq!(fin.status.state, TaskState::Completed);
    assert_eq!(fin.artifacts[0].text(), "ECO: HOLA");
}

#[tokio::test]
#[ignore = "necesita un servidor A2A protegido con Bearer en A2A_LIVE_ENDPOINT"]
async fn a_real_secured_server_refuses_a_wrong_token_and_we_do_not_even_try_without_one() {
    let (config, spec) = remoto(false).await;
    let fabrica = || A2aClientFactory::from_config(&config).unwrap().unwrap();

    // Sin la variable: no hay cliente y el motivo nombra la variable.
    let sin = fabrica().with_env(|_| None);
    assert!(sin.client_for("vivo", &spec).is_none());
    let motivo = sin.unavailable_reason("vivo", &spec).unwrap();
    println!("sin token: {motivo}");
    assert!(motivo.contains("A2A_LIVE_TOKEN"), "{motivo}");

    // Con un token equivocado: el servidor real contesta 401 y lo reconocemos como tal.
    let mal = fabrica()
        .with_env(|_| Some("token-equivocado".to_string()))
        .client_for("vivo", &spec)
        .expect("hay credencial, así que hay cliente");
    let error = mal.send(Message::user_text("hola")).await.unwrap_err();
    println!("token equivocado: {error}");
    assert!(matches!(error, RemoteTaskError::AuthFailed(_)), "{error:?}");
    assert!(!error.to_string().contains("token-equivocado"));
}
