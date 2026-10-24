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
    // Cada implementación pone su prefijo («ECO: », «ECO-JS: »): importa que llegue el texto.
    assert!(
        ultima.artifacts[0].text().ends_with("HOLA"),
        "{:?}",
        ultima.artifacts
    );
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

#[tokio::test]
#[ignore = "necesita un servidor A2A con mTLS en A2A_LIVE_ENDPOINT, su CA en A2A_LIVE_CA_FILE y la identidad en A2A_LIVE_IDENTITY"]
async fn a_real_mtls_server_accepts_our_identity_and_refuses_us_without_it() {
    let endpoint = std::env::var("A2A_LIVE_ENDPOINT").expect("falta A2A_LIVE_ENDPOINT");
    let config = A2aConfig {
        role: A2aRole::Consume,
        ca_file: Some(std::env::var("A2A_LIVE_CA_FILE").expect("falta A2A_LIVE_CA_FILE")),
        ..A2aConfig::default()
    };
    let http = http_client(&config).unwrap().unwrap();
    let credenciales: std::collections::BTreeMap<String, String> =
        [("tls".to_string(), "A2A_LIVE_IDENTITY".to_string())].into();

    // Sin presentar identidad, el apretón de manos TLS del servidor real nos corta.
    let sin = fetch_card(&http, &endpoint, config.max_card_bytes).await;
    println!("sin identidad: {sin:?}");
    assert!(
        sin.is_err(),
        "el servidor mTLS no debería servirnos sin certificado"
    );

    // La descarga de la tarjeta reintenta presentando la identidad que declara el perfil.
    let env = |nombre: &str| std::env::var(nombre).ok();
    let tarjeta =
        codeloom_a2a::sync::fetch_remote_card(&http, &config, &endpoint, &credenciales, &env)
            .await
            .expect("con la identidad declarada la tarjeta debería descargarse");
    println!("tarjeta: {} ({})", tarjeta.card.name, tarjeta.jsonrpc_url);
    assert!(tarjeta.requires_auth);

    let spec = RemoteSpec {
        endpoint,
        fetched: Some(RemoteFetched {
            card_hash: tarjeta.hash,
            jsonrpc_url: tarjeta.jsonrpc_url,
            tenant: tarjeta.tenant,
            requires_auth: tarjeta.requires_auth,
            security: tarjeta.security,
            auth_problem: None,
            streaming: false,
        }),
        credentials: credenciales,
    };
    let fabrica = A2aClientFactory::from_config(&config).unwrap().unwrap();

    // Sin la variable de la identidad no hay cliente, y el motivo nombra la variable.
    let sin_variable = A2aClientFactory::from_config(&config)
        .unwrap()
        .unwrap()
        .with_env(|_| None);
    assert!(sin_variable.client_for("vivo", &spec).is_none());
    let motivo = sin_variable.unavailable_reason("vivo", &spec).unwrap();
    println!("sin variable: {motivo}");
    assert!(motivo.contains("A2A_LIVE_IDENTITY"), "{motivo}");

    // Con ella, la tarea se completa de verdad contra el servidor real.
    let cliente = fabrica
        .client_for("vivo", &spec)
        .expect("con la identidad hay cliente");
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
        tokio::time::sleep(Duration::from_millis(300)).await;
    };
    assert_eq!(fin.status.state, TaskState::Completed);
    assert_eq!(fin.artifacts[0].text(), "ECO-MTLS: HOLA");
}

/// Remoto protegido con OAuth2 `client_credentials`: la credencial (`id:secreto`) se lee de la
/// variable `A2A_LIVE_OAUTH`, bajo el esquema `oa` de la tarjeta.
async fn remoto_oauth() -> (A2aConfig, RemoteSpec) {
    let endpoint = std::env::var("A2A_LIVE_ENDPOINT").expect("falta A2A_LIVE_ENDPOINT");
    let config = A2aConfig {
        role: A2aRole::Consume,
        ..A2aConfig::default()
    };
    let http = http_client(&config).unwrap().unwrap();
    let tarjeta = fetch_card(&http, &endpoint, config.max_card_bytes)
        .await
        .expect("la tarjeta debería descargarse");
    assert!(tarjeta.requires_auth);
    let spec = RemoteSpec {
        endpoint,
        fetched: Some(RemoteFetched {
            card_hash: tarjeta.hash,
            jsonrpc_url: tarjeta.jsonrpc_url,
            tenant: tarjeta.tenant,
            requires_auth: tarjeta.requires_auth,
            security: tarjeta.security,
            auth_problem: None,
            streaming: false,
        }),
        credentials: [("oa".to_string(), "A2A_LIVE_OAUTH".to_string())].into(),
    };
    (config, spec)
}

