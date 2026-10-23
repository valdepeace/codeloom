use crate::error::{CoreError, Result};
use serde::Deserialize;
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

pub mod context;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "openai_compat")]
    OpenAiCompat,
    #[serde(rename = "anthropic")]
    Anthropic,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodeLoomConfig {
    pub provider: ProviderConfig,
    pub model: String,
    #[serde(default = "default_max_steps")]
    pub max_steps: usize,
    /// Nivel de esfuerzo de razonamiento; `None` equivale a «default» y no se escribe en el fichero.
    #[serde(default)]
    pub effort: Option<String>,
}

fn default_max_steps() -> usize {
    30
}

/// Modo de confirmación al delegar en un subagente (`[agents] confirm_delegation`).
///
/// Solo cambia el texto de la regla que el prompt del padre recibe; no es una garantía
/// de comportamiento, pues el cumplimiento depende del modelo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmDelegation {
    /// Pregunta a la persona usuaria solo cuando duda entre varios agentes. Es el valor por defecto.
    #[default]
    Doubt,
    /// Propone la asignación de cada tarea y espera la aprobación antes de delegar.
    Always,
    /// Delega sin preguntar y explica la elección en su respuesta.
    Never,
}

/// Elección automática de agente al delegar (`[agents] routing`, A12b).
///
/// Cabe señalar que A2A no fija cómo se elige un agente: es decisión propia, y por eso va apagada
/// por defecto hasta que se mida lo que ahorra.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    /// Sin enrutador: el padre elige como hasta ahora. Es el valor por defecto.
    #[default]
    Off,
    /// Regla determinista sobre las etiquetas de las tarjetas, sin llamar al modelo.
    Rule,
    /// La regla primero y, si se abstiene, un clasificador barato con salida estricta.
    Classifier,
}

/// Sección opcional `[agents]` de `codeloom.toml`. Sin ella, todo son valores por defecto.
/// Vive aparte de `CodeLoomConfig` para que su ausencia no afecte a la configuración principal.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AgentsConfig {
    /// Directorio de agentes globales. Un `~/` inicial se expande al directorio personal.
    pub global_dir: Option<PathBuf>,
    /// Directorio de agentes del proyecto. Una ruta relativa cuelga de la raíz del proyecto.
    pub project_dir: Option<PathBuf>,
    /// Aprueba todos los agentes de proyecto sin preguntar. Por defecto, desactivado.
    pub trust_project_agents: bool,
    /// Cuándo se pregunta a la persona usuaria antes de delegar. Por defecto, `doubt`.
    pub confirm_delegation: ConfirmDelegation,
    /// Elección automática de agente al delegar. Por defecto, `off`.
    pub routing: RoutingMode,
    /// Modelo del clasificador (solo con `routing = "classifier"`). Sin él, el del padre.
    pub routing_model: Option<String>,
}

/// Forma del fichero a efectos de la sección `[agents]`; el resto de claves se ignora.
#[derive(Deserialize)]
struct FicheroConAgentes {
    #[serde(default)]
    agents: AgentsConfig,
}

impl AgentsConfig {
    /// Lee la sección `[agents]` de un texto TOML. Su ausencia da los valores por defecto.
    pub fn from_toml(text: &str) -> Result<AgentsConfig> {
        toml::from_str::<FicheroConAgentes>(text)
            .map(|fichero| fichero.agents)
            .map_err(|e| CoreError::Config(e.to_string()))
    }

    pub fn load(path: &Path) -> Result<AgentsConfig> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CoreError::Config(format!("read {}: {e}", path.display())))?;
        Self::from_toml(&text)
    }
}

pub fn validate_base_url(input: &str) -> std::result::Result<(), String> {
    let url = reqwest::Url::parse(input).map_err(|_| "Base URL is invalid".to_string())?;
    let local_http = url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
        });
    if url.scheme() != "https" && !local_http {
        return Err("Use HTTPS, or HTTP for a localhost endpoint".into());
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err(
            "Keep credentials out of the Base URL; use the API key environment variable".into(),
        );
    }
    Ok(())
}

