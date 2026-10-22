use super::{SessionId, SessionInfo, SessionStore, SubagentDescriptor, UsageRecord};
use crate::a2a::{self, Artifact};
use crate::error::{CoreError, Result};
use crate::message::{self, Message};
use crate::task::{TaskState, TaskStatus};
use async_trait::async_trait;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

/// Mensaje con el que queda una tarea que seguía en marcha cuando la aplicación se cerró.
const INTERRUPTED_AT_SHUTDOWN: &str = "interrupted by application shutdown";

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn open(path: &str) -> Result<Self> {
        let url = format!("sqlite://{path}?mode=rwc");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                project TEXT NOT NULL,
                created_at_ms INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                payload TEXT NOT NULL,
                seq INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS subagents (
                parent_session_id INTEGER NOT NULL,
                child_session_id INTEGER NOT NULL PRIMARY KEY,
                job_id TEXT NOT NULL,
                label TEXT NOT NULL,
                status TEXT NOT NULL,
                status_message TEXT,
                status_timestamp_ms INTEGER,
                result TEXT,
                notice_delivered INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        // Una base anterior a A2A no tiene las columnas de estado: se añaden solo si faltan,
        // así que abrirla dos veces seguidas no falla.
        let columnas: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('subagents')")
                .fetch_all(&pool)
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
        for (columna, ddl) in [
            (
                "status_message",
                "ALTER TABLE subagents ADD COLUMN status_message TEXT",
            ),
            (
                "status_timestamp_ms",
                "ALTER TABLE subagents ADD COLUMN status_timestamp_ms INTEGER",
            ),
            (
                "artifacts_json",
                "ALTER TABLE subagents ADD COLUMN artifacts_json TEXT",
            ),
        ] {
            if !columnas.iter().any(|existente| existente == columna) {
                sqlx::query(ddl)
                    .execute(&pool)
                    .await
                    .map_err(|e| CoreError::Store(e.to_string()))?;
            }
        }
        // Libro de consumo. Una base anterior no la tiene: `IF NOT EXISTS` la añade sin tocar el resto.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS usage (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                input_price REAL,
                output_price REAL,
                created_at_ms INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        // shortcut: one process per DB; use leases if concurrent Loom processes share it.
        // Lo no terminado al reabrir (y la cadena antigua «running») pasa a fallido, con un
        // mensaje que explica por qué: A2A no define qué le corresponde a un proceso que desapareció.
        sqlx::query(
            "UPDATE subagents SET status = ?, status_message = ?, status_timestamp_ms = ?
             WHERE status IN (?, ?, ?, ?, 'running')",
        )
        .bind(TaskState::Failed.proto_name())
        .bind(encode_status_message(Some(&a2a::Message::agent_text(
            INTERRUPTED_AT_SHUTDOWN,
        )))?)
        .bind(message::now_ms())
        .bind(TaskState::Submitted.proto_name())
        .bind(TaskState::Working.proto_name())
        .bind(TaskState::InputRequired.proto_name())
        .bind(TaskState::AuthRequired.proto_name())
        .execute(&pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(Self { pool })
    }
}

#[async_trait]
impl SessionStore for SqliteStore {
    async fn create_session(&self, project: &str) -> Result<SessionId> {
        let now = crate::message::now_ms();
        let r = sqlx::query("INSERT INTO sessions (project, created_at_ms) VALUES (?, ?)")
            .bind(project)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(SessionId(r.last_insert_rowid()))
    }

    async fn list_sessions(&self, project: &str) -> Result<Vec<SessionInfo>> {
        let rows = sqlx::query(
            "SELECT s.id, COUNT(m.id) AS message_count, MAX(m.id) AS last_message_id
             FROM sessions s
             JOIN messages m ON m.session_id = s.id
             WHERE s.project = ?
             GROUP BY s.id
             ORDER BY last_message_id DESC",
        )
        .bind(project)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let id = SessionId(row.get("id"));
            let payload: Option<String> = sqlx::query_scalar(
                "SELECT payload FROM messages WHERE session_id = ? ORDER BY seq ASC LIMIT 1",
            )
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
            let preview = payload
                .map(|payload| message::codec::from_json(&payload))
                .transpose()?
                .map(|message| message.text().replace('\n', " "))
                .unwrap_or_default();
            sessions.push(SessionInfo {
                id,
                preview,
                message_count: usize::try_from(row.get::<i64, _>("message_count")).unwrap_or(0),
            });
        }
        Ok(sessions)
    }

    async fn append_message(&self, sid: SessionId, msg: &Message) -> Result<()> {
        let next: i64 = sqlx::query(
            "SELECT COALESCE(MAX(seq), -1) + 1 AS n FROM messages WHERE session_id = ?",
        )
        .bind(sid.0)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?
        .get("n");
        sqlx::query("INSERT INTO messages (session_id, payload, seq) VALUES (?, ?, ?)")
            .bind(sid.0)
            .bind(message::codec::to_json(msg)?)
            .bind(next)
            .execute(&self.pool)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn messages(&self, sid: SessionId) -> Result<Vec<Message>> {
        let rows =
            sqlx::query("SELECT payload FROM messages WHERE session_id = ? ORDER BY seq ASC")
                .bind(sid.0)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
        rows.into_iter()
            .map(|r| message::codec::from_json(r.get::<String, _>("payload").as_str()))
            .collect()
    }

    async fn save_subagent(&self, descriptor: &SubagentDescriptor) -> Result<()> {
        sqlx::query(
            "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, status_message, status_timestamp_ms, artifacts_json, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(descriptor.parent.0)
        .bind(descriptor.child.0)
        .bind(&descriptor.job_id)
        .bind(&descriptor.label)
        .bind(descriptor.status.state.proto_name())
        .bind(encode_status_message(descriptor.status.message.as_ref())?)
        .bind(descriptor.status.timestamp_ms)
        .bind(encode_artifacts(&descriptor.artifacts)?)
        .bind(crate::message::now_ms())
        .execute(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn update_subagent_status(
        &self,
        parent: SessionId,
        child: SessionId,
        status: TaskStatus,
    ) -> Result<()> {
        // Leer y escribir en una misma transacción: un terminal no se reabre ni aunque
        // dos escrituras coincidan.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        let actual: Option<String> = sqlx::query_scalar(
            "SELECT status FROM subagents WHERE parent_session_id = ? AND child_session_id = ?",
        )
        .bind(parent.0)
        .bind(child.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        if let Some(actual) = actual {
            let actual = parse_task_state(&actual)?;
            if actual.is_terminal() {
                return Err(CoreError::Store(format!(
                    "subagent already settled as {}",
                    actual.proto_name()
                )));
            }
        }
        sqlx::query(
            "UPDATE subagents SET status = ?, status_message = ?, status_timestamp_ms = ?
             WHERE parent_session_id = ? AND child_session_id = ?",
        )
        .bind(status.state.proto_name())
        .bind(encode_status_message(status.message.as_ref())?)
        .bind(status.timestamp_ms)
        .bind(parent.0)
        .bind(child.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn list_subagents(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
        let rows = sqlx::query(
            "SELECT parent_session_id, child_session_id, job_id, label, status, status_message,
                    status_timestamp_ms, created_at_ms, artifacts_json, result, notice_delivered
             FROM subagents WHERE parent_session_id = ? ORDER BY created_at_ms, child_session_id",
        )
        .bind(parent.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        rows.into_iter()
            .map(|row| {
                let status: String = row.get("status");
                let child: i64 = row.get("child_session_id");
                // Una fila antigua no tiene marca de tiempo propia: se usa la de su creación.
                let timestamp_ms = row
                    .get::<Option<i64>, _>("status_timestamp_ms")
                    .unwrap_or_else(|| row.get("created_at_ms"));
                Ok(SubagentDescriptor {
                    parent: SessionId(row.get("parent_session_id")),
                    child: SessionId(child),
                    job_id: row.get("job_id"),
                    label: row.get("label"),
                    status: TaskStatus {
                        state: parse_task_state(&status)?,
                        message: decode_status_message(row.get("status_message"), child),
                        timestamp_ms,
                    },
                    artifacts: decode_artifacts(
                        row.get("artifacts_json"),
                        row.get("result"),
                        child,
                    )?,
                    notice_delivered: row.get::<i64, _>("notice_delivered") != 0,
                })
            })
            .collect()
    }

    async fn complete_subagent(
        &self,
        parent: SessionId,
        child: SessionId,
        artifacts: &[Artifact],
    ) -> Result<()> {
        // Igual que `update_subagent_status`: un terminal no admite otra transición, tampoco esta.
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        let actual: Option<String> = sqlx::query_scalar(
            "SELECT status FROM subagents WHERE parent_session_id = ? AND child_session_id = ?",
        )
        .bind(parent.0)
        .bind(child.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        if let Some(actual) = actual {
            let actual = parse_task_state(&actual)?;
            if actual.is_terminal() {
                return Err(CoreError::Store(format!(
                    "subagent already settled as {}",
                    actual.proto_name()
                )));
            }
        }
        sqlx::query(
            "UPDATE subagents SET status = ?, status_message = NULL, status_timestamp_ms = ?, artifacts_json = ?
             WHERE parent_session_id = ? AND child_session_id = ?",
        )
        .bind(TaskState::Completed.proto_name())
        .bind(message::now_ms())
        .bind(encode_artifacts(artifacts)?)
        .bind(parent.0)
        .bind(child.0)
        .execute(&mut *tx)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn append_subagent_notice(
        &self,
        parent: SessionId,
        child: SessionId,
        message: &Message,
    ) -> Result<bool> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        let pending = sqlx::query_scalar::<_, i64>(
            "SELECT notice_delivered FROM subagents
             WHERE parent_session_id = ? AND child_session_id = ? AND status IN ('TASK_STATE_COMPLETED', 'completed')",
        )
        .bind(parent.0)
        .bind(child.0)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        if pending != Some(0) {
            tx.rollback()
                .await
                .map_err(|e| CoreError::Store(e.to_string()))?;
            return Ok(false);
        }
        let next: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(seq), -1) + 1 FROM messages WHERE session_id = ?",
        )
        .bind(parent.0)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        sqlx::query("INSERT INTO messages (session_id, payload, seq) VALUES (?, ?, ?)")
            .bind(parent.0)
            .bind(message::codec::to_json(message)?)
            .bind(next)
            .execute(&mut *tx)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        sqlx::query("UPDATE subagents SET notice_delivered = 1 WHERE parent_session_id = ? AND child_session_id = ?")
            .bind(parent.0)
            .bind(child.0)
            .execute(&mut *tx)
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(true)
    }

    async fn record_usage(&self, session: SessionId, record: &UsageRecord) -> Result<()> {
        sqlx::query(
            "INSERT INTO usage (session_id, model, input_tokens, output_tokens, input_price, output_price, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.0)
        .bind(&record.model)
        .bind(to_sql_int(record.input_tokens)?)
        .bind(to_sql_int(record.output_tokens)?)
        .bind(record.input_price)
        .bind(record.output_price)
        .bind(record.created_at_ms)
        .execute(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn session_usage(&self, session: SessionId) -> Result<Vec<UsageRecord>> {
        let rows = sqlx::query(
            "SELECT session_id, model, input_tokens, output_tokens, input_price, output_price, created_at_ms
             FROM usage
             WHERE session_id = ? OR session_id IN (SELECT child_session_id FROM subagents WHERE parent_session_id = ?)
             ORDER BY id",
        )
        .bind(session.0)
        .bind(session.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        rows.into_iter()
            .map(|row| {
                Ok(UsageRecord {
                    session: SessionId(row.get("session_id")),
                    model: row.get("model"),
                    input_tokens: from_sql_int(row.get("input_tokens"))?,
                    output_tokens: from_sql_int(row.get("output_tokens"))?,
                    input_price: row.get("input_price"),
                    output_price: row.get("output_price"),
                    created_at_ms: row.get("created_at_ms"),
                })
            })
            .collect()
    }
}

/// Los tokens se guardan como INTEGER de SQLite (i64); un valor fuera de rango es un error.
fn to_sql_int(value: u64) -> Result<i64> {
    i64::try_from(value).map_err(|_| CoreError::Store(format!("token count out of range: {value}")))
}

fn from_sql_int(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| CoreError::Store(format!("negative token count: {value}")))
}

/// Serializa el mensaje de estado para la columna `status_message`: el JSON de un `Message`.
fn encode_status_message(message: Option<&a2a::Message>) -> Result<Option<String>> {
    message
        .map(|message| serde_json::to_string(message).map_err(|e| CoreError::Store(e.to_string())))
        .transpose()
}

/// Lee la columna `status_message`. Lo escrito por esta versión es el JSON de un `Message`; una
/// fila anterior a A2A guarda texto plano, que se lee como mensaje del agente. El identificador
/// de esa lectura se deriva de la fila, para que dos lecturas den lo mismo.
fn decode_status_message(raw: Option<String>, child: i64) -> Option<a2a::Message> {
    let raw = raw?;
    match serde_json::from_str::<a2a::Message>(&raw) {
        Ok(message) => Some(message),
        Err(_) => {
            let mut message = a2a::Message::agent_text(raw);
            message.message_id = format!("estado-{child}");
            Some(message)
        }
    }
}

/// Serializa los artefactos del resultado para la columna `artifacts_json`.
fn encode_artifacts(artifacts: &[Artifact]) -> Result<String> {
    serde_json::to_string(artifacts).map_err(|e| CoreError::Store(e.to_string()))
}

/// Lee los artefactos de una fila. Si `artifacts_json` es nulo y `result` no, la fila es
/// anterior a A2A y su resultado de texto es un único artefacto de texto.
fn decode_artifacts(
    json: Option<String>,
    legacy_result: Option<String>,
    child: i64,
) -> Result<Vec<Artifact>> {
    match json {
        Some(json) => {
            serde_json::from_str(&json).map_err(|e| CoreError::Store(format!("artifacts: {e}")))
        }
        None => Ok(legacy_result
            .map(|texto| {
                let mut artefacto = Artifact::text_artifact(texto);
                artefacto.artifact_id = format!("resultado-{child}");
                vec![artefacto]
            })
            .unwrap_or_default()),
    }
}

/// Estado de una fila del almacén. Acepta los nombres del proto v1.0 y las cadenas de las
/// versiones anteriores, que se leen como su equivalente de A2A: `running` es `working`;
/// `killed`, `canceled`; `interrupted` y `unfinished`, `failed`.
fn parse_task_state(status: &str) -> Result<TaskState> {
    match status {
        "running" => Ok(TaskState::Working),
        "completed" => Ok(TaskState::Completed),
        "failed" | "interrupted" | "unfinished" => Ok(TaskState::Failed),
        "killed" => Ok(TaskState::Canceled),
        _ => TaskState::from_proto_name(status)
            .ok_or_else(|| CoreError::Store(format!("unknown subagent status: {status}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentPart, Message};

    #[tokio::test]
    async fn append_and_read_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("codeloom.db");
        let store = SqliteStore::open(db.to_str().unwrap()).await.unwrap();
        let sid = store.create_session("/proj").await.unwrap();
        store
            .append_message(sid, &Message::user("one"))
            .await
            .unwrap();
        store
            .append_message(
                sid,
                &Message::assistant(vec![ContentPart::Text { text: "two".into() }]),
            )
            .await
            .unwrap();
        let msgs = store.messages(sid).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].text(), "one");
        assert_eq!(msgs[1].text(), "two");
    }

    #[tokio::test]
    async fn lists_only_nonempty_sessions_for_project_in_recent_activity_order() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("list.db").to_str().unwrap())
            .await
            .unwrap();
        let old = store.create_session("/proj").await.unwrap();
        let other_project = store.create_session("/other").await.unwrap();
        let recent = store.create_session("/proj").await.unwrap();
        store
            .append_message(old, &Message::user("older task"))
            .await
            .unwrap();
        store
            .append_message(other_project, &Message::user("other task"))
            .await
            .unwrap();
        store
            .append_message(recent, &Message::user("recent task"))
            .await
            .unwrap();

        let sessions = store.list_sessions("/proj").await.unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].id, recent);
        assert_eq!(sessions[0].preview, "recent task");
        assert_eq!(sessions[0].message_count, 1);
        assert_eq!(sessions[1].id, old);
    }

    use crate::task::{TaskState, TaskStatus};

    /// Descriptor de prueba del subagente `child` con el estado A2A dado.
    fn descriptor(parent: SessionId, child: SessionId, state: TaskState) -> SubagentDescriptor {
        SubagentDescriptor {
            parent,
            child,
            job_id: format!("subagent-{}", child.0),
            label: "review".into(),
            status: TaskStatus::new(state),
            artifacts: Vec::new(),
            notice_delivered: false,
        }
    }

    /// Estado guardado tal cual en la columna `status` de la fila del hijo.
    async fn raw_status(store: &SqliteStore, child: SessionId) -> String {
        sqlx::query_scalar("SELECT status FROM subagents WHERE child_session_id = ?")
            .bind(child.0)
            .fetch_one(&store.pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn persists_subagent_catalog_and_completes_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("subagents.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        let inicial = descriptor(parent, child, TaskState::Working);
        store.save_subagent(&inicial).await.unwrap();
        assert_eq!(store.list_subagents(parent).await.unwrap(), vec![inicial]);
        store
            .complete_subagent(
                parent,
                child,
                &[crate::a2a::Artifact::text_artifact("finished")],
            )
            .await
            .unwrap();
        assert_eq!(
            store.list_subagents(parent).await.unwrap()[0].status.state,
            TaskState::Completed
        );
        let notice = Message::user("[Subagent review completed]\nfinished");
        assert!(store
            .append_subagent_notice(parent, child, &notice)
            .await
            .unwrap());
        assert!(!store
            .append_subagent_notice(parent, child, &notice)
            .await
            .unwrap());
        assert_eq!(store.messages(parent).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn reopened_store_marks_non_terminal_children_failed_with_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reopen.db");
        let store = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let mut vivos = Vec::new();
        for state in [
            TaskState::Submitted,
            TaskState::Working,
            TaskState::InputRequired,
            TaskState::AuthRequired,
        ] {
            let child = store.create_session("/proj").await.unwrap();
            store
                .save_subagent(&descriptor(parent, child, state))
                .await
                .unwrap();
            vivos.push(child);
        }
        let terminado = store.create_session("/proj").await.unwrap();
        store
            .save_subagent(&descriptor(parent, terminado, TaskState::Completed))
            .await
            .unwrap();
        // Una fila escrita con la cadena antigua «running» también cuenta como no terminal.
        let antigua = store.create_session("/proj").await.unwrap();
        sqlx::query(
            "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(parent.0)
        .bind(antigua.0)
        .bind("subagent-antigua")
        .bind("review")
        .bind("running")
        .bind(crate::message::now_ms())
        .execute(&store.pool)
        .await
        .unwrap();
        vivos.push(antigua);
        drop(store);

        let reopened = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        let listed = reopened.list_subagents(parent).await.unwrap();
        for child in vivos {
            let fila = listed.iter().find(|d| d.child == child).unwrap();
            assert_eq!(fila.status.state, TaskState::Failed, "hijo {child:?}");
            assert_eq!(
                fila.status.message_text().as_deref(),
                Some("interrupted by application shutdown")
            );
        }
        let fila = listed.iter().find(|d| d.child == terminado).unwrap();
        assert_eq!(fila.status.state, TaskState::Completed);
        assert_eq!(fila.status.message, None);
    }

    #[tokio::test]
    async fn state_roundtrips_through_sqlite_with_message_and_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("roundtrip.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        for state in TaskState::ALL {
            let child = store.create_session("/proj").await.unwrap();
            let guardado = SubagentDescriptor {
                status: TaskStatus {
                    state,
                    message: Some(crate::a2a::Message::agent_text(format!(
                        "mensaje de {state:?}"
                    ))),
                    timestamp_ms: 1_700_000_000_123,
                },
                ..descriptor(parent, child, state)
            };
            store.save_subagent(&guardado).await.unwrap();
            // Lo que se escribe es el nombre del proto v1.0, no una cadena propia.
            assert_eq!(raw_status(&store, child).await, state.proto_name());
            let leido = store
                .list_subagents(parent)
                .await
                .unwrap()
                .into_iter()
                .find(|d| d.child == child)
                .unwrap();
            assert_eq!(leido, guardado);
        }
    }

    #[tokio::test]
    async fn legacy_strings_are_read_as_equivalent_states() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("legacy.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let pares = [
            ("running", TaskState::Working),
            ("completed", TaskState::Completed),
            ("failed", TaskState::Failed),
            ("killed", TaskState::Canceled),
            ("interrupted", TaskState::Failed),
            ("unfinished", TaskState::Failed),
        ];
        let mut hijos = Vec::new();
        for (cadena, esperado) in pares {
            let child = store.create_session("/proj").await.unwrap();
            // Fila escrita por una versión anterior, con la cadena antigua tal cual.
            sqlx::query(
                "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, created_at_ms)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(parent.0)
            .bind(child.0)
            .bind(format!("subagent-{cadena}"))
            .bind("review")
            .bind(cadena)
            .bind(crate::message::now_ms())
            .execute(&store.pool)
            .await
            .unwrap();
            hijos.push((child, esperado));
        }
        let listed = store.list_subagents(parent).await.unwrap();
        assert_eq!(listed.len(), hijos.len());
        for (child, esperado) in hijos {
            let fila = listed.iter().find(|d| d.child == child).unwrap();
            assert_eq!(fila.status.state, esperado, "hijo {child:?}");
        }
    }

    #[tokio::test]
    async fn store_migration_adds_status_columns_to_an_old_database_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("viejo-a2a.db");
        {
            let pool =
                sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.to_str().unwrap()))
                    .await
                    .unwrap();
            // Esquema anterior a A2A: sin `status_message` ni `status_timestamp_ms`.
            sqlx::query(
                "CREATE TABLE subagents (
                    parent_session_id INTEGER NOT NULL,
                    child_session_id INTEGER NOT NULL PRIMARY KEY,
                    job_id TEXT NOT NULL,
                    label TEXT NOT NULL,
                    status TEXT NOT NULL,
                    result TEXT,
                    notice_delivered INTEGER NOT NULL DEFAULT 0,
                    created_at_ms INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, created_at_ms)
                 VALUES (1, 2, 'subagent-old', 'review', 'completed', 1)",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }

        let store = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        let primera = store.list_subagents(SessionId(1)).await.unwrap();
        assert_eq!(primera.len(), 1);
        assert_eq!(primera[0].status.state, TaskState::Completed);
        assert_eq!(primera[0].status.message, None);
        drop(store);

        // Segunda apertura: las columnas ya existen y no vuelve a fallar ni a cambiar nada.
        let again = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        assert_eq!(again.list_subagents(SessionId(1)).await.unwrap(), primera);
        let columnas: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('subagents')")
                .fetch_all(&again.pool)
                .await
                .unwrap();
        assert!(columnas.iter().any(|c| c == "status_message"));
        assert!(columnas.iter().any(|c| c == "status_timestamp_ms"));
    }

    #[tokio::test]
    async fn update_from_terminal_state_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("terminal.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        store
            .save_subagent(&descriptor(parent, child, TaskState::Working))
            .await
            .unwrap();
        store
            .update_subagent_status(parent, child, TaskStatus::new(TaskState::Completed))
            .await
            .unwrap();
        for intento in [TaskState::Working, TaskState::Failed, TaskState::Canceled] {
            assert!(
                store
                    .update_subagent_status(parent, child, TaskStatus::new(intento))
                    .await
                    .is_err(),
                "de Completed a {intento:?} debería fallar"
            );
        }
        assert!(store
            .complete_subagent(
                parent,
                child,
                &[crate::a2a::Artifact::text_artifact("otra vez")]
            )
            .await
            .is_err());
        assert_eq!(
            store.list_subagents(parent).await.unwrap()[0].status.state,
            TaskState::Completed
        );
    }

    #[tokio::test]
    async fn complete_subagent_writes_the_proto_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("complete.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        store
            .save_subagent(&descriptor(parent, child, TaskState::Working))
            .await
            .unwrap();
        store
            .complete_subagent(
                parent,
                child,
                &[crate::a2a::Artifact::text_artifact("finished")],
            )
            .await
            .unwrap();
        assert_eq!(raw_status(&store, child).await, "TASK_STATE_COMPLETED");
    }

    /// Apunte de prueba con precios opcionales `(entrada, salida)` por millón. La sesión es
    /// provisional: el almacén devuelve la sesión de la fila con la que se guardó.
    fn usage(model: &str, input: u64, output: u64, prices: Option<(f64, f64)>) -> UsageRecord {
        UsageRecord {
            session: SessionId(0),
            model: model.into(),
            input_tokens: input,
            output_tokens: output,
            input_price: prices.map(|(input, _)| input),
            output_price: prices.map(|(_, output)| output),
            created_at_ms: crate::message::now_ms(),
        }
    }

    #[tokio::test]
    async fn usage_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("u.db").to_str().unwrap())
            .await
            .unwrap();
        let sid = store.create_session("/p").await.unwrap();
        let priced = UsageRecord {
            session: sid,
            model: "mistral-nemo".into(),
            input_tokens: 1200,
            output_tokens: 300,
            input_price: Some(0.02),
            output_price: Some(0.04),
            created_at_ms: 1_000,
        };
        let unpriced = UsageRecord {
            session: sid,
            model: "sin-precio".into(),
            input_tokens: 5,
            output_tokens: 7,
            input_price: None,
            output_price: None,
            created_at_ms: 2_000,
        };
        store.record_usage(sid, &priced).await.unwrap();
        store.record_usage(sid, &unpriced).await.unwrap();
        assert_eq!(
            store.session_usage(sid).await.unwrap(),
            vec![priced, unpriced]
        );
    }

    #[tokio::test]
    async fn usage_record_carries_its_session_after_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("sesion.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/p").await.unwrap();
        let child = store.create_session("/p").await.unwrap();
        store
            .save_subagent(&SubagentDescriptor {
                parent,
                child,
                job_id: "subagent-1".into(),
                label: "review".into(),
                status: TaskStatus::new(TaskState::Completed),
                artifacts: Vec::new(),
                notice_delivered: false,
            })
            .await
            .unwrap();
        store
            .record_usage(parent, &usage("padre", 1, 1, None))
            .await
            .unwrap();
        store
            .record_usage(child, &usage("hijo", 2, 2, None))
            .await
            .unwrap();

        let rows = store.session_usage(parent).await.unwrap();
        let apuntes: Vec<(SessionId, &str)> = rows
            .iter()
            .map(|record| (record.session, record.model.as_str()))
            .collect();
        assert_eq!(
            apuntes,
            vec![(parent, "padre"), (child, "hijo")],
            "cada apunte vuelve con la sesión que lo originó"
        );
    }

    #[tokio::test]
    async fn usage_is_isolated_per_session() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("iso.db").to_str().unwrap())
            .await
            .unwrap();
        let first = store.create_session("/p").await.unwrap();
        let second = store.create_session("/p").await.unwrap();
        store
            .record_usage(first, &usage("a", 10, 1, Some((1.0, 2.0))))
            .await
            .unwrap();
        store
            .record_usage(second, &usage("b", 20, 2, None))
            .await
            .unwrap();

        let of_first = store.session_usage(first).await.unwrap();
        assert_eq!(of_first.len(), 1);
        assert_eq!(of_first[0].model, "a");
        let of_second = store.session_usage(second).await.unwrap();
        assert_eq!(of_second.len(), 1);
        assert_eq!(of_second[0].model, "b");
    }

    #[tokio::test]
    async fn session_usage_includes_children() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("hijos.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/p").await.unwrap();
        let child = store.create_session("/p").await.unwrap();
        let stranger = store.create_session("/p").await.unwrap();
        store
            .save_subagent(&SubagentDescriptor {
                parent,
                child,
                job_id: "subagent-1".into(),
                label: "review".into(),
                status: TaskStatus::new(TaskState::Completed),
                artifacts: Vec::new(),
                notice_delivered: false,
            })
            .await
            .unwrap();

        store
            .record_usage(parent, &usage("p1", 1, 1, None))
            .await
            .unwrap();
        store
            .record_usage(child, &usage("c1", 2, 2, None))
            .await
            .unwrap();
        store
            .record_usage(parent, &usage("p2", 3, 3, None))
            .await
            .unwrap();
        store
            .record_usage(stranger, &usage("otra", 9, 9, None))
            .await
            .unwrap();

        let models: Vec<String> = store
            .session_usage(parent)
            .await
            .unwrap()
            .into_iter()
            .map(|record| record.model)
            .collect();
        assert_eq!(
            models,
            vec!["p1", "c1", "p2"],
            "por id, sin la sesión ajena"
        );
    }

    #[tokio::test]
    async fn store_migrates_an_existing_database_without_usage_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("viejo.db");
        {
            // Base anterior al libro de consumo: solo la tabla de sesiones, con una fila.
            let pool =
                sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.to_str().unwrap()))
                    .await
                    .unwrap();
            sqlx::query(
                "CREATE TABLE sessions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    project TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO sessions (project, created_at_ms) VALUES ('/p', 1)")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        let store = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        let old = SessionId(1);
        assert!(store.session_usage(old).await.unwrap().is_empty());
        store
            .record_usage(old, &usage("nuevo", 4, 4, Some((1.0, 1.0))))
            .await
            .unwrap();
        assert_eq!(store.session_usage(old).await.unwrap().len(), 1);
    }

    /// Inserta una fila con el esquema de columnas que escribía la versión anterior de `status`.
    async fn fila_antigua(
        store: &SqliteStore,
        parent: SessionId,
        child: SessionId,
        status: &str,
        status_message: Option<&str>,
        result: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, status_message, status_timestamp_ms, result, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(parent.0)
        .bind(child.0)
        .bind(format!("subagent-{}", child.0))
        .bind("review")
        .bind(status)
        .bind(status_message)
        .bind(crate::message::now_ms())
        .bind(result)
        .bind(crate::message::now_ms())
        .execute(&store.pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn status_message_roundtrips_through_sqlite_as_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("mensaje.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        let mensaje = crate::a2a::Message::agent_text("¿Cuál?");
        let guardado = SubagentDescriptor {
            status: TaskStatus {
                state: TaskState::InputRequired,
                message: Some(mensaje.clone()),
                timestamp_ms: 1_700_000_000_123,
            },
            ..descriptor(parent, child, TaskState::InputRequired)
        };
        store.save_subagent(&guardado).await.unwrap();
        let leido = store.list_subagents(parent).await.unwrap();
        assert_eq!(leido[0].status.message, Some(mensaje));
        assert_eq!(leido, vec![guardado]);
    }

    #[tokio::test]
    async fn legacy_plain_text_status_message_reads_as_agent_message() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("legacy-mensaje.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        fila_antigua(
            &store,
            parent,
            child,
            "TASK_STATE_INPUT_REQUIRED",
            Some("¿Cuál?"),
            None,
        )
        .await;
        let leido = store.list_subagents(parent).await.unwrap();
        let mensaje = leido[0]
            .status
            .message
            .clone()
            .expect("el texto plano se lee como mensaje");
        assert_eq!(mensaje.role, crate::a2a::Role::Agent);
        assert_eq!(mensaje.text(), "¿Cuál?");
        // Dos lecturas dan el mismo mensaje: su identificador se deriva de la fila.
        assert_eq!(store.list_subagents(parent).await.unwrap(), leido);
    }

    #[tokio::test]
    async fn artifacts_roundtrip_through_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("artefactos.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        let artefactos = vec![
            crate::a2a::Artifact::text_artifact("resumen"),
            crate::a2a::Artifact {
                artifact_id: "datos".into(),
                name: Some("datos".into()),
                description: Some("binario y json".into()),
                parts: vec![
                    crate::a2a::Part {
                        content: crate::a2a::PartContent::Raw(vec![0, 255]),
                        metadata: None,
                        filename: Some("d.bin".into()),
                        media_type: Some("application/octet-stream".into()),
                    },
                    crate::a2a::Part {
                        content: crate::a2a::PartContent::Data(serde_json::json!({"ok": true})),
                        metadata: None,
                        filename: None,
                        media_type: None,
                    },
                ],
                metadata: None,
                extensions: vec![],
            },
        ];
        let guardado = SubagentDescriptor {
            artifacts: artefactos,
            ..descriptor(parent, child, TaskState::Completed)
        };
        store.save_subagent(&guardado).await.unwrap();
        assert_eq!(store.list_subagents(parent).await.unwrap(), vec![guardado]);
    }

    #[tokio::test]
    async fn legacy_result_column_reads_as_one_text_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("legacy-result.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        fila_antigua(
            &store,
            parent,
            child,
            "TASK_STATE_COMPLETED",
            None,
            Some("hecho"),
        )
        .await;
        let leido = store.list_subagents(parent).await.unwrap();
        assert_eq!(leido[0].artifacts.len(), 1);
        assert_eq!(leido[0].artifacts[0].text(), "hecho");
        assert_eq!(leido[0].result_text().as_deref(), Some("hecho"));
    }

    #[tokio::test]
    async fn column_migration_for_artifacts_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("columna-artefactos.db");
        {
            // Base anterior a los artefactos: `result` existe y no hay `artifacts_json`.
            let pool =
                sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", path.to_str().unwrap()))
                    .await
                    .unwrap();
            sqlx::query(
                "CREATE TABLE subagents (
                    parent_session_id INTEGER NOT NULL,
                    child_session_id INTEGER NOT NULL PRIMARY KEY,
                    job_id TEXT NOT NULL,
                    label TEXT NOT NULL,
                    status TEXT NOT NULL,
                    result TEXT,
                    notice_delivered INTEGER NOT NULL DEFAULT 0,
                    created_at_ms INTEGER NOT NULL)",
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, result, created_at_ms)
                 VALUES (1, 2, 'subagent-old', 'review', 'completed', 'hecho', 1)",
            )
            .execute(&pool)
            .await
            .unwrap();
            pool.close().await;
        }

        let primera_apertura = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        let primera = primera_apertura.list_subagents(SessionId(1)).await.unwrap();
        assert_eq!(primera[0].result_text().as_deref(), Some("hecho"));
        drop(primera_apertura);

        let again = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        assert_eq!(again.list_subagents(SessionId(1)).await.unwrap(), primera);
        let columnas: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('subagents')")
                .fetch_all(&again.pool)
                .await
                .unwrap();
        assert!(columnas.iter().any(|c| c == "artifacts_json"));
    }

    #[tokio::test]
    async fn complete_subagent_stores_a_text_artifact() {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(dir.path().join("completo.db").to_str().unwrap())
            .await
            .unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        store
            .save_subagent(&descriptor(parent, child, TaskState::Working))
            .await
            .unwrap();
        store
            .complete_subagent(
                parent,
                child,
                &[crate::a2a::Artifact::text_artifact("finished")],
            )
            .await
            .unwrap();
        let leido = store.list_subagents(parent).await.unwrap();
        assert_eq!(leido[0].artifacts.len(), 1);
        assert_eq!(leido[0].artifacts[0].text(), "finished");
        assert_eq!(leido[0].result_text().as_deref(), Some("finished"));
        // `complete_subagent` ya no escribe la columna antigua `result`.
        let result: Option<String> =
            sqlx::query_scalar("SELECT result FROM subagents WHERE child_session_id = ?")
                .bind(child.0)
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert_eq!(result, None);
    }
}
