use crate::dream::sandbox::DreamSandbox;
use crate::dream::{MemoryOperation, OperationKind, PruneReason, RawObservation};
use crate::query::engine::QueryEngine;

pub struct DreamAnalyzer;

impl DreamAnalyzer {
    pub async fn analyze(
        observations: &[RawObservation],
        current_memory_snapshot: &str,
        sandbox: &mut DreamSandbox,
        query_engine: &QueryEngine,
    ) -> anyhow::Result<Vec<MemoryOperation>> {
        if observations.is_empty() { return Ok(vec![]); }
        let observations_text = Self::format_observations(observations);
        let analysis_prompt = format!(
            r#"You are performing MEMORY CONSOLIDATION (autoDream).
Analyze NEW OBSERVATIONS against CURRENT MEMORY and output a JSON array of operations.

## CURRENT MEMORY
{current_memory}

## NEW OBSERVATIONS
{observations}

## OPERATIONS
- {{"kind": "merge", "topic": "...", "subtopic": "...", "content": "...", "reasoning": "...", "confidence": 0.0-1.0, "source_entries": ["path1.md", "path2.md"]}}
- {{"kind": "update", "topic": "...", "subtopic": "...", "content": "...", "reasoning": "...", "confidence": 0.0-1.0, "existing_path": "..."}}
- {{"kind": "create", "topic": "...", "subtopic": "...", "content": "...", "reasoning": "...", "confidence": 0.0-1.0}}
- {{"kind": "prune", "topic": "...", "subtopic": "...", "content": "", "reasoning": "...", "confidence": 0.0, "prune_reason": "contradicted|stale|derivable|duplicate|low_confidence"}}
- {{"kind": "confirm", "topic": "...", "subtopic": "...", "content": "...", "reasoning": "...", "confidence": 0.9, "from_confidence": "observed", "to_confidence": "verified"}}

Output ONLY the JSON array."#,
            current_memory = current_memory_snapshot,
            observations = observations_text
        );

        let system_prompt = "You are a precise memory consolidation engine. Be conservative.";
        let response = sandbox.sandboxed_llm_call(query_engine, system_prompt, &analysis_prompt).await?;
        let operations = Self::parse_operations(&response)?;
        Ok(operations)
    }

    fn format_observations(observations: &[RawObservation]) -> String {
        observations.iter().enumerate().map(|(i, obs)| {
            format!("{}. [{}] [{:?}] (conf: {:.1}): {}", i + 1, obs.timestamp.format("%Y-%m-%d %H:%M"), obs.source, obs.confidence, &obs.content[..obs.content.len().min(400)])
        }).collect::<Vec<_>>().join("\n")
    }

    fn parse_operations(response: &str) -> anyhow::Result<Vec<MemoryOperation>> {
        let json_str = Self::extract_json_array(response);
        let raw: Vec<serde_json::Value> = serde_json::from_str(&json_str)?;
        let mut operations = Vec::new();
        for item in raw {
            let kind_str = item.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown");
            let topic = item.get("topic").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let subtopic = item.get("subtopic").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let reasoning = item.get("reasoning").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let confidence = item.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5);

            let kind = match kind_str {
                "merge" => {
                    let sources = item.get("source_entries").and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default();
                    OperationKind::Merge { source_entries: sources }
                },
                "update" => {
                    let path = item.get("existing_path").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    OperationKind::Update { existing_path: path }
                },
                "create" => OperationKind::Create,
                "prune" => {
                    let reason = match item.get("prune_reason").and_then(|v| v.as_str()).unwrap_or("stale") {
                        "contradicted" => PruneReason::Contradicted,
                        "derivable" => PruneReason::Derivable,
                        "duplicate" => PruneReason::Duplicate,
                        "low_confidence" => PruneReason::LowConfidence,
                        _ => PruneReason::Stale,
                    };
                    OperationKind::Prune { reason }
                },
                "confirm" => {
                    let from = item.get("from_confidence").and_then(|v| v.as_str()).unwrap_or("observed").to_string();
                    let to = item.get("to_confidence").and_then(|v| v.as_str()).unwrap_or("verified").to_string();
                    OperationKind::Confirm { from_confidence: from, to_confidence: to }
                },
                _ => continue,
            };
            operations.push(MemoryOperation { kind, topic, subtopic, content, reasoning, confidence });
        }
        Ok(operations)
    }

    fn extract_json_array(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.starts_with('[') { return trimmed.to_string(); }
        if let Some(start) = trimmed.find("```json") {
            let after = &trimmed[start + 7..];
            if let Some(end) = after.find("```") { return after[..end].trim().to_string(); }
        }
        if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') { return trimmed[start..=end].to_string(); }
        }
        "[]".to_string()
    }
}
