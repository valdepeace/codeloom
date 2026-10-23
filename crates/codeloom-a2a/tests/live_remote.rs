//! Prueba contra un servidor A2A real (A8a): se ejecuta a mano, no en la suite normal.
//!
//! Cabe señalar que estas pruebas llevan `#[ignore]` porque necesitan un servidor externo, y la
//! regla del proyecto es que la suite no toque la red. Se lanzan con
//! `A2A_LIVE_ENDPOINT=http://127.0.0.1:9999 cargo test -p codeloom-a2a --test live_remote -- --ignored`
//! frente a un servidor construido con el SDK oficial (`a2a-sdk`), es decir, una implementación
//! independiente de la nuestra.

use std::time::Duration;

use codeloom_a2a::client::A2aClientFactory;
use codeloom_a2a::config::{A2aConfig, A2aRole};
use codeloom_a2a::remote::{fetch_card, http_client};
use codeloom_core::a2a::Message;
use codeloom_core::registry::{RemoteFetched, RemoteSpec};
use codeloom_core::remote_task::{RemoteClientFactory, RemoteReply, RemoteTaskClient};
use codeloom_core::task::TaskState;

async fn cliente() -> std::sync::Arc<dyn RemoteTaskClient> {
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
            security: None,
            auth_problem: None,
        }),
        credentials: Default::default(),
    };
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
