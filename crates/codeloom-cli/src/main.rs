use anyhow::{Context, Result};
use clap::Parser;
use codeloom_core::agent::AgentLoop;
use codeloom_core::config::{CodeLoomConfig, ProviderKind};
use codeloom_core::event::EventBus;
use codeloom_core::permission::PermissionService;
use codeloom_core::provider::anthropic::AnthropicProvider;
use codeloom_core::provider::openai::OpenAiCompatProvider;
use codeloom_core::provider::Provider;
use codeloom_core::session::sqlite::SqliteStore;
use codeloom_core::session::SessionStore;
use codeloom_core::tools::bash::BashTool;
use codeloom_core::tools::edit::EditTool;
use codeloom_core::tools::glob::GlobTool;
use codeloom_core::tools::grep::GrepTool;
use codeloom_core::tools::view::ViewTool;
use codeloom_core::tools::ToolRegistry;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Parser)]
#[command(name = env!("CARGO_BIN_NAME"), version)]
struct Cli {
    /// Ejecuta un prompt y sale (sin TUI).
    #[arg(long)]
    print: Option<String>,
    #[arg(long)]
    yolo: bool,
}

fn build_provider(cfg: &CodeLoomConfig) -> Result<Arc<dyn Provider>> {
    let key = std::env::var(&cfg.provider.api_key_env)
        .with_context(|| format!("falta la variable de entorno {}", cfg.provider.api_key_env))?;
    Ok(match cfg.provider.kind {
        ProviderKind::OpenAiCompat => Arc::new(OpenAiCompatProvider::new(
            cfg.provider.base_url.clone(),
            key,
        )),
        ProviderKind::Anthropic => {
            Arc::new(AnthropicProvider::new(cfg.provider.base_url.clone(), key))
        }
    })
}

fn build_tools() -> ToolRegistry {
    let mut tools = ToolRegistry::new();
    tools.register(Arc::new(ViewTool));
    tools.register(Arc::new(GlobTool));
    tools.register(Arc::new(GrepTool));
    tools.register(Arc::new(EditTool));
    tools.register(Arc::new(BashTool));
    tools
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;
    let cfg = CodeLoomConfig::load(&cwd.join("codeloom.toml"))?;
    let provider = build_provider(&cfg)?;

    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| cwd.clone())
        .join(".codeloom");
    std::fs::create_dir_all(&data_dir)?;
    let store = Arc::new(SqliteStore::open(data_dir.join("sessions.db").to_str().unwrap()).await?);
    let session = store
        .create_session(cwd.to_str().unwrap_or_default())
        .await?;

    let bus = EventBus::new(1024);
    let bus_rx = bus.subscribe();
    let permission = Arc::new(PermissionService::allow_all());
    let agent = Arc::new(AgentLoop::new(
        provider,
        store,
        Arc::new(build_tools()),
        bus,
        permission,
        cfg,
        cwd,
    ));

    if let Some(prompt) = cli.print {
        let outcome = agent.run(session, prompt, CancellationToken::new()).await?;
        println!("{}", outcome.final_text);
        return Ok(());
    }

    let _ = cli.yolo;
    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::channel::<String>(8);
    let agent_task = agent.clone();
    tokio::spawn(async move {
        while let Some(p) = prompt_rx.recv().await {
            if let Err(e) = agent_task.run(session, p, CancellationToken::new()).await {
                eprintln!("{}: {e}", env!("CARGO_BIN_NAME"));
            }
        }
    });

    codeloom_tui::run::run(codeloom_tui::app::App::new(), prompt_tx, bus_rx).await?;
    Ok(())
}
