pub mod schema;

use crate::api::telemetry::TelemetryHub;

use crate::runtime::session::Session;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct Database {
    pool: Pool<SqliteConnectionManager>,
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
        let manager = SqliteConnectionManager::file(db_path);
        let pool = Pool::new(manager)?;

        {
            let conn = pool.get()?;
            // Enable WAL mode for better concurrent access
            conn.execute_batch("PRAGMA journal_mode=WAL;")?;
            conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        }

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &Pool<SqliteConnectionManager> {
        &self.pool
    }

    pub fn migrate(&self) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        conn.execute_batch(schema::INITIAL_SCHEMA)?;
        Ok(())
    }

    pub fn save_session(&self, session: &Session) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        let messages_json = serde_json::to_string(&session.messages)?;
        conn.execute(
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
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
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

    pub fn log_telemetry_event(
        &self,
        category: &str,
        event_type: &str,
        payload: &serde_json::Value,
    ) -> anyhow::Result<()> {
        let conn = self.pool.get()?;
        let payload_json = serde_json::to_string(payload)?;
        conn.execute(
            "INSERT INTO telemetry_events (category, event_type, payload, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                category,
                event_type,
                payload_json,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(())
    }

    pub fn get_telemetry_history(
        &self,
        category: Option<&str>,
        limit: usize,
    ) -> anyhow::Result<Vec<serde_json::Value>> {
        let conn = self.pool.get()?;
        let mut query =
            "SELECT category, event_type, payload, timestamp FROM telemetry_events".to_string();
        if category.is_some() {
            query.push_str(" WHERE category = ?1");
        }
        query.push_str(" ORDER BY timestamp DESC LIMIT ?2");

        let mut stmt = conn.prepare(&query)?;

        let map_row = |row: &rusqlite::Row| {
            Ok(serde_json::json!({
                "category": row.get::<_, String>(0)?,
                "event_type": row.get::<_, String>(1)?,
                "payload": serde_json::from_str::<serde_json::Value>(&row.get::<_, String>(2)?).unwrap_or_default(),
                "timestamp": row.get::<_, String>(3)?
            }))
        };

        let rows = if let Some(cat) = category {
            stmt.query_map(params![cat, limit as i64], map_row)?
        } else {
            stmt.query_map(params![limit as i64], map_row)?
        };
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn save_prompt_variant(
        &self,
        variant_name: &str,
        prompt_text: &str,
        parent_version: Option<i64>,
    ) -> anyhow::Result<i64> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO prompt_lineage (parent_version, variant_name, prompt_text, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                parent_version,
                variant_name,
                prompt_text,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_active_prompt(&self) -> anyhow::Result<Option<String>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT prompt_text FROM prompt_lineage WHERE is_active = 1 LIMIT 1"
        )?;
        let prompt = stmt.query_row([], |row| row.get(0)).ok();
        Ok(prompt)
    }
}
