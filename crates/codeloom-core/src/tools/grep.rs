use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct GrepTool;

/// Máximo de coincidencias que se devuelven al modelo; el resto se cuenta, no se muestra.
const MAX_GREP_MATCHES: usize = 200;
/// Caracteres que se conservan de cada línea coincidente.
const MAX_GREP_LINE_CHARS: usize = 300;

/// Recorta `line` a `max` caracteres por una frontera de carácter, marcando el corte.
fn cut_line(line: &str, max: usize) -> String {
    match line.char_indices().nth(max) {
        Some((end, _)) => format!("{} [line truncated]", &line[..end]),
        None => line.to_string(),
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }
    fn description(&self) -> &str {
        "Search file contents by regex. Params: pattern (string), glob (optional string). \
         Returns at most 200 matching lines, each cut to 300 characters; when there are more \
         matches, a final line says how many there were in total."
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type":"object",
            "properties":{"pattern":{"type":"string"},"glob":{"type":"string"}},
            "required":["pattern"]
        })
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
        let pattern = input["pattern"].as_str().unwrap_or_default();
        let re = regex::Regex::new(pattern).map_err(|e| CoreError::Tool(e.to_string()))?;
        let cwd = ctx.cwd.clone();
        let (hits, total) = tokio::task::spawn_blocking(move || {
            let mut shown = Vec::new();
            let mut total = 0usize;
            for entry in ignore::WalkBuilder::new(&cwd).build().flatten() {
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(entry.path()) else {
                    continue;
                };
                for (i, line) in content.lines().enumerate() {
                    if re.is_match(line) {
                        total += 1;
                        if shown.len() < MAX_GREP_MATCHES {
                            shown.push(format!(
                                "{}:{}: {}",
                                entry.path().display(),
                                i + 1,
                                cut_line(line, MAX_GREP_LINE_CHARS)
                            ));
                        }
                    }
                }
            }
            (shown, total)
        })
        .await
        .map_err(|e| CoreError::Tool(e.to_string()))?;
        let mut text = hits.join("\n");
        if total > hits.len() {
            text.push_str(&format!(
                "\n[{} of {total} matches shown; narrow the pattern]",
                hits.len()
            ));
        }
        Ok(ToolOutput::ok(text))
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
    async fn grep_finds_matching_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "foo\nbar\nfoobar\n").unwrap();
        let out = GrepTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"foo"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains("a.txt:1: foo"));
        assert!(out.content.contains("a.txt:3: foobar"));
    }

    #[tokio::test]
    async fn grep_caps_matches_and_long_lines() {
        let dir = tempfile::tempdir().unwrap();
        // 250 coincidencias: la primera es una línea de 1 000 caracteres (ASCII) y el resto son cortas.
        let mut text = format!("{}hit\n", "x".repeat(1000));
        for i in 0..249 {
            text.push_str(&format!("hit {i}\n"));
        }
        std::fs::write(dir.path().join("a.txt"), text).unwrap();
        let out = GrepTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"hit"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        // 200 líneas de coincidencia más el aviso final.
        assert_eq!(out.content.lines().count(), 201);
        assert!(out
            .content
            .ends_with("[200 of 250 matches shown; narrow the pattern]"));
        let first = out.content.lines().next().unwrap();
        assert!(first.contains("a.txt:1: "));
        assert!(first.contains(&"x".repeat(300)));
        assert!(!first.contains(&"x".repeat(301)));
    }

    #[tokio::test]
    async fn grep_cuts_long_lines_on_a_char_boundary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), format!("{}\n", "ñ".repeat(400))).unwrap();
        let out = GrepTool
            .run(
                ctx(dir.path()),
                serde_json::json!({"pattern":"ñ"}),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(out.content.contains(&"ñ".repeat(300)));
        assert!(!out.content.contains(&"ñ".repeat(301)));
        // Sin aviso final: hay una sola coincidencia.
        assert!(!out.content.contains("matches shown"));
    }
}
