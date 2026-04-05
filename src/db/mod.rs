pub mod schema;

use crate::runtime::session::Session;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct Database {
    conn: Connection,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub created_at: String,
    pub updated_at: String,
    pub turn_count: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub summary: Option<String>,
    pub model: Option<String>,
}

impl Database {
    pub fn new(data_dir: &PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("dreamswarm.db");
        let conn = Connection::open(&db_path)?;

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    pub fn migrate(&self) -> anyhow::Result<()> {
        self.conn.execute_batch(schema::INITIAL_SCHEMA)?;
        Ok(())
    }

    pub fn save_session(&self, session: &Session) -> anyhow::Result<()> {
        let messages_json = serde_json::to_string(&session.messages)?;
        self.conn.execute(
            "INSERT INTO sessions (id, messages, created_at, updated_at, turn_count, total_tokens, total_cost_usd, summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET
             messages = ?2,
             updated_at = ?4,
             turn_count = ?5,
             total_tokens = ?6,
             total_cost_usd = ?7,
             summary = ?8",
            params![
                session.id,
                messages_json,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                session.turn_count as i64,
                session.total_tokens as i64,
                session.total_cost_usd,
                session.summary,
            ],
        )?;
        Ok(())
    }

    pub fn load_session(&self, session_id: &str) -> anyhow::Result<Session> {
        let mut stmt = self.conn.prepare(
            "SELECT id, messages, created_at, updated_at, turn_count, total_tokens, total_cost_usd, summary
             FROM sessions WHERE id LIKE ?1 || '%' LIMIT 1"
        )?;

        let session = stmt.query_row(params![session_id], |row| {
            let messages_json: String = row.get(1)?;
            let messages = serde_json::from_str(&messages_json).unwrap_or_default();
            Ok(Session {
                id: row.get(0)?,
                messages,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                    .unwrap_or_default()
                    .with_timezone(&chrono::Utc),
                turn_count: row.get::<_, i64>(4)? as u32,
                total_tokens: row.get::<_, i64>(5)? as u64,
                total_cost_usd: row.get(6)?,
                summary: row.get(7)?,
            })
        })?;

        Ok(session)
    }
}
