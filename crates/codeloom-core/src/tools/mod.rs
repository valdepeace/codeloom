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

/// Recorta `content` conservando `max / 2` bytes del principio y `max / 2` del final, y
/// sustituye lo omitido por un marcador con el número de bytes suprimidos. Ambos cortes
/// caen en frontera de carácter: el de cabeza retrocede y el de cola avanza, de modo que
/// nunca se conserva más de `max` bytes. Si `content` cabe en `max`, lo devuelve intacto.
pub fn limit_bytes(content: &str, max: usize) -> std::borrow::Cow<'_, str> {
    if content.len() <= max {
        return std::borrow::Cow::Borrowed(content);
    }
    let half = max / 2;
    let mut head_end = half;
    while !content.is_char_boundary(head_end) {
        head_end -= 1;
    }
    let mut tail_start = content.len() - half;
    while !content.is_char_boundary(tail_start) {
        tail_start += 1;
    }
    let kept = head_end + (content.len() - tail_start);
    let omitted = content.len() - kept;
    std::borrow::Cow::Owned(format!(
        "{}\n[... {omitted} bytes omitted ...]\n{}",
        &content[..head_end],
        &content[tail_start..]
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
        // ADAPTADO: el marcador es ahora «[... N bytes omitted ...]» (cabeza y cola), no el antiguo
        // «[output truncated: ...]». La cota de tamaño se mantiene: el marcador es corto.
        assert!(out.len() < MAX_TOOL_OUTPUT_BYTES + 100);
        assert!(out.contains("\n[... 20000 bytes omitted ...]\n"));
    }

    #[test]
    fn limit_bytes_respects_the_given_cap() {
        assert_eq!(limit_bytes("corto", 10), "corto");
        // Cada «ñ» ocupa 2 bytes. ADAPTADO: con cabeza y cola, cada extremo recibe 25 bytes, que
        // los cortes ajustan a 24 (12 «ñ») y se omiten 152 de 200. El tope sigue verificándose:
        // lo conservado (sin el marcador) no supera los 51 bytes pedidos.
        let big = "ñ".repeat(100);
        let out = limit_bytes(&big, 51);
        let marcador = "\n[... 152 bytes omitted ...]\n";
        assert!(out.starts_with(&"ñ".repeat(12)));
        assert!(!out.starts_with(&"ñ".repeat(13)));
        assert!(out.contains(marcador));
        assert!(out.len() - marcador.len() <= 51);
    }

    #[test]
    fn limit_bytes_keeps_head_and_tail() {
        let original = format!("{}{}{}", "a".repeat(100), "m".repeat(800), "z".repeat(100));
        let out = limit_bytes(&original, 200);
        // Se conservan 100 bytes de cada extremo; omitidos = 1 000 − 200 = 800.
        assert!(out.starts_with(&"a".repeat(100)));
        assert!(out.ends_with(&"z".repeat(100)));
        assert!(out.contains("\n[... 800 bytes omitted ...]\n"));
        assert!(out.len() < original.len());
    }

    #[test]
    fn limit_bytes_cuts_on_char_boundaries_at_both_ends() {
        // Cada «ñ» ocupa 2 bytes. Con max 51 el corte de cabeza (25) cae a mitad de carácter y
        // retrocede a 24; el de cola (175) también y avanza a 176. Se conservan 48 bytes.
        let original = "ñ".repeat(100);
        let out = limit_bytes(&original, 51);
        assert!(out.starts_with(&"ñ".repeat(12)));
        assert!(!out.starts_with(&"ñ".repeat(13)));
        assert!(out.ends_with(&"ñ".repeat(12)));
        assert!(out.contains("\n[... 152 bytes omitted ...]\n"));
    }
}
