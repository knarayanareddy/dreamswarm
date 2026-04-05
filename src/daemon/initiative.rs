use crate::daemon::signals::{Signal, SignalKind};
use crate::daemon::trust::TrustSystem;
use crate::daemon::{DaemonConfig, Initiative, ProactiveAction, Urgency};
use crate::query::engine::QueryEngine;
use chrono::Utc;

pub struct InitiativeEngine {
    trust: TrustSystem,
    tokens_used_today: u64,
    cost_today: f64,
    config: DaemonConfig,
}

impl InitiativeEngine {
    pub fn new(config: DaemonConfig) -> Self {
        Self {
            trust: TrustSystem::new(),
            tokens_used_today: 0,
            cost_today: 0.0,
            config,
        }
    }

    pub async fn evaluate(
        &mut self,
        signals: &[Signal],
        query_engine: Option<&QueryEngine>,
    ) -> Initiative {
        if signals.is_empty() {
            return Initiative::Sleep;
        }
        if self.tokens_used_today >= self.config.daily_token_budget
            || self.cost_today >= self.config.daily_cost_budget
        {
            return Initiative::Observe("Budget exceeded".to_string());
        }
        if self.trust.is_paused() {
            return Initiative::Observe(format!(
                "Trust too low ({:.0}%)",
                self.trust.current_level * 100.0
            ));
        }

        if let Some(initiative) = self.rule_based_evaluation(signals) {
            return initiative;
        }

        if let Some(engine) = query_engine {
            if let Some(initiative) = self.llm_evaluation(signals, engine).await {
                return initiative;
            }
        }

        let descriptions: Vec<String> = signals.iter().map(|s| s.description.clone()).collect();
        Initiative::Observe(format!("Signals noted: {}", descriptions.join("; ")))
    }

    fn rule_based_evaluation(&self, signals: &[Signal]) -> Option<Initiative> {
        let build_failures: Vec<&Signal> = signals
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SignalKind::BuildError | SignalKind::TestFailure | SignalKind::CIFailed
                )
            })
            .collect();

        if !build_failures.is_empty() && self.trust.should_auto_act(&Urgency::High) {
            return Some(Initiative::Act(ProactiveAction::RunTests {
                reason: format!("Build/test failure: {}", build_failures[0].description),
                changed_files: build_failures
                    .iter()
                    .filter_map(|s| {
                        s.metadata
                            .get("path")
                            .and_then(|p| p.as_str())
                            .map(String::from)
                    })
                    .collect(),
            }));
        }

        let source_changes: Vec<&Signal> = signals
            .iter()
            .filter(|s| {
                s.kind == SignalKind::FileChanged
                    && s.metadata
                        .get("path")
                        .and_then(|p| p.as_str())
                        .map(|p| p.ends_with(".rs") || p.ends_with(".ts") || p.ends_with(".py"))
                        .unwrap_or(false)
            })
            .collect();

        if source_changes.len() >= 3 && self.trust.should_auto_act(&Urgency::Medium) {
            return Some(Initiative::Act(ProactiveAction::RunTests {
                reason: format!("{} source files changed", source_changes.len()),
                changed_files: source_changes
                    .iter()
                    .filter_map(|s| {
                        s.metadata
                            .get("path")
                            .and_then(|p| p.as_str())
                            .map(String::from)
                    })
                    .collect(),
            }));
        }
        None
    }

    async fn llm_evaluation(
        &self,
        signals: &[Signal],
        query_engine: &QueryEngine,
    ) -> Option<Initiative> {
        let signals_text: String = signals
            .iter()
            .map(|s| format!("- [{:?}] {:?}: {}", s.severity, s.kind, s.description))
            .collect::<Vec<_>>()
            .join("\n");

        let tick_prompt = format!(
            r#"<tick>
Current time: {} Trust level: {:.0}%
Budget remaining: {} tokens / ${:.2}
Recent signals: {}
Evaluate: is there anything worth doing right now?
Respond with exactly ONE of:
1. ACT: [specific action description]
2. OBSERVE: [what you noticed]
3. SLEEP
</tick>"#,
            Utc::now().format("%H:%M:%S UTC"),
            self.trust.current_level * 100.0,
            self.config
                .daily_token_budget
                .saturating_sub(self.tokens_used_today),
            self.config.daily_cost_budget - self.cost_today,
            signals_text
        );

        let messages = vec![serde_json::json!({ "role": "user", "content": tick_prompt })];
        match query_engine
            .complete(
                "You are a focused daemon evaluating proactive action. Be brief.",
                &messages,
                &[],
            )
            .await
        {
            Ok(response) => {
                let text = response
                    .content
                    .iter()
                    .filter_map(|b| {
                        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                            b.get("text").and_then(|t| t.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("");
                let text = text.trim();
                if text.starts_with("ACT:") {
                    Some(Initiative::Act(ProactiveAction::CustomAction {
                        description: text[4..].trim().to_string(),
                        tool_calls: vec![],
                    }))
                } else if text.starts_with("OBSERVE:") {
                    Some(Initiative::Observe(text[8..].trim().to_string()))
                } else {
                    Some(Initiative::Sleep)
                }
            }
            Err(_) => None,
        }
    }

    pub fn record_usage(&mut self, tokens: u64, cost: f64) {
        self.tokens_used_today += tokens;
        self.cost_today += cost;
    }

    pub fn reset_daily(&mut self) {
        self.tokens_used_today = 0;
        self.cost_today = 0.0;
    }

    pub fn trust(&self) -> &TrustSystem {
        &self.trust
    }
    pub fn trust_mut(&mut self) -> &mut TrustSystem {
        &mut self.trust
    }
}
