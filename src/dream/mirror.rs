use crate::daemon::daily_log::{DailyLog, LogEntry, LogEntryKind};
use crate::dream::sandbox::DreamSandbox;
use crate::dream::{AgentHealth, AgentVitals, MemoryOperation, MirrorSnapshot};
use crate::query::engine::QueryEngine;
use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct MirrorEngine {
    state_dir: PathBuf,
}

impl MirrorEngine {
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    pub async fn reflect(
        &self,
        query_engine: &QueryEngine,
        sandbox: &mut DreamSandbox,
    ) -> anyhow::Result<Vec<MemoryOperation>> {
        tracing::info!("Mirror Cycle: Initiating self-reflection...");

        let snapshot = self.generate_snapshot()?;
        let reflection_prompt = self.format_reflection_prompt(&snapshot);

        let system_prompt = "You are the DreamSwarm Mirror Instance. Your goal is to optimize the swarm's cognitive efficiency by identifying patterns in logs and operations.";

        let response = sandbox
            .sandboxed_llm_call(query_engine, system_prompt, &reflection_prompt)
            .await?;
        let insights = self.parse_mirror_insights(&response);

        Ok(insights)
    }

    pub fn generate_snapshot(&self) -> anyhow::Result<MirrorSnapshot> {
        let daily_log = DailyLog::new(&self.state_dir)?;
        let recent_entries = daily_log.read_recent_days(7)?;

        let mut total_tokens = 0;
        let mut total_ops = 0;
        let mut conflicts = 0;
        let mut session_map: HashMap<String, Vec<LogEntry>> = HashMap::new();

        for entry in &recent_entries {
            total_tokens += entry.tokens_consumed;
            if let Some(ref sid) = entry.session_id {
                session_map
                    .entry(sid.clone())
                    .or_default()
                    .push(entry.clone());
            }
            if entry.kind == LogEntryKind::Error {
                conflicts += 1;
            }
            if entry.kind == LogEntryKind::Decision {
                total_ops += 1;
            }
        }

        let mut agent_stats = HashMap::new();
        for (sid, entries) in session_map {
            let mut vitals = AgentVitals::default();
            let mut tool_history = Vec::new();

            for entry in &entries {
                if entry.kind == LogEntryKind::Action {
                    vitals.last_tool_call = Some(entry.timestamp);
                    let tool_signature = format!("{:?}_{}", entry.tools_used, entry.content);
                    tool_history.push(tool_signature);
                }
            }

            // Detect loops: last 3 tools are identical
            if tool_history.len() >= 3 {
                let last_three = &tool_history[tool_history.len() - 3..];
                if last_three.iter().all(|x| x == &last_three[0]) {
                    vitals.tool_loop_count = tool_history.len();
                    vitals.is_stalled = true;
                }
            }

            // Simple entropy: variety of tools used
            let unique_tools: std::collections::HashSet<_> = tool_history.iter().collect();
            vitals.entropy_score = unique_tools.len() as f64 / tool_history.len().max(1) as f64;

            agent_stats.insert(
                sid,
                AgentHealth {
                    success_count: entries
                        .iter()
                        .filter(|e| e.kind == LogEntryKind::Action)
                        .count() as usize,
                    conflict_count: entries
                        .iter()
                        .filter(|e| e.kind == LogEntryKind::Error)
                        .count() as usize,
                    avg_confidence: 0.8, // Placeholder
                    vitals,
                },
            );
        }

        Ok(MirrorSnapshot {
            timestamp: Utc::now(),
            total_ops: total_ops.max(1),
            conflict_rate: conflicts as f64 / total_ops.max(1) as f64,
            token_efficiency: total_tokens as f64 / total_ops.max(1) as f64,
            trust_score: 0.5, // Placeholder for Phase 4
            most_volatile_topic: None,
            agent_performance: agent_stats,
        })
    }

    fn format_reflection_prompt(&self, snapshot: &MirrorSnapshot) -> String {
        format!(
            r#"## SWARM MIRROR SNAPSHOT ({})
- Total Operations: {}
- Conflict Rate: {:.2}%
- Token Efficiency: {:.1} tokens/op
- Volatile Topic: {}

## INSTRUCTIONS
Analyze the performance metrics above and the recent history of the swarm.
Output a JSON array of operations to improve the system.

OPERATIONS:
- {{"kind": "refine_instructions", "agent_id": "...", "new_instructions": "...", "reasoning": "..."}}
- {{"kind": "consolidate_theme", "topic": "...", "l2_paths": ["..."], "reasoning": "..."}}
- {{"kind": "heal_agent", "agent_id": "...", "reason": "...", "reasoning": "..."}}

Focus on reducing conflicts and recovering stalled agents."#,
            snapshot.timestamp.format("%Y-%m-%d"),
            snapshot.total_ops,
            snapshot.conflict_rate * 100.0,
            snapshot.token_efficiency,
            snapshot.most_volatile_topic.as_deref().unwrap_or("None")
        )
    }

    fn parse_mirror_insights(&self, response: &str) -> Vec<MemoryOperation> {
        let mut insights = Vec::new();
        if let Ok(json_ops) = serde_json::from_str::<Vec<serde_json::Value>>(response) {
            for op in json_ops {
                let kind = op["kind"].as_str().unwrap_or("");
                let reasoning = op["reasoning"]
                    .as_str()
                    .unwrap_or("No reasoning provided")
                    .to_string();

                match kind {
                    "heal_agent" => {
                        insights.push(MemoryOperation {
                            kind: crate::dream::OperationKind::HealAgent {
                                agent_id: op["agent_id"].as_str().unwrap_or("unknown").to_string(),
                                reason: op["reason"]
                                    .as_str()
                                    .unwrap_or("Unknown stall")
                                    .to_string(),
                            },
                            topic: "Resilience".into(),
                            subtopic: op["agent_id"].as_str().unwrap_or("unknown").to_string(),
                            content: String::new(),
                            reasoning,
                            confidence: 1.0,
                        });
                    }
                    "refine_instructions" => {
                        insights.push(MemoryOperation {
                            kind: crate::dream::OperationKind::RefineInstructions {
                                agent_id: op["agent_id"].as_str().unwrap_or("unknown").to_string(),
                                new_instructions: op["new_instructions"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string(),
                            },
                            topic: "Optimization".into(),
                            subtopic: op["agent_id"].as_str().unwrap_or("unknown").to_string(),
                            content: String::new(),
                            reasoning,
                            confidence: 0.8,
                        });
                    }
                    _ => {}
                }
            }
        }
        insights
    }
}