impl CodeLoomConfig {
    pub fn load(path: &Path) -> Result<CodeLoomConfig> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CoreError::Config(format!("read {}: {e}", path.display())))?;
        toml::from_str(&text).map_err(|e| CoreError::Config(e.to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut document = std::fs::read_to_string(path)
            .map_err(|e| CoreError::Config(format!("read {}: {e}", path.display())))?
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| CoreError::Config(format!("parse {}: {e}", path.display())))?;
        document["model"] = toml_edit::value(self.model.clone());
        document["provider"]["kind"] = toml_edit::value(match self.provider.kind {
            ProviderKind::OpenAiCompat => "openai_compat",
            ProviderKind::Anthropic => "anthropic",
        });
        document["provider"]["base_url"] = toml_edit::value(self.provider.base_url.clone());
        document["provider"]["api_key_env"] = toml_edit::value(self.provider.api_key_env.clone());
        match &self.effort {
            Some(effort) => document["effort"] = toml_edit::value(effort.clone()),
            None => {
                document.remove("effort");
            }
        }
        let text = document.to_string();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let mut temp_file = None;
        for attempt in 0..100 {
            let candidate = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), attempt));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => {
                    temp_file = Some((candidate, file));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(CoreError::Config(format!(
                        "create config temp file: {error}"
                    )))
                }
            }
        }
        let (temp_path, mut temp) = temp_file
            .ok_or_else(|| CoreError::Config("cannot create temporary config file".into()))?;
        let result = (|| {
            temp.write_all(text.as_bytes())?;
            temp.sync_all()?;
            std::fs::rename(&temp_path, path)?;
            Ok::<_, std::io::Error>(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        result.map_err(|e| CoreError::Config(format!("write {}: {e}", path.display())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parses_minimal_config() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"
            model = "deepseek-chat"
            provider = {{ kind = "openai_compat", base_url = "https://api.deepseek.com", api_key_env = "DEEPSEEK_API_KEY" }}
            max_steps = 20
        "#).unwrap();
        let cfg = CodeLoomConfig::load(f.path()).unwrap();
        assert_eq!(cfg.model, "deepseek-chat");
        assert_eq!(cfg.max_steps, 20);
        assert_eq!(cfg.provider.kind, ProviderKind::OpenAiCompat);
    }

    #[test]
    fn defaults_max_steps() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"
            model = "gpt-4o"
            provider = {{ kind = "openai_compat", base_url = "https://api.openai.com", api_key_env = "OPENAI_API_KEY" }}
        "#).unwrap();
        let cfg = CodeLoomConfig::load(f.path()).unwrap();
        assert_eq!(cfg.max_steps, 30);
    }

    #[test]
    fn saves_and_reloads_provider_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codeloom.toml");
        std::fs::write(
            &path,
            "# preserved\nmodel = \"old\"\nmax_steps = 7\n[provider]\nkind = \"openai_compat\"\nbase_url = \"https://old.test\"\napi_key_env = \"OLD_KEY\"\n",
        )
        .unwrap();
        let config = CodeLoomConfig {
            provider: ProviderConfig {
                kind: ProviderKind::Anthropic,
                base_url: "https://api.anthropic.com".into(),
                api_key_env: "ANTHROPIC_API_KEY".into(),
            },
            model: "claude-test".into(),
            max_steps: 7,
            effort: None,
        };
        config.save(&path).unwrap();
        let loaded = CodeLoomConfig::load(&path).unwrap();
        assert_eq!(loaded.model, "claude-test");
        assert_eq!(loaded.provider.kind, ProviderKind::Anthropic);
        assert_eq!(loaded.provider.api_key_env, "ANTHROPIC_API_KEY");
        assert_eq!(loaded.max_steps, 7);
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("# preserved"));
    }

    #[test]
    fn config_without_effort_still_loads() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        write!(f, r#"
            model = "gpt-4o"
            provider = {{ kind = "openai_compat", base_url = "https://api.openai.com", api_key_env = "OPENAI_API_KEY" }}
        "#).unwrap();
        let cfg = CodeLoomConfig::load(f.path()).unwrap();
        assert_eq!(cfg.effort, None, "una config antigua equivale a «default»");
    }

    #[test]
    fn config_roundtrips_effort() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("codeloom.toml");
        std::fs::write(
            &path,
            "# preserved\nmodel = \"old\"\nmax_steps = 7\n[provider]\nkind = \"openai_compat\"\nbase_url = \"https://old.test\"\napi_key_env = \"OLD_KEY\"\n",
        )
        .unwrap();
        let mut config = CodeLoomConfig {
            provider: ProviderConfig {
                kind: ProviderKind::Anthropic,
                base_url: "https://api.anthropic.com".into(),
                api_key_env: "ANTHROPIC_API_KEY".into(),
            },
            model: "claude-test".into(),
            max_steps: 7,
            effort: Some("high".into()),
        };
        config.save(&path).unwrap();
        let loaded = CodeLoomConfig::load(&path).unwrap();
        assert_eq!(loaded.effort.as_deref(), Some("high"));
        assert_eq!(loaded.provider.kind, ProviderKind::Anthropic);

        // Con `None` la clave desaparece del fichero y al releerlo no hay esfuerzo.
        config.effort = None;
        config.save(&path).unwrap();
        let loaded = CodeLoomConfig::load(&path).unwrap();
        assert_eq!(loaded.effort, None);
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("# preserved"));
    }

    #[test]
    fn accepts_secure_or_local_provider_urls_without_embedded_credentials() {
        assert!(validate_base_url("https://api.example.test/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8000/v1").is_ok());
        assert!(validate_base_url("http://api.example.test/v1").is_err());
        assert!(validate_base_url("https://user:secret@example.test/v1").is_err());
    }

    #[test]
    fn agents_section_is_optional_in_config() {
        let text = "model = \"gpt-4o\"\nprovider = { kind = \"openai_compat\", base_url = \"https://api.openai.com\", api_key_env = \"OPENAI_API_KEY\" }\n";
        let agents = AgentsConfig::from_toml(text).unwrap();
        assert_eq!(agents, AgentsConfig::default());
        assert_eq!(agents.global_dir, None);
        assert_eq!(agents.project_dir, None);
        assert!(
            !agents.trust_project_agents,
            "por defecto no se confía en proyectos"
        );
    }

    #[test]
    fn agents_config_parses_dirs_and_trust_flag() {
        let text = r#"
            model = "gpt-4o"
            provider = { kind = "openai_compat", base_url = "https://api.openai.com", api_key_env = "OPENAI_API_KEY" }
            [agents]
            global_dir = "~/mis-agentes"
            project_dir = "config/agentes"
            trust_project_agents = true
        "#;
        let agents = AgentsConfig::from_toml(text).unwrap();
        assert_eq!(agents.global_dir, Some(PathBuf::from("~/mis-agentes")));
        assert_eq!(agents.project_dir, Some(PathBuf::from("config/agentes")));
        assert!(agents.trust_project_agents);
        // La sección convive con el resto: la configuración principal sigue leyéndose igual.
        let cfg: CodeLoomConfig = toml::from_str(text).unwrap();
        assert_eq!(cfg.model, "gpt-4o");
    }

    #[test]
    fn confirm_delegation_defaults_to_doubt() {
        let sin_seccion = "model = \"gpt-4o\"\nprovider = { kind = \"openai_compat\", base_url = \"https://api.openai.com\", api_key_env = \"OPENAI_API_KEY\" }\n";
        let agents = AgentsConfig::from_toml(sin_seccion).unwrap();
        assert_eq!(agents.confirm_delegation, ConfirmDelegation::Doubt);

        let sin_clave = format!("{sin_seccion}[agents]\ntrust_project_agents = false\n");
        let agents = AgentsConfig::from_toml(&sin_clave).unwrap();
        assert_eq!(agents.confirm_delegation, ConfirmDelegation::Doubt);
    }

    #[test]
    fn confirm_delegation_reads_the_three_modes() {
        let casos = [
            ("doubt", ConfirmDelegation::Doubt),
            ("always", ConfirmDelegation::Always),
            ("never", ConfirmDelegation::Never),
        ];
        for (valor, esperado) in casos {
            let text = format!("[agents]\nconfirm_delegation = \"{valor}\"\n");
            let agents = AgentsConfig::from_toml(&text).unwrap();
            assert_eq!(agents.confirm_delegation, esperado, "valor {valor}");
        }
    }

    #[test]
    fn routing_defaults_to_off_and_reads_its_modes_and_model() {
        let agents = AgentsConfig::from_toml("").unwrap();
        assert_eq!(agents.routing, RoutingMode::Off);
        assert_eq!(agents.routing_model, None);

        for (valor, esperado) in [
            ("off", RoutingMode::Off),
            ("rule", RoutingMode::Rule),
            ("classifier", RoutingMode::Classifier),
        ] {
            let text = format!("[agents]\nrouting = \"{valor}\"\nrouting_model = \"barato\"\n");
            let agents = AgentsConfig::from_toml(&text).unwrap();
            assert_eq!(agents.routing, esperado, "valor {valor}");
            assert_eq!(agents.routing_model.as_deref(), Some("barato"));
        }
    }

    #[test]
    fn routing_rejects_unknown_values() {
        let error = AgentsConfig::from_toml("[agents]\nrouting = \"magic\"\n").unwrap_err();
        assert!(matches!(error, CoreError::Config(_)), "got {error:?}");
    }

    #[test]
    fn confirm_delegation_rejects_unknown_values() {
        let text = "[agents]\nconfirm_delegation = \"sometimes\"\n";
        let error = AgentsConfig::from_toml(text).unwrap_err();
        assert!(matches!(error, CoreError::Config(_)), "got {error:?}");
    }
}
