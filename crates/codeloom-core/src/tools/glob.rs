use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

/// Máximo de coincidencias que se devuelven al modelo.
const MAX_HITS: usize = 300;

pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }
    fn description(&self) -> &str {
        "Find files by glob pattern, relative to the workspace, honouring .gitignore. Params: pattern (string). At most 300 matches are returned: narrow the pattern."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({"type":"object","properties":{"pattern":{"type":"string"}},"required":["pattern"]})
    }
    fn execution_mode(&self, _i: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Parallel
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        _c: CancellationToken,
    ) -> Result<ToolOutput> {
        let pattern = input["pattern"].as_str().unwrap_or_default().to_string();
        let pat = glob::Pattern::new(&pattern).map_err(|e| CoreError::Tool(e.to_string()))?;
        let cwd = ctx.cwd.clone();
        // Se recorre respetando `.gitignore` y sin entrar en `.git`: un `**/*` no debe
        // volcar `target/` ni `node_modules/` al modelo, que lo pagaría en tokens.
        let (mut hits, total) = tokio::task::spawn_blocking(move || {
            let opts = glob::MatchOptions {
                require_literal_separator: true,
                ..Default::default()
            };
            let mut hits = Vec::new();
            let mut total = 0usize;
            let walk = ignore::WalkBuilder::new(&cwd)
                .hidden(false)
                .filter_entry(|e| e.file_name() != ".git")
                .build();
            for entry in walk.flatten() {
                let Ok(rel) = entry.path().strip_prefix(&cwd) else {
                    continue;
                };
                if rel.as_os_str().is_empty() || !pat.matches_path_with(rel, opts) {
                    continue;
                }
                total += 1;
                if hits.len() < MAX_HITS {
                    hits.push(entry.path().display().to_string());
                }
            }
            (hits, total)
        })
        .await
        .map_err(|e| CoreError::Tool(e.to_string()))?;
        hits.sort();
        if total > hits.len() {
            hits.push(format!(
                "[{} of {total} matches shown; narrow the pattern]",
                hits.len()
            ));
        }
        Ok(ToolOutput::ok(hits.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::PermissionService;
    use std::sync::Arc;

    fn ctx(dir: &std::path::Path) -> ToolCtx {
        ToolCtx {
            cwd: dir.to_path_buf(),
            permission: Arc::new(PermissionService::allow_all()),
            session: crate::session::SessionId(0),
            jobs: Arc::new(crate::job::LocalJobRegistry::default()),
            bus: crate::event::EventBus::new(8),
            subagents: None,
        }
    }

    #[tokio::test]
    async fn glob_finds_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        let out = GlobTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"*.rs"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("a.rs"));
        assert!(!out.content.contains("b.txt"));
    }

    #[tokio::test]
    async fn glob_skips_gitignored_and_caps_matches() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(
            dir.path().join(".gitignore"),
            "target/
",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target").join("junk.o"), "").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        for i in 0..(MAX_HITS + 20) {
            std::fs::write(dir.path().join("src").join(format!("f{i}.rs")), "").unwrap();
        }
        let out = GlobTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"**/*"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(
            !out.content.contains("junk.o"),
            "target/ está en .gitignore"
        );
        assert!(!out.content.contains(".git/"), "nunca se entra en .git");
        assert!(out.content.contains("matches shown; narrow the pattern"));
        assert!(out.content.lines().count() <= MAX_HITS + 1);
    }

    /// Peor caso real (regla 10 del decálogo de contexto): el incidente fue un `**/*` sobre este
    /// mismo repositorio, con `target/` dentro. Se ignora por defecto porque depende del disco.
    #[tokio::test]
    #[ignore = "recorre el repositorio real; ejecutar con --ignored"]
    async fn glob_everything_on_the_real_repository_stays_small() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_path_buf();
        let out = GlobTool
            .run(
                ctx(&raiz),
                serde_json::json!({"pattern":"**/*"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        println!(
            "glob **/* -> {} bytes, {} líneas",
            out.content.len(),
            out.content.lines().count()
        );
        assert!(out.content.len() < 60_000, "{} bytes", out.content.len());
        assert!(!out.content.contains("target"), "no debe recorrer target/");
    }
}
