//! Lectura confinada para el agente servido (A8b).
//!
//! Cabe señalar que `ViewTool` resuelve la ruta con `cwd.join(path)`, y `join` con una ruta absoluta
//! sustituye la base: un tercero podría pedir `C:\Users\…\.ssh\id_rsa` o `../../secreto`. Quien
//! sirve el agente a otros no puede permitirlo, así que `view` se envuelve aquí: la ruta ha de ser
//! relativa, sin `..`, y su forma canónica (tras resolver enlaces simbólicos) ha de quedar dentro del
//! directorio de trabajo; además se vedan por nombre los ficheros que suelen guardar secretos y se
//! acota el tamaño del fichero.
//!
//! La lista de nombres vedados es un primer filtro y **no** una garantía: lo que de verdad protege es
//! el confinamiento al directorio y la autenticación del servidor.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use codeloom_core::error::{CoreError, Result};
use codeloom_core::tools::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use tokio_util::sync::CancellationToken;

/// Tamaño máximo de un fichero que el agente servido puede leer.
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Directorios que nunca se leen: guardan credenciales o configuración con ellas.
const DIRECTORIOS_VEDADOS: &[&str] = &[".git", ".ssh", ".gnupg", ".aws", ".azure", ".kube"];

fn nombre_vedado(nombre: &str) -> bool {
    let n = nombre.to_ascii_lowercase();
    n.starts_with(".env")
        || n.starts_with("id_rsa")
        || n.starts_with("id_ed25519")
        || n.starts_with("id_ecdsa")
        || n.ends_with(".pem")
        || n.ends_with(".key")
        || n.ends_with(".p12")
        || n.ends_with(".pfx")
        || matches!(
            n.as_str(),
            "credentials" | ".netrc" | ".npmrc" | ".pypirc" | ".pgpass"
        )
}

fn vedado(ruta: &Path) -> bool {
    ruta.components().any(|componente| match componente {
        Component::Normal(parte) => {
            let parte = parte.to_string_lossy();
            DIRECTORIOS_VEDADOS
                .iter()
                .any(|v| parte.eq_ignore_ascii_case(v))
                || nombre_vedado(&parte)
        }
        _ => false,
    })
}

/// Resuelve `relativa` dentro de `cwd` o explica por qué no se puede leer. Devuelve la ruta canónica.
pub fn confine_path(cwd: &Path, relativa: &str) -> std::result::Result<PathBuf, String> {
    let pedida = Path::new(relativa);
    if relativa.is_empty() || relativa.contains(':') || relativa.contains('\0') {
        return Err("path must be relative to the workspace".into());
    }
    for componente in pedida.components() {
        match componente {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err("path must be relative to the workspace and cannot use `..`".into()),
        }
    }
    if vedado(pedida) {
        return Err("this path is not readable".into());
    }
    let base = cwd
        .canonicalize()
        .map_err(|_| "workspace is not available".to_string())?;
    let canonica = base
        .join(pedida)
        .canonicalize()
        .map_err(|_| "file not found".to_string())?;
    let dentro = canonica
        .strip_prefix(&base)
        .map_err(|_| "path resolves outside the workspace".to_string())?;
    // Un enlace simbólico puede llevar a un fichero vedado dentro del propio proyecto.
    if vedado(dentro) {
        return Err("this path is not readable".into());
    }
    let metadatos = std::fs::metadata(&canonica).map_err(|_| "file not found".to_string())?;
    if !metadatos.is_file() {
        return Err("path is not a file".into());
    }
    if metadatos.len() > MAX_FILE_BYTES {
        return Err(format!("file is larger than {MAX_FILE_BYTES} bytes"));
    }
    Ok(canonica)
}

/// `view` con las rutas confinadas al directorio de trabajo.
pub struct ConfinedView<T: Tool>(pub T);

