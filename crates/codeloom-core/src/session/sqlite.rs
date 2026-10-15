use super::{SessionId, SessionStore};
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
            .max_connections(4)
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
}
