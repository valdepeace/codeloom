//! Montaje y arranque del servidor A2A (A8b).
//!
//! Cabe señalar que aquí se reúne lo que el resto de módulos deja suelto: validar la configuración
//! (sin autenticación resuelta no se arranca), comprobar que el agente servido existe y es local,
//! construir un lanzador dedicado con las herramientas de solo lectura y enlazar el puerto. El
//! lanzador es distinto del que usa la conversación: no comparte herramientas ni permisos con ella.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use codeloom_core::config::CodeLoomConfig;
use codeloom_core::error::{CoreError, Result};
use codeloom_core::event::EventBus;
use codeloom_core::job::LocalJobRegistry;
use codeloom_core::permission::{DenyAll, PermissionService};
use codeloom_core::provider::Provider;
use codeloom_core::registry::{AgentRegistry, Execution};
use codeloom_core::session::SessionStore;
use codeloom_core::subagent::InProcessSubagentLauncher;
use tokio_util::sync::CancellationToken;

use super::auth::Authenticator;
use super::card::served_card;
use super::config::ServeConfig;
use super::host::{served_tools, HostLimits, LauncherHost, MAX_RESULT_BYTES};
use super::rpc::ServeState;

/// Lo que el servidor toma prestado del proceso anfitrión.
pub struct ServeDeps {
    pub registry: Arc<AgentRegistry>,
    pub provider: Arc<dyn Provider>,
    pub store: Arc<dyn SessionStore>,
    pub config: CodeLoomConfig,
    pub cwd: PathBuf,
    pub bus: EventBus,
}

/// Valida, monta y arranca el servidor. Devuelve la dirección real de escucha. Falla, sin abrir
/// ningún puerto, si falta la autenticación o su secreto, si la dirección no es de bucle local o si
/// el agente servido no existe o no es local. `env` resuelve las variables de entorno.
pub async fn start(
    serve: &ServeConfig,
    deps: ServeDeps,
    env: &dyn Fn(&str) -> Option<String>,
    apagado: CancellationToken,
) -> Result<SocketAddr> {
    let auth = serve.validate(env)?;
    let addr = serve.listen_addr()?;
    let interfaz = serve.interface_url()?;

    let instantanea = deps.registry.snapshot();
    let entrada = instantanea.get(&serve.agent).ok_or_else(|| {
        CoreError::Config(format!(
            "a2a.serve.agent «{}» no existe en el registro de agentes",
            serve.agent
        ))
    })?;
    if !matches!(entrada.execution, Execution::Local(_)) {
        return Err(CoreError::Config(format!(
            "a2a.serve.agent «{}» no es un agente local: no se reexpone a un tercero",
            serve.agent
        )));
    }
    let tarjeta = served_card(&entrada.card, &interfaz, &auth.scheme);

    // Nadie aprobaría un permiso en nombre de un tercero: lo que pida permiso se deniega.
    let lanzador = Arc::new(
        InProcessSubagentLauncher::new(
            Arc::new(RwLock::new(deps.provider)),
            deps.store.clone(),
            Arc::new(served_tools()),
            Arc::new(PermissionService::with_responder(Box::new(DenyAll))),
            Arc::new(RwLock::new(deps.config)),
            deps.cwd.clone(),
            Arc::new(LocalJobRegistry::default()),
            deps.bus.clone(),
        )
        .with_registry(deps.registry.clone()),
    );
    let anfitrion = LauncherHost::new(
        lanzador,
        deps.store.as_ref(),
        &deps.cwd.to_string_lossy(),
        serve.agent.clone(),
        HostLimits::from_config(serve, MAX_RESULT_BYTES),
    )
    .await?
    .with_bus(deps.bus);

    let estado = Arc::new(ServeState {
        host: Arc::new(anfitrion),
        auth: Authenticator::new(auth),
        card: tarjeta,
        max_wait: std::time::Duration::from_secs(serve.max_wait_secs),
    });
    super::serve(estado, addr, apagado)
        .await
        .map_err(|e| CoreError::Config(format!("no se pudo escuchar en {addr}: {e}")))
}
