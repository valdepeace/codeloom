use crate::error::Result;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub mod ask_user;
pub mod bash;
pub mod edit;
pub mod glob;
pub mod grep;
pub mod job_kill;
pub mod job_list;
pub mod job_output;
pub mod job_start;
pub mod subagent;
pub mod subagent_control;
pub mod subagent_list;
pub mod todo_write;
pub mod view;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Parallel,
    Exclusive,
}

#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }
    pub fn err(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Tope de lo que una herramienta puede devolver al modelo. Ciertamente, una salida
/// desmedida (un `glob **/*` que recorre `target/`) desborda el contexto del modelo y
/// el proveedor responde con un 400; de ahí que se recorte siempre, con aviso expreso.
pub const MAX_TOOL_OUTPUT_BYTES: usize = 20_000;

/// Tope del texto variable de un aviso (detalle de un job, resultado de un subagente) que
/// entra al historial como mensaje de usuario, por el que el tope de herramientas no pasa.
pub const MAX_NOTICE_BYTES: usize = 8_000;

/// Recorta `content` a `max` bytes por una frontera de carácter y añade un aviso con el
/// tamaño original. Si cabe, lo devuelve intacto.
pub fn limit_bytes(content: &str, max: usize) -> std::borrow::Cow<'_, str> {
    if content.len() <= max {
        return std::borrow::Cow::Borrowed(content);
    }
    let mut end = max;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    std::borrow::Cow::Owned(format!(
        "{}
[output truncated: {} of {} bytes shown; narrow the request]",
        &content[..end],
        end,
        content.len()
    ))
}

/// Recorta la salida de una herramienta a `MAX_TOOL_OUTPUT_BYTES`; ver `limit_bytes`.
pub fn limit_output(content: &str) -> std::borrow::Cow<'_, str> {
    limit_bytes(content, MAX_TOOL_OUTPUT_BYTES)
}

#[derive(Clone)]
pub struct ToolCtx {
    pub cwd: PathBuf,
    pub permission: Arc<crate::permission::PermissionService>,
    pub session: crate::session::SessionId,
    pub jobs: Arc<dyn crate::job::JobRegistry>,
    pub bus: crate::event::EventBus,
    pub subagents: Option<Arc<dyn crate::subagent::SubagentLauncher>>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> serde_json::Value;
    fn execution_mode(&self, _input: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Parallel
    }
    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput>;
}

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }
    pub fn without(&self, name: &str) -> Self {
        let mut registry = self.clone();
        registry.tools.remove(name);
        registry
    }
    pub fn specs(&self) -> Vec<crate::provider::ToolSpec> {
        self.tools
            .values()
            .map(|t| crate::provider::ToolSpec {
                name: t.name().to_string(),
                description: t.description().to_string(),
                input_schema: t.schema(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Dummy;
    #[async_trait]
    impl Tool for Dummy {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "d"
        }
        fn schema(&self) -> serde_json::Value {
            serde_json::json!({"type":"object"})
        }
        async fn run(
            &self,
            _c: ToolCtx,
            _i: serde_json::Value,
            _x: CancellationToken,
        ) -> Result<ToolOutput> {
            Ok(ToolOutput::ok("ok"))
        }
    }

    #[test]
    fn registers_and_lists_specs() {
        let mut r = ToolRegistry::new();
        r.register(std::sync::Arc::new(Dummy));
        assert!(r.get("dummy").is_some());
        assert_eq!(r.specs()[0].name, "dummy");
    }

    #[test]
    fn limit_output_keeps_small_and_truncates_large_on_a_char_boundary() {
        assert_eq!(limit_output("hola"), "hola");
        let big = "ñ".repeat(MAX_TOOL_OUTPUT_BYTES);
        let out = limit_output(&big);
        assert!(out.len() < big.len());
        assert!(out.contains("[output truncated:"));
        assert!(out.contains(&format!("of {} bytes", big.len())));
    }

    #[test]
    fn limit_bytes_respects_the_given_cap() {
        assert_eq!(limit_bytes("corto", 10), "corto");
        // Cada «ñ» ocupa 2 bytes: el corte de 51 bytes cae a mitad de carácter y retrocede a 50.
        let big = "ñ".repeat(100);
        let out = limit_bytes(&big, 51);
        assert!(out.starts_with(&"ñ".repeat(25)));
        assert!(!out.starts_with(&"ñ".repeat(26)));
        assert!(out.contains("[output truncated: 50 of 200 bytes"));
    }
}
