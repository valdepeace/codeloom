use serde::Deserialize;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

pub struct HerdrContext {
    pub workspace_cwd: Option<PathBuf>,
    reporter: Option<Reporter>,
}

struct Reporter {
    binary: OsString,
    pane: OsString,
    source: String,
    sequence: u64,
}

#[derive(Clone, Copy)]
pub enum AgentState {
    Idle,
    Working,
    Blocked,
}

impl AgentState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Deserialize)]
struct InvocationContext {
    workspace_cwd: Option<PathBuf>,
}

impl HerdrContext {
    pub fn from_env() -> Option<Self> {
        if std::env::var("HERDR_ENV").ok().as_deref() != Some("1") {
            return None;
        }
        let workspace_cwd = std::env::var("HERDR_PLUGIN_CONTEXT_JSON")
            .ok()
            .and_then(|json| serde_json::from_str::<InvocationContext>(&json).ok())
            .and_then(|context| context.workspace_cwd);
        let reporter = match (
            std::env::var_os("HERDR_PANE_ID"),
            std::env::var("HERDR_PLUGIN_ID").ok(),
        ) {
            (Some(pane), Some(plugin)) => Some(Reporter {
                binary: std::env::var_os("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into()),
                pane,
                source: format!("plugin:{plugin}"),
                sequence: 0,
            }),
            _ => None,
        };
        Some(Self {
            workspace_cwd,
            reporter,
        })
    }

    pub async fn report(&mut self, state: AgentState, message: &str) -> Result<(), String> {
        let Some(reporter) = &mut self.reporter else {
            return Ok(());
        };
        reporter.sequence = reporter.sequence.saturating_add(1);
        let status = Command::new(&reporter.binary)
            .args([
                "pane",
                "report-agent",
                "--source",
                &reporter.source,
                "--agent",
                "CodeLoom",
                "--state",
                state.as_str(),
                "--message",
                message,
                "--seq",
                &reporter.sequence.to_string(),
            ])
            .arg(&reporter.pane)
            .stdin(Stdio::null())
            .status()
            .await
            .map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("herdr exited with {status}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_workspace_cwd_from_plugin_invocation_context() {
        let context: InvocationContext =
            serde_json::from_str(r#"{"workspace_cwd":"E:/workspace/project","workspace_id":"w1"}"#)
                .unwrap();
        assert_eq!(
            context.workspace_cwd,
            Some(PathBuf::from("E:/workspace/project"))
        );
    }
}
