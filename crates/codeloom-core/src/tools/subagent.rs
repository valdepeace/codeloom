use super::{ExecutionMode, Tool, ToolCtx, ToolOutput};
use crate::error::{CoreError, Result};
use crate::session::SessionId;
use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

pub struct SubagentTool;

#[async_trait]
impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn description(&self) -> &str {
        "Start an in-process child agent to work on a task independently. Returns child_id and job_id at once; the child runs in the background and its result is delivered to you automatically when it finishes, so do not poll job_output: tell the user what you delegated and end your turn (job_list/job_kill remain available). Params: prompt, optional label, optional agent: the name of a registered agent (omitted = default), and optional follow_up_of: the child_id of one of your subagents that already finished; the new child starts in the same context with that subagent's result in its prompt (a subagent still running cannot be followed up)."
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "minLength": 1, "maxLength": 32768 },
                "label": { "type": "string", "maxLength": 128 },
                "agent": { "type": "string", "minLength": 1, "maxLength": 64,
                           "description": "Name of a registered agent. Omitted means the default agent." },
                "follow_up_of": { "type": "integer", "minimum": 1,
                                  "description": "child_id of a finished subagent of yours whose result the new child continues from." }
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
        // Omitido o `null` lanza un hijo normal; si viene, debe ser el id entero (≥ 1) de un hijo.
        let follow_up_of = match &input["follow_up_of"] {
            serde_json::Value::Null => None,
            valor => match valor.as_i64().filter(|id| *id >= 1) {
                Some(id) => Some(SessionId(id)),
                None => return Err(CoreError::Tool(
                    "follow_up_of must be the integer child_id (1 or more) of a finished subagent"
                        .into(),
                )),
            },
        };
        let (child, job) = match follow_up_of {
            Some(anterior) => {
                launcher
                    .start_follow_up(ctx.session, label, prompt, agent, anterior, cancel)
                    .await?
            }
            None => {
                launcher
                    .start(ctx.session, label, prompt, agent, cancel)
                    .await?
            }
        };
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
        /// Tareas referidas que llegaron a `start_follow_up`.
        seguimientos: Mutex<Vec<SessionId>>,
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

        async fn start_follow_up(
            &self,
            _parent: SessionId,
            _label: String,
            _prompt: String,
            _agent: Option<String>,
            follow_up_of: SessionId,
            _cancel: CancellationToken,
        ) -> Result<(SessionId, JobId)> {
            self.seguimientos.lock().unwrap().push(follow_up_of);
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

    #[test]
    fn delegate_tool_schema_declares_an_optional_follow_up() {
        let esquema = SubagentTool.schema();
        assert_eq!(esquema["properties"]["follow_up_of"]["type"], "integer");
        assert_eq!(esquema["properties"]["follow_up_of"]["minimum"], 1);
        let requeridos = esquema["required"].as_array().unwrap();
        assert!(!requeridos.contains(&serde_json::json!("follow_up_of")));
        assert!(SubagentTool.description().contains("follow_up_of"));
    }

    #[tokio::test]
    async fn delegate_tool_passes_follow_up_of_to_start_follow_up() {
        let espia = Arc::new(LanzadorEspia::default());
        let entrada = serde_json::json!({"prompt": "sigue", "follow_up_of": 7});

        let _ = SubagentTool
            .run(
                contexto_con(espia.clone()),
                entrada,
                CancellationToken::new(),
            )
            .await;

        assert_eq!(*espia.seguimientos.lock().unwrap(), vec![SessionId(7)]);
        assert!(espia.agentes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn delegate_tool_without_follow_up_of_starts_normally() {
        let espia = Arc::new(LanzadorEspia::default());
        for entrada in [
            serde_json::json!({"prompt": "revisa"}),
            serde_json::json!({"prompt": "revisa", "follow_up_of": null}),
        ] {
            let _ = SubagentTool
                .run(
                    contexto_con(espia.clone()),
                    entrada,
                    CancellationToken::new(),
                )
                .await;
        }

        assert!(espia.seguimientos.lock().unwrap().is_empty());
        assert_eq!(espia.agentes.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn delegate_tool_rejects_an_invalid_follow_up_of() {
        for invalido in [
            serde_json::json!(0),
            serde_json::json!(-3),
            serde_json::json!(1.5),
            serde_json::json!("7"),
            serde_json::json!(true),
        ] {
            let espia = Arc::new(LanzadorEspia::default());
            let entrada = serde_json::json!({"prompt": "sigue", "follow_up_of": invalido});

            let resultado = SubagentTool
                .run(
                    contexto_con(espia.clone()),
                    entrada,
                    CancellationToken::new(),
                )
                .await;

            match resultado {
                Err(CoreError::Tool(texto)) => {
                    assert!(texto.contains("follow_up_of"), "{texto}");
                }
                otro => panic!("se esperaba un error de herramienta con {invalido}: {otro:?}"),
            }
            assert!(espia.seguimientos.lock().unwrap().is_empty());
            assert!(espia.agentes.lock().unwrap().is_empty());
        }
    }
}
