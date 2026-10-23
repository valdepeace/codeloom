use anyhow::{bail, Context, Result};
use clap::Parser;
use codeloom_core::agent::compact::CompactionConfig;
use codeloom_core::agent::AgentLoop;
use codeloom_core::budget::BudgetConfig;
use codeloom_core::catalog::{
    cache_path, fetch_raw, is_stale, read_cache, refresh, Catalog, CATALOG_URL,
};
use codeloom_core::config::{AgentsConfig, CodeLoomConfig, ProviderKind};
use codeloom_core::event::EventBus;
use codeloom_core::permission::{DenyAll, PermissionService};
use codeloom_core::provider::anthropic::AnthropicProvider;
use codeloom_core::provider::openai::OpenAiCompatProvider;
use codeloom_core::provider::Provider;
use codeloom_core::registry::{
    AgentRegistry, AgentSource, BuiltInSource, FileSource, ProfileLevel, TrustStore,
};
use codeloom_core::session::sqlite::SqliteStore;
use codeloom_core::session::SessionStore;
use codeloom_core::tools::ask_user::AskUserTool;
use codeloom_core::tools::bash::BashTool;
use codeloom_core::tools::edit::EditTool;
use codeloom_core::tools::glob::GlobTool;
use codeloom_core::tools::grep::GrepTool;
use codeloom_core::tools::job_kill::JobKillTool;
use codeloom_core::tools::job_list::JobListTool;
use codeloom_core::tools::job_output::JobOutputTool;
use codeloom_core::tools::job_start::JobStartTool;
use codeloom_core::tools::spill::ToolSpill;
use codeloom_core::tools::subagent::SubagentTool;
use codeloom_core::tools::subagent_control::SubagentControlTool;
use codeloom_core::tools::subagent_list::SubagentListTool;
use codeloom_core::tools::todo_write::TodoWriteTool;
use codeloom_core::tools::view::ViewTool;
use codeloom_core::tools::ToolRegistry;
use codeloom_herdr::{AgentState, HerdrContext};
use codeloom_tui::app::{ProviderSettings, SessionChoice};
use codeloom_tui::run::{SessionCommand, SessionUpdate};
use codeloom_tui::widgets::permission::UiPermissionResponder;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
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
    let key = std::env::var(&cfg.provider.api_key_env).with_context(|| {
        format!(
            "falta la variable de entorno {} (ponla en .env o en el entorno)",
            cfg.provider.api_key_env
        )
    })?;
    if key.is_empty() {
        bail!("environment variable {} is empty", cfg.provider.api_key_env);
    }
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

fn config_from_provider_settings(
    settings: ProviderSettings,
    current: &CodeLoomConfig,
) -> Result<CodeLoomConfig, String> {
    let mut candidate = current.clone();
    candidate.model = settings.model.trim().to_string();
    candidate.provider.kind = settings.kind;
    candidate.provider.base_url = settings.base_url.trim().to_string();
    candidate.provider.api_key_env = settings.api_key_env.trim().to_string();
    candidate.effort = settings.effort;

    if candidate.model.is_empty() {
        return Err("Model cannot be empty".into());
    }
    codeloom_core::config::validate_base_url(&candidate.provider.base_url)?;
    let mut chars = candidate.provider.api_key_env.chars();
    if !chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        || !chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
    {
        return Err("API key variable must be a valid environment variable name".into());
    }
    Ok(candidate)
}

fn apply_provider_settings(
    settings: ProviderSettings,
    current: &CodeLoomConfig,
    config_path: &std::path::Path,
    agent: &AgentLoop,
) -> Result<CodeLoomConfig, String> {
    let candidate = config_from_provider_settings(settings, current)?;
    let provider = build_provider(&candidate).map_err(|error| error.to_string())?;
    candidate
        .save(config_path)
        .map_err(|error| error.to_string())?;
    agent.update_provider(provider, candidate.clone());
    Ok(candidate)
}