#[async_trait]
impl<T: Tool> Tool for ConfinedView<T> {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn description(&self) -> &str {
        self.0.description()
    }
    fn schema(&self) -> serde_json::Value {
        self.0.schema()
    }
    fn execution_mode(&self, input: &serde_json::Value) -> ExecutionMode {
        self.0.execution_mode(input)
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        mut input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        let pedida = input["path"].as_str().unwrap_or_default().to_string();
        let canonica = confine_path(&ctx.cwd, &pedida)
            .map_err(|motivo| CoreError::Tool(format!("{}: {motivo}", self.0.name())))?;
        // Se pasa la ruta canónica y absoluta: así lo que se lee es lo que se comprobó.
        input["path"] = serde_json::Value::String(canonica.to_string_lossy().into_owned());
        self.0.run(ctx, input, cancel).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codeloom_core::tools::view::ViewTool;
    use std::sync::Arc;

    fn proyecto() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "fn main() {}\n").unwrap();
        std::fs::write(dir.path().join(".env"), "OPENROUTER_API_KEY=sk-secreto\n").unwrap();
        dir
    }

    #[test]
    fn a_relative_file_inside_the_workspace_is_readable() {
        let dir = proyecto();

        let ruta = confine_path(dir.path(), "src/lib.rs").unwrap();

        assert!(ruta.ends_with("lib.rs"));
        assert!(confine_path(dir.path(), "./src/lib.rs").is_ok());
    }

    #[test]
    fn absolute_paths_and_parent_components_are_refused() {
        let dir = proyecto();
        let fuera = dir.path().parent().unwrap().join("fuera.txt");
        std::fs::write(&fuera, "secreto").unwrap();
        for malo in [
            fuera.to_string_lossy().into_owned(),
            "/etc/passwd".to_string(),
            "C:\\Windows\\win.ini".to_string(),
            "\\\\servidor\\recurso\\f".to_string(),
            "../fuera.txt".to_string(),
            "src/../../fuera.txt".to_string(),
            "src/..\\..\\fuera.txt".to_string(),
            "lib.rs:flujo".to_string(),
            String::new(),
        ] {
            assert!(confine_path(dir.path(), &malo).is_err(), "{malo:?}");
        }
        std::fs::remove_file(fuera).ok();
    }

    #[test]
    fn files_that_usually_hold_secrets_are_refused_by_name() {
        let dir = proyecto();
        for nombre in [
            ".env",
            ".env.local",
            ".ENV",
            ".envrc",
            "id_rsa",
            "id_ed25519.pub",
            "clave.pem",
            "tls.KEY",
            ".npmrc",
            ".git/config",
            "src/.git/HEAD",
            ".ssh/known_hosts",
        ] {
            let ruta = dir.path().join(nombre);
            std::fs::create_dir_all(ruta.parent().unwrap()).unwrap();
            std::fs::write(&ruta, "x").unwrap();

            assert!(confine_path(dir.path(), nombre).is_err(), "{nombre}");
        }
    }

    #[test]
    fn a_missing_file_a_directory_and_a_huge_file_are_refused() {
        let dir = proyecto();
        let enorme = dir.path().join("enorme.bin");
        let fichero = std::fs::File::create(&enorme).unwrap();
        fichero.set_len(MAX_FILE_BYTES + 1).unwrap();

        assert!(confine_path(dir.path(), "no-existe.rs").is_err());
        assert!(confine_path(dir.path(), "src").is_err());
        assert!(confine_path(dir.path(), "enorme.bin")
            .unwrap_err()
            .contains("larger"));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_that_leaves_the_workspace_or_reaches_a_secret_is_refused() {
        let dir = proyecto();
        let fuera = tempfile::tempdir().unwrap();
        std::fs::write(fuera.path().join("otro.txt"), "secreto").unwrap();
        std::os::unix::fs::symlink(fuera.path().join("otro.txt"), dir.path().join("enlace"))
            .unwrap();
        std::os::unix::fs::symlink(dir.path().join(".env"), dir.path().join("atajo")).unwrap();

        assert!(confine_path(dir.path(), "enlace").is_err());
        assert!(confine_path(dir.path(), "atajo").is_err());
    }

    #[tokio::test]
    async fn the_wrapped_view_reads_inside_and_never_outside() {
        let dir = proyecto();
        let herramienta = ConfinedView(ViewTool);
        let ctx = || ToolCtx {
            cwd: dir.path().to_path_buf(),
            permission: Arc::new(codeloom_core::permission::PermissionService::allow_all()),
            session: codeloom_core::session::SessionId(0),
            jobs: Arc::new(codeloom_core::job::LocalJobRegistry::default()),
            bus: codeloom_core::event::EventBus::new(8),
            subagents: None,
        };

        let bien = herramienta
            .run(
                ctx(),
                serde_json::json!({"path": "src/lib.rs"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(bien.content.contains("fn main"));

        for malo in [".env", "../x", "/etc/passwd"] {
            let error = herramienta
                .run(
                    ctx(),
                    serde_json::json!({"path": malo}),
                    CancellationToken::new(),
                )
                .await
                .err()
                .unwrap_or_else(|| panic!("{malo} debía rechazarse"));
            assert!(!error.to_string().contains("sk-secreto"));
        }
    }
}
