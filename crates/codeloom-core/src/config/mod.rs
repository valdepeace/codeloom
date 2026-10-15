use crate::error::{CoreError, Result};
use serde::Deserialize;
use std::path::Path;

pub mod context;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
}

fn default_max_steps() -> usize {
    30
}

impl CodeLoomConfig {
    pub fn load(path: &Path) -> Result<CodeLoomConfig> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CoreError::Config(format!("read {}: {e}", path.display())))?;
        toml::from_str(&text).map_err(|e| CoreError::Config(e.to_string()))
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
}
