use codeloom_core::agent::AgentLoop;
use codeloom_core::config::{CodeLoomConfig, ProviderConfig, ProviderKind};
use codeloom_core::event::EventBus;
use codeloom_core::permission::PermissionService;
use codeloom_core::provider::openai::OpenAiCompatProvider;
use codeloom_core::session::sqlite::SqliteStore;
use codeloom_core::session::SessionStore;
use codeloom_core::tools::view::ViewTool;
use codeloom_core::tools::ToolRegistry;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn end_to_end_single_turn() {
    let server = MockServer::start().await;
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"hello \"}}]}\n\n\
                data: {\"choices\":[{\"delta\":{\"content\":\"world\"}}]}\n\n\
                data: {\"choices\":[{\"finish_reason\":\"stop\"}]}\n\n";
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
        .mount(&server)
        .await;

    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(
        SqliteStore::open(dir.path().join("s.db").to_str().unwrap())
            .await
            .unwrap(),
    );
    let sid = store.create_session("/proj").await.unwrap();

    let mut tools = ToolRegistry::new();
    tools.register(Arc::new(ViewTool));

    let cfg = CodeLoomConfig {
        provider: ProviderConfig {
            kind: ProviderKind::OpenAiCompat,
            base_url: server.uri(),
            api_key_env: "K".into(),
        },
        model: "m".into(),
        max_steps: 3,
    };
    let provider = Arc::new(OpenAiCompatProvider::new(server.uri(), "sk-test"));
    let agent = AgentLoop::new(
        provider,
        store.clone(),
        Arc::new(tools),
        EventBus::new(64),
        Arc::new(PermissionService::allow_all()),
        cfg,
        dir.path().to_path_buf(),
    );

    let outcome = agent
        .run(sid, "say hi".into(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(outcome.final_text, "hello world");
    assert_eq!(outcome.steps, 1);
    let msgs = store.messages(sid).await.unwrap();
    assert!(msgs.iter().any(|m| m.text().contains("hello world")));
}
