use crate::dream::sandbox::DreamSandbox;
use crate::dream::{MemoryOperation, OperationKind};
use crate::memory::MemorySystem;
use crate::query::engine::QueryEngine;
use std::collections::HashMap;

pub struct ThematicSynthesizer;

impl ThematicSynthesizer {
    /// Detects L2 clusters that meet the '8/5 Rule' for consolidation.
    pub fn detect_consolidation_targets(
        memory: &MemorySystem,
    ) -> anyhow::Result<Vec<(String, Vec<String>)>> {
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();

        let all_topics = memory.topics.list_all()?;
        for path in all_topics {
            let parts: Vec<&str> = path.split('/').collect();
            if parts.len() >= 2 {
                let topic = parts[0];
                groups.entry(topic.to_string()).or_default().push(path);
            }
        }

        let mut targets = Vec::new();
        for (topic, paths) in groups {
            let mut total_chars = 0;
            for path in &paths {
                if let Some(content) = memory.topics.read(path)? {
                    total_chars += content.len();
                }
            }

            // The 8/5 Rule: 5+ files OR 32k characters (~8k tokens)
            if paths.len() >= 5 || total_chars >= 32_000 {
                targets.push((topic, paths));
            }
        }

        Ok(targets)
    }

    /// Detects 'Feature Vacuums': implementation gaps identified during L3 consolidation.
    pub fn detect_feature_vacuums(memory: &MemorySystem) -> anyhow::Result<Vec<(String, String)>> {
        let mut vacuums = Vec::new();
        let all_topics = memory.topics.list_all()?;
        for path in all_topics {
            if path.contains("chapter") {
                if let Some(content) = memory.topics.read(&path)? {
                    if content.contains("TODO")
                        || content.contains("FIXME")
                        || content.contains("MISSING")
                    {
                        vacuums.push((
                            path,
                            "Detected implementation gap in architecture chapter".to_string(),
                        ));
                    }
                }
            }
        }
        Ok(vacuums)
    }

    pub async fn propose_synthesis(
        topic: &str,
        l2_paths: &[String],
        memory: &MemorySystem,
        query_engine: &QueryEngine,
        sandbox: &mut DreamSandbox,
    ) -> anyhow::Result<MemoryOperation> {
        let mut aggregate_content = String::new();
        for path in l2_paths {
            if let Some(content) = memory.topics.read(path)? {
                aggregate_content.push_str(&format!("### Source: {}\n{}\n\n", path, content));
            }
        }

        let synthesis_prompt = format!(
            r#"You are the autoDream Thematic Synthesizer.
Consolidate the following fragmented L2 observations into a single high-level L3 THEME CHAPTER for the topic: '{}'.

## FRAGMENTS
{}

## INSTRUCTIONS
1. Create a comprehensive, well-structured Markdown document.
2. Resolve any minor redundant details while preserving all verified facts.
3. Ensure semantic links [[Topic/Subtopic]] are preserved or improved.
4. Output only the synthesized content."#,
            topic, aggregate_content
        );

        let system_prompt = "You are a master of technical synthesis. Create high-density, low-redundancy L3 chapters.";
        let synthesized = sandbox
            .sandboxed_llm_call(query_engine, system_prompt, &synthesis_prompt)
            .await?;

        Ok(MemoryOperation {
            kind: OperationKind::ConsolidateTheme {
                l2_paths: l2_paths.to_vec(),
            },
            topic: topic.to_string(),
            subtopic: "chapter".to_string(),
            content: synthesized,
            reasoning: format!(
                "Thematic synthesis of {} fragments (8/5 Rule triggered).",
                l2_paths.len()
            ),
            confidence: 1.0,
        })
    }
}
