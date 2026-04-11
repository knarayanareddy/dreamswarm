use crate::swarm::evolution::prompt_evolution::PromptAnalyzer;
use crate::api::telemetry::TelemetryHub;
use crate::db::Database;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{Utc, Duration};

pub struct EvolutionCoordinator {
    analyzer: PromptAnalyzer,
    db: Arc<RwLock<Database>>,
    telemetry: Arc<TelemetryHub>,
}

impl EvolutionCoordinator {
    pub fn new(analyzer: PromptAnalyzer, db: Arc<RwLock<Database>>, telemetry: Arc<TelemetryHub>) -> Self {
        Self { analyzer, db, telemetry }
    }

    /// Checks if a daily optimization cycle is due and runs it.
    pub async fn run_cycle_if_due(&self) -> anyhow::Result<()> {
        let last_evolve = self.get_last_evolution_time().await?;
        if Utc::now() - last_evolve < Duration::days(1) {
            return Ok(());
        }

        tracing::info!("Evolution Coordinator: Daily optimization cycle triggered.");

        // 1. Generate Challenger
        let challenger_text = self.analyzer.generate_challenger_prompt().await?;

        // 2. Save to Lineage (Inactive initially)
        {
            let db = self.db.read().await;
            db.save_prompt_variant("challenger_alpha", &challenger_text, None)?;
        }

        // 3. Broadcast to Swarm
        let preview = if challenger_text.len() > 50 {
            &challenger_text[..50]
        } else {
            &challenger_text
        };

        self.telemetry.log_event("swarm", "evolution_variant_created", serde_json::json!({
            "variant": "challenger_alpha",
            "reason": "Daily Optimization Cycle",
            "prompt_preview": preview
        })).await;

        Ok(())
    }

    async fn get_last_evolution_time(&self) -> anyhow::Result<chrono::DateTime<Utc>> {
        let db = self.db.read().await;
        let conn = db.pool().get()?;
        let mut stmt = conn.prepare("SELECT created_at FROM prompt_lineage ORDER BY created_at DESC LIMIT 1")?;
        let last_time: Option<String> = stmt.query_row([], |row| row.get::<_, String>(0)).ok();
        
        match last_time {
            Some(t) => {
                let dt = chrono::DateTime::parse_from_rfc3339(&t)?
                    .with_timezone(&Utc);
                Ok(dt)
            }
            None => Ok(Utc::now() - Duration::days(2)), // First time
        }
    }
}
