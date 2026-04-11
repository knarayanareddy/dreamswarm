use crate::api::telemetry::TelemetryHub;
use crate::query::engine::QueryEngine;
use std::sync::Arc;

pub struct PromptAnalyzer {
    engine: Arc<QueryEngine>,
    telemetry: Arc<TelemetryHub>,
}

impl PromptAnalyzer {
    pub fn new(engine: Arc<QueryEngine>, telemetry: Arc<TelemetryHub>) -> Self {
        Self { engine, telemetry }
    }

    /// Analyzes daily performance and generates a 'Challenger' prompt.
    pub async fn generate_challenger_prompt(&self) -> anyhow::Result<String> {
        let history = self.telemetry.get_history(None, 200).await?;
        let stats_summary = self.extract_stats(&history);

        tracing::info!("Neural Evolution: Analyzing telemetry for prompt optimization...");

        let system_critique_prompt = format!(
            "You are the Neural Optimizer for DreamSwarm. Analyze the following system performance stats and the current base prompt. 
             Suggest a 'Challenger' version of the system prompt that aims to reduce token consumption by at least 10% or fix recurring failure patterns identified in the telemetry.
             
             TELEMETRY STATS:
             {}
             
             CURRENT BASE PROMPT:
             Assume the current prompt is the foundational Galactic Intelligence directive.
             
             Return ONLY the raw text of the improved prompt.",
            stats_summary
        );

        let messages = serde_json::json!({"role": "user", "content": system_critique_prompt});
        let response = self
            .engine
            .complete("You are a prompt engineer.", &[messages], &[])
            .await?;

        // Use MiniMax-2.5 via galactic router (configured in router.rs)
        tracing::info!("Neural Evolution: Generated new Challenger variant.");
        Ok(response
            .content
            .first()
            .and_then(|c| c["content"].as_str())
            .unwrap_or("Fallback Prompt")
            .to_string())
    }

    fn extract_stats(&self, history: &[serde_json::Value]) -> String {
        // Simple extraction for the prompt context
        let errors = history
            .iter()
            .filter(|e| e["category"] == "immune" || e["event_type"] == "error")
            .count();
        let total = history.len();

        format!(
            "Total Events: {}, Critical Errors: {}. Common fail signals: GIT_LOCK, TIMEOUT.",
            total, errors
        )
    }
}
