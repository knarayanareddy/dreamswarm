use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use std::path::PathBuf;

fn knowledge_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".dreamswarm")
        .join("knowledge")
}

/// Allows an agent to publish a "finding" or "lesson learned" to the
/// shared knowledge graph so that all other agents can benefit from it.
pub struct PublishKnowledgeTool;

#[async_trait]
impl Tool for PublishKnowledgeTool {
    fn name(&self) -> &str {
        "PublishKnowledge"
    }

    fn description(&self) -> &str {
        "Publish a key finding, lesson learned, or architectural decision to the shared swarm knowledge graph. Other agents can retrieve it via SearchKnowledge."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title":   { "type": "string", "description": "Short title for this knowledge entry." },
                "content": { "type": "string", "description": "The full content / finding to store." },
                "tags":    { "type": "array",  "items": { "type": "string" }, "description": "Relevant tags (e.g. [\"security\", \"rust\", \"memory\"])." }
            },
            "required": ["title", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let title = input["title"].as_str().unwrap_or("Untitled");
        let content = input["content"].as_str().unwrap_or("");
        let tags: Vec<String> = input["tags"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let dir = knowledge_dir();
        std::fs::create_dir_all(&dir)?;

        let id = uuid::Uuid::new_v4().to_string();
        let entry = serde_json::json!({
            "id": id,
            "title": title,
            "content": content,
            "tags": tags,
            "published_at": chrono::Utc::now().to_rfc3339()
        });

        let path = dir.join(format!("{}.json", id));
        std::fs::write(&path, serde_json::to_string_pretty(&entry)?)?;

        tracing::info!("Knowledge published: '{}' (id: {})", title, id);
        Ok(ToolOutput {
            content: format!("✅ Knowledge published successfully (id: {})", id),
            is_error: false,
        })
    }
}

/// Allows an agent to search the shared knowledge graph for findings
/// published by any agent in the swarm.
pub struct SearchKnowledgeTool;

#[async_trait]
impl Tool for SearchKnowledgeTool {
    fn name(&self) -> &str {
        "SearchKnowledge"
    }

    fn description(&self) -> &str {
        "Search the shared swarm knowledge graph for relevant findings, lessons, or decisions published by other agents."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Keywords to search for across all knowledge entries." }
            },
            "required": ["query"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let query = input["query"].as_str().unwrap_or("").to_lowercase();
        let dir = knowledge_dir();

        if !dir.exists() {
            return Ok(ToolOutput {
                content: "Knowledge graph is empty. No findings have been published yet.".into(),
                is_error: false,
            });
        }

        let mut results: Vec<String> = vec![];

        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let raw = std::fs::read_to_string(entry.path())?;
            if let Ok(doc) = serde_json::from_str::<Value>(&raw) {
                let title = doc["title"].as_str().unwrap_or("").to_lowercase();
                let content = doc["content"].as_str().unwrap_or("").to_lowercase();
                let tags = doc["tags"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default()
                    .to_lowercase();

                if title.contains(&query) || content.contains(&query) || tags.contains(&query) {
                    results.push(format!(
                        "### {}\n*Published: {}*\n{}\n**Tags:** {}\n",
                        doc["title"].as_str().unwrap_or(""),
                        doc["published_at"].as_str().unwrap_or(""),
                        doc["content"].as_str().unwrap_or(""),
                        doc["tags"]
                            .as_array()
                            .map(|a| a
                                .iter()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", "))
                            .unwrap_or_default()
                    ));
                }
            }
        }

        if results.is_empty() {
            Ok(ToolOutput {
                content: format!("No knowledge entries found matching '{}'.", query),
                is_error: false,
            })
        } else {
            Ok(ToolOutput {
                content: format!(
                    "## Knowledge Graph Results for '{}'\n\n{} result(s) found:\n\n{}",
                    query,
                    results.len(),
                    results.join("\n---\n")
                ),
                is_error: false,
            })
        }
    }
}