fn build_tools() -> ToolRegistry {
    let mut tools = ToolRegistry::new();
    tools.register(Arc::new(ViewTool));
    tools.register(Arc::new(GlobTool));
    tools.register(Arc::new(GrepTool));
    tools.register(Arc::new(EditTool));
    tools.register(Arc::new(BashTool));
    tools.register(Arc::new(JobListTool));
    tools.register(Arc::new(JobOutputTool));
    tools.register(Arc::new(JobKillTool));
    tools.register(Arc::new(JobStartTool));
    tools.register(Arc::new(SubagentTool));
    tools.register(Arc::new(SubagentControlTool));
    tools.register(Arc::new(SubagentListTool));
    tools.register(Arc::new(TodoWriteTool));
    tools.register(Arc::new(AskUserTool));
    tools
}

async fn report_herdr(context: &mut Option<HerdrContext>, state: AgentState, message: &str) {
    if let Some(herdr) = context {
        if let Err(error) = herdr.report(state, message).await {
            eprintln!("herdr reporting unavailable; continuing standalone: {error}");
            *context = None;
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut herdr = HerdrContext::from_env();
    let cwd = herdr
        .as_ref()
        .and_then(|context| context.workspace_cwd.as_ref())
        .filter(|path| path.is_dir())
        .cloned()
        .unwrap_or(std::env::current_dir()?);
    load_dotenv(&cwd);
    let config_path = cwd.join("codeloom.toml");
    let cfg = CodeLoomConfig::load(&config_path)?;
    let agents_cfg = AgentsConfig::load(&config_path)?;
    // Como `[agents]`, un `[budget]` mal escrito impide arrancar con un error claro: un aviso
    // por stderr se perdería tras la pantalla alternativa de la interfaz.
    let presupuesto = BudgetConfig::load(&config_path)?;
    // Igual que `[budget]`: una sección `[compaction]` mal escrita impide arrancar con un error claro.
    let compactacion = CompactionConfig::load(&config_path)?;
    // Igual que las anteriores: una sección `[a2a]` mal escrita impide arrancar con un error claro.
    let a2a = codeloom_a2a::config::A2aConfig::load(&config_path)?;
    let cwd_label = cwd.display().to_string();
    let model_label = cfg.model.clone();
    let provider = build_provider(&cfg)?;

    let data_dir = dirs::home_dir()
        .unwrap_or_else(|| cwd.clone())
        .join(".codeloom");
    std::fs::create_dir_all(&data_dir)?;
    let store = Arc::new(SqliteStore::open(data_dir.join("sessions.db").to_str().unwrap()).await?);
    let project = cwd.to_str().unwrap_or_default().to_string();
    let sessions = if cli.print.is_none() {
        store.list_sessions(&project).await?
    } else {
        Vec::new()
    };

    let bus = EventBus::new(1024);
    let bus_rx = bus.subscribe();
    let arranque = arrancar_agentes(dirs::home_dir().as_deref(), &cwd, &agents_cfg);
    let registro_agentes = Arc::new(arranque.registro);
    // La vigilancia dura lo que `main`: al soltar el `WatcherHandle` se detiene.
    let _vigilancia_agentes = registro_agentes.spawn_watcher(arranque.directorios, bus.clone());
    let fabrica_remotos = arrancar_remotos(&a2a, &registro_agentes, &bus)?;
    let (perm_tx, perm_rx) = tokio::sync::mpsc::channel(8);
    let (ask_tx, ask_rx) = tokio::sync::mpsc::channel(4);
    let permission = Arc::new(if cli.print.is_some() {
        // limitación: `-p` no es interactivo; sin `--yolo` se deniega todo.
        if cli.yolo {
            PermissionService::allow_all()
        } else {
            PermissionService::with_responder(Box::new(DenyAll))
        }
    } else {
        PermissionService::with_responder(Box::new(
            UiPermissionResponder::new(perm_tx, cli.yolo).with_questions(ask_tx),
        ))
    });
    // Catálogo compartido con el agente (y sus hijos): vacío hasta que llega la carga en
    // segundo plano, así que los apuntes de antes llevan precio `NULL`.
    let catalog = Arc::new(std::sync::RwLock::new(Catalog::default()));
    let agent = AgentLoop::new(
        provider,
        store.clone(),
        Arc::new(build_tools()),
        bus,
        permission,
        cfg.clone(),
        cwd,
    )
    .with_catalog(catalog.clone())
    .with_agent_registry(registro_agentes.clone())
    .with_delegation_confirm(agents_cfg.confirm_delegation)
    .with_spill(ToolSpill::new(data_dir.join("tool-output")))
    .with_budget(presupuesto)
    .with_compaction(compactacion);
    let agent = match fabrica_remotos {
        Some(fabrica) => agent.with_remote_clients(fabrica),
        None => agent,
    };
    let agent = Arc::new(agent);

    if let Some(prompt) = cli.print {
        let session = store.create_session(&project).await?;
        let outcome = agent.run(session, prompt, CancellationToken::new()).await?;
        println!("{}", outcome.final_text);
        return Ok(());
    }

    report_herdr(&mut herdr, AgentState::Idle, "ready").await;

    let (prompt_tx, mut prompt_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (wake_tx, mut wake_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    // Compactaciones manuales (`/compact`): el trabajador resume la sesión abierta.
    let (compact_tx, mut compact_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let (settings_tx, mut settings_rx) =
        tokio::sync::mpsc::unbounded_channel::<codeloom_tui::run::ProviderSettingsCommand>();
    let (session_command_tx, mut session_command_rx) = tokio::sync::mpsc::unbounded_channel();
    let (session_update_tx, session_update_rx) = tokio::sync::mpsc::unbounded_channel();
    // Solo en modo TUI (`--print` ya ha salido arriba): la carga corre en segundo plano.
    let (catalog_tx, catalog_rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(load_catalog(catalog_tx, catalog.clone()));
    let active_turn: codeloom_tui::run::ActiveTurn = Arc::new(std::sync::Mutex::new(None));
    let worker_active_turn = active_turn.clone();
    let agent_task = agent.clone();
    let worker_store = store.clone();
    let worker_project = project.clone();
    let worker_config_path = config_path.clone();
    let mut worker_config = cfg.clone();
    tokio::spawn(async move {
        let mut herdr = herdr;
        let mut session = None;
        let mut wake_open = true;
        loop {
            tokio::select! {
                command = session_command_rx.recv() => match command {
                    Some(SessionCommand::Refresh) => {
                        let update = worker_store.list_sessions(&worker_project).await
                            .map(SessionUpdate::List)
                            .unwrap_or_else(|error| SessionUpdate::Error(error.to_string()));
                        let _ = session_update_tx.send(update);
                    }
                    Some(SessionCommand::Select(choice)) => {
                        let loaded = async {
                            let id = match choice {
                                SessionChoice::Resume(id) => id,
                                SessionChoice::New => worker_store.create_session(&worker_project).await?,
                            };
                            let messages = worker_store.messages(id).await?;
                            // Una sesión nueva no tiene apuntes: la consulta devuelve vacío.
                            let usage = worker_store.session_usage(id).await?;
                            let subagents = worker_store.list_subagents(id).await?;
                            Ok::<_, codeloom_core::error::CoreError>((id, messages, usage, subagents))
                        }.await;
                        match loaded {
                            Ok((id, messages, usage, subagents)) => {
                                session = Some(id);
                                let _ = session_update_tx.send(SessionUpdate::Loaded(id, messages));
                                let _ = session_update_tx.send(SessionUpdate::Usage(id, usage));
                                let _ = session_update_tx.send(SessionUpdate::Subagents(id, subagents));
                            }
                            Err(error) => {
                                let _ = session_update_tx.send(SessionUpdate::Error(error.to_string()));
                            }
                        }
                    }
                    None => break,
                },
                Some(command) = settings_rx.recv() => {
                    let result = apply_provider_settings(
                        command.settings,
                        &worker_config,
                        &worker_config_path,
                        &agent_task,
                    );
                    if let Ok(config) = &result {
                        worker_config = config.clone();
                    }
                    let _ = command.reply.send(result.map(|_| ( )));
                }
                // `/compact`: la interfaz solo lo pide con sesión abierta y sin turno en curso.
                // `compact_now` publica siempre `ManualCompactionEnded`, que libera a la interfaz.
                Some(()) = compact_rx.recv() => {
                    let Some(session_id) = session else { continue; };
                    agent_task.compact_now(session_id, CancellationToken::new()).await;
                }
                prompt = prompt_rx.recv() => match prompt {
                    Some(prompt) => {
                        let Some(session_id) = session else { continue; };
                        report_herdr(&mut herdr, AgentState::Working, "working").await;
                        let cancel = CancellationToken::new();
                        *worker_active_turn.lock().unwrap() = Some(cancel.clone());
                        let result = agent_task.run(session_id, prompt, cancel).await;
                        *worker_active_turn.lock().unwrap() = None;
                        match result {
                            Ok(_) => report_herdr(&mut herdr, AgentState::Idle, "ready").await,
                            Err(error) => {
                                report_herdr(&mut herdr, AgentState::Blocked, "error").await;
                                eprintln!("{}: {error}", env!("CARGO_BIN_NAME"));
                            }
                        }
                    }
                    None => break,
                },
                // Despertar del padre ocioso. Sin sesión activa se ignora. El informe a
                // herdr solo cambia si el despertar ejecutó turno: `run_woken` no dice de
                // antemano si lo hará, así que no se emite `Working` previo; un `Ok(None)`
                // no toca el estado (el padre seguía ocioso) y `Ok(Some)` cierra con `Idle`.
                wake = wake_rx.recv(), if wake_open => match wake {
                    Some(()) => {
                        let Some(session_id) = session else { continue; };
                        let cancel = CancellationToken::new();
                        *worker_active_turn.lock().unwrap() = Some(cancel.clone());
                        let result = agent_task.run_woken(session_id, cancel).await;
                        *worker_active_turn.lock().unwrap() = None;
                        match result {
                            Ok(Some(_)) => report_herdr(&mut herdr, AgentState::Idle, "ready").await,
                            Ok(None) => {}
                            Err(error) => {
                                report_herdr(&mut herdr, AgentState::Blocked, "error").await;
                                eprintln!("{}: {error}", env!("CARGO_BIN_NAME"));
                            }
                        }
                    }
                    // Canal cerrado: no volver a entrar en esta rama (evita busy-loop).
                    None => wake_open = false,
                }
            }
        }
    });

    let mut app = codeloom_tui::app::App::with_context(cwd_label, model_label);
    app.set_provider_settings((&cfg).into());
    app.set_sessions(sessions);
    app.set_prefs_path(data_dir.join("ui.toml"));
    app.set_agent_registry(registro_agentes.clone());
    codeloom_tui::run::run(
        app,
        prompt_tx,
        wake_tx,
        compact_tx,
        settings_tx,
        bus_rx,
        perm_rx,
        ask_rx,
        active_turn,
        session_command_tx,
        session_update_rx,
        catalog_rx,
    )
    .await?;
    Ok(())
}

/// Entrega a la interfaz el catálogo de models.dev: primero la caché (si existe) y, si falta
/// o ha caducado, uno descargado. Cada catálogo que llega se copia también en `shared`, el que
/// usa el agente para fijar los precios de los apuntes. Los fallos se ignoran en silencio: sin
/// catálogo, `/settings` funciona como antes. Corre en su propia tarea, así que nunca retrasa
/// el arranque.
async fn load_catalog(
    tx: tokio::sync::mpsc::UnboundedSender<Arc<Catalog>>,
    shared: Arc<std::sync::RwLock<Catalog>>,
) {
    let publish = |catalog: Catalog| {
        if let Ok(mut guard) = shared.write() {
            *guard = catalog.clone();
        }
        let _ = tx.send(Arc::new(catalog));
    };
    let Some(path) = cache_path() else {
        return;
    };
    // La lectura de la caché es E/S síncrona: se aparta del hilo del ejecutor.
    let cached = tokio::task::spawn_blocking({
        let path = path.clone();
        move || read_cache(&path)
    })
    .await
    .ok()
    .flatten();

    let needs_refresh = match &cached {
        Some((_, modified)) => is_stale(*modified, SystemTime::now()),
        None => true,
    };
    if let Some((catalog, _)) = cached {
        publish(catalog);
    }
    if needs_refresh {
        if let Ok(catalog) = refresh(&path, || fetch_raw(CATALOG_URL)).await {
            publish(catalog);
        }
    }
}

/// Carga `.env` del directorio de trabajo al entorno del proceso (sin pisar
/// variables ya definidas). Los secretos viven en `.env`, no en la linea de
/// comandos.
fn load_dotenv(dir: &std::path::Path) {
    let _ = dotenvy::from_path(dir.join(".env"));
}

/// Registro de agentes y directorios que vigila, tal como quedan al arrancar.
struct ArranqueAgentes {
    registro: AgentRegistry,
    directorios: Vec<PathBuf>,
}

/// Construye el registro con `default` integrado, la fuente global y la de proyecto, de menor
/// a mayor precedencia. Sin `home` no hay fuente global ni almacén de aprobaciones: los agentes
/// de proyecto quedan pendientes y nunca se guarda una aprobación dentro del repositorio.
fn arrancar_agentes(home: Option<&Path>, raiz: &Path, agents: &AgentsConfig) -> ArranqueAgentes {
    let global = match &agents.global_dir {
        Some(ruta) => resolver_ruta(ruta, home, raiz),
        None => home.map(|h| h.join(".agents").join("agents")),
    };
    let proyecto = match &agents.project_dir {
        Some(ruta) => resolver_ruta(ruta, home, raiz),
        None => Some(raiz.join(".agents").join("agents")),
    };
    let mut fuentes: Vec<Box<dyn AgentSource>> = vec![Box::new(BuiltInSource)];
    let mut directorios = Vec::new();
    if let Some(dir) = global {
        directorios.push(dir.clone());
        fuentes.push(Box::new(FileSource::new(dir, ProfileLevel::Global)));
    }
    if let Some(dir) = proyecto {
        directorios.push(dir.clone());
        fuentes.push(Box::new(FileSource::new(dir, ProfileLevel::Project)));
    }
    let mut registro = AgentRegistry::new(fuentes);
    if let Some(home) = home {
        match TrustStore::open(home.join(".codeloom")) {
            Ok(almacen) => {
                registro =
                    registro.with_trust(almacen, raiz.to_path_buf(), agents.trust_project_agents);
            }
            Err(error) => {
                eprintln!("codeloom: almacén de aprobaciones de agentes no disponible: {error}");
            }
        }
    }
    ArranqueAgentes {
        registro,
        directorios,
    }
}

/// Pone en marcha el consumo de agentes remotos si `[a2a] role` lo pide (A8a). Con el rol apagado
/// no se crea cliente alguno ni se hace petición. Las tarjetas se descargan en segundo plano para
/// no retrasar el arranque; un remoto que no responde queda como problema de carga de ese agente
/// y nunca se aprueba solo.
fn arrancar_remotos(
    a2a: &codeloom_a2a::config::A2aConfig,
    registro: &Arc<AgentRegistry>,
    bus: &EventBus,
) -> Result<Option<Arc<dyn codeloom_core::remote_task::RemoteClientFactory>>> {
    let Some(cliente) = codeloom_a2a::remote::http_client(a2a)? else {
        return Ok(None);
    };
    let Some(fabrica) = codeloom_a2a::client::A2aClientFactory::from_config(a2a)? else {
        return Ok(None);
    };
    let (registro, bus, config) = (registro.clone(), bus.clone(), a2a.clone());
    tokio::spawn(async move {
        // El informe se descarta: el registro ya publica el cambio y deja cada fallo como
        // problema de carga de su agente, que es donde la persona usuaria lo ve.
        let _ = codeloom_a2a::sync::refresh_remote_cards(&registro, &bus, &cliente, &config).await;
    });
    Ok(Some(Arc::new(fabrica)))
}

/// Resuelve una ruta de `[agents]`: un `~/` inicial va al directorio personal y una ruta
/// relativa cuelga de la raíz del proyecto. `None` si depende de un home que no existe.
fn resolver_ruta(ruta: &Path, home: Option<&Path>, raiz: &Path) -> Option<PathBuf> {
    let texto = ruta.to_string_lossy();
    if let Some(resto) = texto
        .strip_prefix("~/")
        .or_else(|| texto.strip_prefix("~\\"))
    {
        return home.map(|h| h.join(resto));
    }
    if ruta.is_absolute() {
        Some(ruta.to_path_buf())
    } else {
        Some(raiz.join(ruta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeloom_core::registry::Approval;

    fn test_config() -> CodeLoomConfig {
        CodeLoomConfig {
            provider: codeloom_core::config::ProviderConfig {
                kind: ProviderKind::OpenAiCompat,
                base_url: "https://old.test/v1".into(),
                api_key_env: "OLD_KEY".into(),
            },
            model: "old-model".into(),
            max_steps: 8,
            effort: None,
        }
    }

    #[test]
    fn loads_env_from_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "CODELOOM_TEST_ENV_VAR=hello\n").unwrap();
        load_dotenv(dir.path());
        assert_eq!(std::env::var("CODELOOM_TEST_ENV_VAR").unwrap(), "hello");
    }

    #[test]
    fn validates_provider_settings_before_persisting() {
        let current = test_config();
        let settings = ProviderSettings {
            kind: ProviderKind::Anthropic,
            model: "  claude-test  ".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: None,
        };
        let updated = config_from_provider_settings(settings.clone(), &current).unwrap();
        assert_eq!(updated.model, "claude-test");
        assert_eq!(updated.provider.kind, ProviderKind::Anthropic);

        let mut invalid = settings;
        invalid.base_url = "https://".into();
        assert!(config_from_provider_settings(invalid, &current).is_err());
    }

    #[test]
    fn settings_effort_reaches_the_config() {
        let current = test_config();
        let settings = ProviderSettings {
            kind: ProviderKind::Anthropic,
            model: "claude-think".into(),
            base_url: "https://api.anthropic.com/v1".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            effort: Some("high".into()),
        };
        let updated = config_from_provider_settings(settings, &current).unwrap();
        assert_eq!(updated.effort.as_deref(), Some("high"));
    }

    fn perfil_de_prueba(nombre: &str) -> String {
        format!("---\nname: {nombre}\ndescription: Agente de prueba\n---\n\nCuerpo.\n")
    }

    #[test]
    fn startup_without_home_directory_does_not_panic() {
        let proyecto = tempfile::tempdir().unwrap();
        let agentes_dir = proyecto.path().join(".agents").join("agents");
        std::fs::create_dir_all(&agentes_dir).unwrap();
        std::fs::write(agentes_dir.join("revisor.md"), perfil_de_prueba("revisor")).unwrap();

        let arranque = arrancar_agentes(None, proyecto.path(), &AgentsConfig::default());

        // Sin home: ni fuente global ni vigilancia global, y el proyecto se sigue leyendo.
        assert_eq!(arranque.directorios, vec![agentes_dir]);
        let instantanea = arranque.registro.snapshot();
        assert!(instantanea.get("default").is_some());
        assert!(instantanea.get("revisor").is_some());
        // Sin home no hay almacén de aprobaciones: el agente de proyecto queda pendiente.
        assert_eq!(instantanea.approval("revisor"), Some(Approval::Pending));
    }

    #[test]
    fn startup_with_home_reads_global_and_project_agents() {
        let home = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let global_dir = home.path().join(".agents").join("agents");
        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::write(global_dir.join("global.md"), perfil_de_prueba("global")).unwrap();
        let proyecto_dir = proyecto.path().join(".agents").join("agents");
        std::fs::create_dir_all(&proyecto_dir).unwrap();
        std::fs::write(proyecto_dir.join("local.md"), perfil_de_prueba("local")).unwrap();

        let arranque =
            arrancar_agentes(Some(home.path()), proyecto.path(), &AgentsConfig::default());

        assert_eq!(arranque.directorios, vec![global_dir, proyecto_dir]);
        let instantanea = arranque.registro.snapshot();
        assert_eq!(instantanea.approval("global"), Some(Approval::NotRequired));
        assert_eq!(instantanea.approval("local"), Some(Approval::Pending));
        // El almacén de aprobaciones vive en el directorio de datos del usuario, nunca en el proyecto.
        assert!(home.path().join(".codeloom").is_dir());
    }

    #[test]
    fn agents_config_overrides_the_default_directories() {
        let home = tempfile::tempdir().unwrap();
        let proyecto = tempfile::tempdir().unwrap();
        let agents = AgentsConfig {
            global_dir: Some(PathBuf::from("~/mis-agentes")),
            project_dir: Some(PathBuf::from("config/agentes")),
            trust_project_agents: true,
            ..AgentsConfig::default()
        };

        let arranque = arrancar_agentes(Some(home.path()), proyecto.path(), &agents);

        assert_eq!(
            arranque.directorios,
            vec![
                home.path().join("mis-agentes"),
                proyecto.path().join("config").join("agentes"),
            ]
        );
    }

    /// Registro con un único remoto `lejano` apuntando a `endpoint`, y el directorio que lo guarda.
    fn registro_con_remoto(endpoint: &str) -> (Arc<AgentRegistry>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lejano.md"),
            format!(
                "---
name: lejano
description: Alta manual
transport: a2a-remote
a2a:
  endpoint: {endpoint}
---
"
            ),
        )
        .unwrap();
        let registro = Arc::new(AgentRegistry::new(vec![
            Box::new(BuiltInSource),
            Box::new(FileSource::new(
                dir.path().to_path_buf(),
                ProfileLevel::Global,
            )),
        ]));
        (registro, dir)
    }

    #[tokio::test]
    async fn with_the_a2a_role_off_no_remote_machinery_is_started() {
        let servidor = wiremock::MockServer::start().await;
        let (registro, _dir) = registro_con_remoto(&servidor.uri());

        let fabrica = arrancar_remotos(
            &codeloom_a2a::config::A2aConfig::default(),
            &registro,
            &EventBus::new(16),
        )
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert!(fabrica.is_none());
        assert!(servidor.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn with_the_a2a_role_consume_the_remote_card_is_fetched_and_never_approved() {
        use wiremock::matchers::{method, path};
        let servidor = wiremock::MockServer::start().await;
        let mut tarjeta: serde_json::Value = serde_json::from_str(include_str!(
            "../../codeloom-a2a/tests/fixtures/agent-card.sample.json"
        ))
        .unwrap();
        for interfaz in tarjeta["supportedInterfaces"].as_array_mut().unwrap() {
            interfaz["url"] = serde_json::json!(format!("{}/a2a/v1", servidor.uri()));
        }
        tarjeta
            .as_object_mut()
            .unwrap()
            .remove("securityRequirements");
        wiremock::Mock::given(method("GET"))
            .and(path("/.well-known/agent-card.json"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_json(tarjeta))
            .mount(&servidor)
            .await;
        let (registro, _dir) = registro_con_remoto(&servidor.uri());
        let a2a = codeloom_a2a::config::A2aConfig {
            role: codeloom_a2a::config::A2aRole::Consume,
            request_timeout_secs: 2,
            ..Default::default()
        };

        let fabrica = arrancar_remotos(&a2a, &registro, &EventBus::new(16)).unwrap();

        assert!(fabrica.is_some());
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let descargada = registro.snapshot().entries().into_iter().any(|e| {
                    matches!(&e.execution, codeloom_core::registry::Execution::Remote(spec)
                        if e.name == "lejano" && spec.fetched.is_some())
                });
                if descargada {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("la tarjeta debería descargarse y anotarse en el registro");
        assert_eq!(
            registro.snapshot().approval("lejano"),
            Some(Approval::Pending),
            "descargar la tarjeta no aprueba nada"
        );
    }
}
