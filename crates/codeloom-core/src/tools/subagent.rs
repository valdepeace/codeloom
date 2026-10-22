use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct SubagentTool;

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn description(&self) -> &str {
        "Start an in-process child agent to work on a task independently. Returns child_id and job_id at once; the child runs in the background and its result is delivered to you automatically when it finishes, so do not poll job_output: tell the user what you delegated and end your turn (job_list/job_kill remain available). Params: prompt, optional label, and optional agent: the name of a registered agent (omitted = default)."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "minLength": 1, "maxLength": 32768 },
                "label": { "type": "string", "maxLength": 128 },
                "agent": { "type": "string", "minLength": 1, "maxLength": 64,
                           "description": "Name of a registered agent. Omitted means the default agent." }
            },
            "required": ["prompt"]
        })
    }

    fn execution_mode(&self, _: &serde_json::Value) -> ExecutionMode {
        ExecutionMode::Exclusive
    }

    async fn run(
        &self,
        ctx: ToolCtx,
        input: serde_json::Value,
        cancel: CancellationToken,
    ) -> Result<ToolOutput> {
        let prompt = input["prompt"]
            .as_str()
            .filter(|s| !s.trim().is_empty() && s.len() <= 32_768)
            .ok_or_else(|| CoreError::Tool("prompt must be 1 to 32768 bytes".into()))?
            .to_string();
        let label = input["label"]
            .as_str()
            .filter(|s| s.len() <= 128 && !s.trim().is_empty())
            .unwrap_or("subagent")
            .to_string();
        let launcher = ctx
            .subagents
            .as_ref()
            .ok_or_else(|| CoreError::Tool("subagents are unavailable in this context".into()))?;
        // La herramienta todavía no elige agente: `None` es el agente `default` (tarea 6b lo cablea).
        // Omitido o `null` es el agente `default`; cualquier otro valor debe ser un nombre.
        let agent = match &input["agent"] {
            serde_json::Value::Null => None,
            serde_json::Value::String(nombre) if !nombre.trim().is_empty() => Some(nombre.clone()),
            _ => {
                return Err(CoreError::Tool(
                    "agent must be the name of a registered agent".into(),
                ))
            }
        };
        let (child, job) = launcher
            .start(ctx.session, label, prompt, agent, cancel)
            .await?;
        if let Some(descriptor) = launcher
            .list(ctx.session)
            .await?
            .into_iter()
            .find(|descriptor| descriptor.child == child)
        {
            ctx.bus
                .publish(crate::event::LiveEvent::SubagentChanged(descriptor));
        }
        super::job_start::track_job(&ctx, &job)?;
        Ok(ToolOutput::ok(
            serde_json::json!({
                "child_id": child.0,
                "job_id": job.0,
            })
            .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job::JobId;
    use crate::message::Message;
    use crate::permission::PermissionService;
    use crate::session::{SessionId, SubagentDescriptor};
    use crate::subagent::SubagentLauncher;
    use std::sync::{Arc, Mutex};

    /// Lanzador de prueba: anota el agente que recibe `start` y luego falla, para no seguir
    /// hasta el seguimiento del trabajo, que aquí no se prueba.
    #[derive(Default)]
    struct LanzadorEspia {
        agentes: Mutex<Vec<Option<String>>>,
    }

    #[async_trait]
    impl SubagentLauncher for LanzadorEspia {
        async fn start(
            &self,
            _parent: SessionId,
            _label: String,
            _prompt: String,
            agent: Option<String>,
            _cancel: CancellationToken,
        ) -> Result<(SessionId, JobId)> {
            self.agentes.lock().unwrap().push(agent);
            Err(CoreError::Tool("fin de la prueba".into()))
        }

        async fn control(
            &self,
            _parent: SessionId,
            _child: SessionId,
            _action: &str,
            _message: Option<String>,
        ) -> Result<()> {
            Err(CoreError::Tool("not used".into()))
        }

        async fn list(&self, _parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
            Ok(Vec::new())
        }

        async fn deliver_notices(&self, _parent: SessionId) -> Result<Vec<Message>> {
            Ok(Vec::new())
        }
    }

    fn contexto_con(lanzador: Arc<LanzadorEspia>) -> ToolCtx {
        ToolCtx {
            cwd: std::env::temp_dir(),
            permission: Arc::new(PermissionService::allow_all()),
            session: SessionId(1),
            jobs: Arc::new(crate::job::LocalJobRegistry::default()),
            bus: crate::event::EventBus::new(8),
            subagents: Some(lanzador),
        }
    }

    #[test]
    fn delegate_tool_schema_declares_an_optional_agent() {
        let esquema = SubagentTool.schema();
        assert_eq!(esquema["properties"]["agent"]["type"], "string");
        let requeridos = esquema["required"].as_array().unwrap();
        assert!(!requeridos.contains(&serde_json::json!("agent")));
        assert!(SubagentTool.description().contains("registered agent"));
    }

    #[tokio::test]
    async fn delegate_tool_passes_the_agent_argument_to_start() {
        let espia = Arc::new(LanzadorEspia::default());
        let entrada = serde_json::json!({"prompt": "revisa", "agent": "revisor"});

        let _ = SubagentTool
            .run(
                contexto_con(espia.clone()),
                entrada,
                CancellationToken::new(),
            )
            .await;

        assert_eq!(
            *espia.agentes.lock().unwrap(),
            vec![Some("revisor".to_string())]
        );
    }

    #[tokio::test]
    async fn delegate_tool_without_agent_passes_none() {
        let espia = Arc::new(LanzadorEspia::default());
        let entrada = serde_json::json!({"prompt": "revisa"});

        let _ = SubagentTool
            .run(
                contexto_con(espia.clone()),
                entrada,
                CancellationToken::new(),
            )
            .await;

        assert_eq!(*espia.agentes.lock().unwrap(), vec![None]);
    }
}