/// Espera a que la tarea llegue a un estado que cumpla `fin`.
async fn esperar(
    cliente: &std::sync::Arc<dyn RemoteTaskClient>,
    id: &str,
    fin: impl Fn(TaskState) -> bool,
) -> codeloom_core::remote_task::RemoteTask {
    let inicio = std::time::Instant::now();
    loop {
        let actual = cliente.get(id).await.unwrap();
        println!("  {:?}", actual.status.state);
        if fin(actual.status.state) {
            return actual;
        }
        assert!(
            inicio.elapsed() < Duration::from_secs(20),
            "no llegó al estado"
        );
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

#[tokio::test]
#[ignore = "necesita un servidor A2A con OAuth2 client_credentials en A2A_LIVE_ENDPOINT y su credencial id:secreto en A2A_LIVE_OAUTH"]
async fn a_real_oauth2_server_gives_us_a_token_and_completes_a_task() {
    let (config, spec) = remoto_oauth().await;
    let fabrica = || A2aClientFactory::from_config(&config).unwrap().unwrap();

    // Sin tocar a la gente: con la credencial buena, el token se pide solo y la tarea se completa.
    let cliente = fabrica()
        .client_for("vivo", &spec)
        .expect("con la credencial hay cliente");
    let RemoteReply::Task(tarea) = cliente.send(Message::user_text("hola")).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    let fin = esperar(&cliente, &tarea.id, TaskState::is_terminal).await;
    assert_eq!(fin.status.state, TaskState::Completed);
    assert_eq!(fin.artifacts[0].text(), "ECO-OAUTH: HOLA");

    // Con un secreto equivocado, el punto de token rechaza y jamás se llama sin credencial.
    let mal = fabrica()
        .with_env(|_| Some("cliente-prueba:secreto-equivocado".to_string()))
        .client_for("vivo", &spec)
        .expect("hay credencial, así que hay cliente");
    let error = mal.send(Message::user_text("hola")).await.unwrap_err();
    println!("secreto equivocado: {error}");
    assert!(matches!(error, RemoteTaskError::AuthFailed(_)), "{error:?}");
    assert!(!error.to_string().contains("secreto-equivocado"));
}

#[tokio::test]
#[ignore = "necesita el servidor OAuth2 de pruebas (pregunta con «pregunta…») en A2A_LIVE_ENDPOINT y A2A_LIVE_OAUTH"]
async fn a_real_server_asks_for_input_and_resumes_the_same_task_with_the_answer() {
    let (config, spec) = remoto_oauth().await;
    let cliente = A2aClientFactory::from_config(&config)
        .unwrap()
        .unwrap()
        .client_for("vivo", &spec)
        .expect("con la credencial hay cliente");

    let RemoteReply::Task(tarea) = cliente
        .send(Message::user_text("pregunta algo"))
        .await
        .unwrap()
    else {
        panic!("se esperaba una tarea");
    };
    let detenida = esperar(&cliente, &tarea.id, |e| e == TaskState::InputRequired).await;
    let pregunta = detenida.status.message.as_ref().map(Message::text);
    println!("pregunta del remoto: {pregunta:?}");
    assert_eq!(pregunta.as_deref(), Some("¿Cómo te llamas?"));

    // La respuesta viaja con el mismo `taskId` y `contextId`: el servidor real la reconoce como
    // continuación de la misma tarea, no como una tarea nueva.
    let mut respuesta = Message::user_text("Andres");
    respuesta.task_id = Some(tarea.id.clone());
    respuesta.context_id = detenida.context_id.clone();
    let RemoteReply::Task(seguida) = cliente.send(respuesta).await.unwrap() else {
        panic!("se esperaba una tarea");
    };
    assert_eq!(seguida.id, tarea.id, "debe ser la misma tarea");
    let fin = esperar(&cliente, &tarea.id, TaskState::is_terminal).await;
    assert_eq!(fin.status.state, TaskState::Completed);
    assert_eq!(fin.artifacts[0].text(), "RESPUESTA: ANDRES");
}
