use super::{SessionId, SessionInfo, SessionStore, SubagentDescriptor, SubagentStatus};
use crate::error::{CoreError, Result};
use crate::message::{self, Message};
use async_trait::async_trait;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

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
                result TEXT,
                notice_delivered INTEGER NOT NULL DEFAULT 0,
                created_at_ms INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        // shortcut: one process per DB; use leases if concurrent Loom processes share it.
        sqlx::query("UPDATE subagents SET status = 'interrupted' WHERE status = 'running'")
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
            "INSERT INTO subagents (parent_session_id, child_session_id, job_id, label, status, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(descriptor.parent.0)
        .bind(descriptor.child.0)
        .bind(&descriptor.job_id)
        .bind(&descriptor.label)
        .bind(subagent_status(descriptor.status))
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
        status: SubagentStatus,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE subagents SET status = ? WHERE parent_session_id = ? AND child_session_id = ?",
        )
        .bind(subagent_status(status))
        .bind(parent.0)
        .bind(child.0)
        .execute(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        Ok(())
    }

    async fn list_subagents(&self, parent: SessionId) -> Result<Vec<SubagentDescriptor>> {
        let rows = sqlx::query(
            "SELECT parent_session_id, child_session_id, job_id, label, status, result, notice_delivered
             FROM subagents WHERE parent_session_id = ? ORDER BY created_at_ms, child_session_id",
        )
        .bind(parent.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoreError::Store(e.to_string()))?;
        rows.into_iter()
            .map(|row| {
                let status: String = row.get("status");
                Ok(SubagentDescriptor {
                    parent: SessionId(row.get("parent_session_id")),
                    child: SessionId(row.get("child_session_id")),
                    job_id: row.get("job_id"),
                    label: row.get("label"),
                    status: parse_subagent_status(&status)?,
                    result: row.get("result"),
                    notice_delivered: row.get::<i64, _>("notice_delivered") != 0,
                })
            })
            .collect()
    }

    async fn complete_subagent(
        &self,
        parent: SessionId,
        child: SessionId,
        result: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE subagents SET status = 'completed', result = ? WHERE parent_session_id = ? AND child_session_id = ?")
            .bind(result)
            .bind(parent.0)
            .bind(child.0)
            .execute(&self.pool)
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
             WHERE parent_session_id = ? AND child_session_id = ? AND status = 'completed'",
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
}

fn subagent_status(status: SubagentStatus) -> &'static str {
    match status {
        SubagentStatus::Running => "running",
        SubagentStatus::Completed => "completed",
        SubagentStatus::Killed => "killed",
        SubagentStatus::Failed => "failed",
        SubagentStatus::Interrupted => "interrupted",
    }
}

fn parse_subagent_status(status: &str) -> Result<SubagentStatus> {
    match status {
        "running" => Ok(SubagentStatus::Running),
        "completed" => Ok(SubagentStatus::Completed),
        "killed" => Ok(SubagentStatus::Killed),
        "failed" => Ok(SubagentStatus::Failed),
        "interrupted" => Ok(SubagentStatus::Interrupted),
        _ => Err(CoreError::Store(format!(
            "unknown subagent status: {status}"
        ))),
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

    #[tokio::test]
    async fn persists_subagent_catalog_and_marks_running_entries_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("subagents.db");
        let store = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        let parent = store.create_session("/proj").await.unwrap();
        let child = store.create_session("/proj").await.unwrap();
        let descriptor = SubagentDescriptor {
            parent,
            child,
            job_id: "subagent-1".into(),
            label: "review".into(),
            status: SubagentStatus::Running,
            result: None,
            notice_delivered: false,
        };
        store.save_subagent(&descriptor).await.unwrap();
        assert_eq!(
            store.list_subagents(parent).await.unwrap(),
            vec![descriptor]
        );
        store
            .complete_subagent(parent, child, "finished")
            .await
            .unwrap();
        assert_eq!(
            store.list_subagents(parent).await.unwrap()[0].status,
            SubagentStatus::Completed
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

        store
            .update_subagent_status(parent, child, SubagentStatus::Running)
            .await
            .unwrap();
        drop(store);
        let reopened = SqliteStore::open(path.to_str().unwrap()).await.unwrap();
        assert_eq!(
            reopened.list_subagents(parent).await.unwrap()[0].status,
            SubagentStatus::Interrupted
        );
    }
}
