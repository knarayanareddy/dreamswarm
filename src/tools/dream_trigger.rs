use crate::dream::engine::DreamEngine;
use crate::dream::report::DreamReporter;
use crate::dream::DreamConfig;
use crate::memory::MemorySystem;
use crate::query::engine::QueryEngine;
use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct DreamTriggerTool {
    memory: Arc<RwLock<MemorySystem>>,
    query_engine: Arc<QueryEngine>,
    working_dir: String,
    daemon_state_dir: std::path::PathBuf,
}

impl DreamTriggerTool {
    pub fn new(
        memory: Arc<RwLock<MemorySystem>>,
        query_engine: Arc<QueryEngine>,
        working_dir: &str,
        daemon_state_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            memory,
            query_engine,
            working_dir: working_dir.to_string(),
            daemon_state_dir,
        }
    }
}

#[async_trait]
impl Tool for DreamTriggerTool {
    fn name(&self) -> &str {
        "DreamTrigger"
    }
    fn description(&self) -> &str {
        "Manually trigger a memory consolidation cycle (autoDream). \
        Analyzes logs and transcripts to merge duplicates, resolve contradictions, \
        and confirm facts. Runs in a sandboxed environment."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "lookback_days": { "type": "integer", "description": "Days of history to consider (default: 7)" },
                "dry_run": { "type": "boolean", "description": "Plan but don't apply (default: false)" }
            }
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Moderate
    }
    fn command_signature(&self, _: &Value) -> String {
        "dream:trigger".into()
    }
    fn describe_call(&self, input: &Value) -> String {
        let dry = input
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if dry {
            "Dry-run memory consolidation".into()
        } else {
            "Run memory consolidation (autoDream)".into()
        }
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let lookback = input
            .get("lookback_days")
            .and_then(|v| v.as_u64())
            .unwrap_or(7) as u32;
        let config = DreamConfig {
            lookback_days: lookback,
            ..Default::default()
        };
        let engine = DreamEngine::new(
            config,
            std::path::PathBuf::from(&self.working_dir),
            self.daemon_state_dir.clone(),
        );
        let memory = self.memory.read().await;
        let report = engine.dream(&memory, &self.query_engine).await?;
        Ok(ToolOutput {
            content: DreamReporter::format(&report),
            is_error: false,
        })
    }
}
