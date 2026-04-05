use crate::query::engine::LLMProvider;
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

pub struct FullCompact {
    pub max_file_reinjection_tokens: usize,
    pub max_reinjected_files: usize,
    pub post_compact_budget_tokens: usize,
}

impl Default for FullCompact {
    fn default() -> Self {
        Self {
            max_file_reinjection_tokens: 5_000,
            max_reinjected_files: 5,
            post_compact_budget_tokens: 50_000,
        }
    }
}

#[derive(Debug)]
pub struct FullCompactResult {
    pub compacted_messages: Vec<Value>,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub files_reinjected: Vec<String>,
}

impl FullCompact {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn compact(
        &self,
        messages: &[Value],
        provider: &dyn LLMProvider,
    ) -> anyhow::Result<FullCompactResult> {
        let tokens_before: usize = messages.iter().map(|m| m.to_string().len() / 4 + 4).sum();

        // Step 1: Generate full summary
        let summary = self.generate_full_summary(messages, provider).await?;

        // Step 2: Extract recent file paths
        let recent_files = self.extract_recent_files(messages);

        // Step 3: Build compacted message array
        let mut compacted = Vec::new();
        compacted.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "[CONTEXT COMPACTION: The conversation history has been compressed. \
                Below is a structured summary of everything that happened.]\n\n{}",
                summary
            )
        }));

        compacted.push(serde_json::json!({
            "role": "assistant",
            "content": "I understand. I've reviewed the compressed context and I'm ready to continue where we left off."
        }));

        // Step 4: Re-inject recent files
        let mut reinjected_files = Vec::new();
        let mut reinjection_tokens = 0;

        for file_path in recent_files.iter().take(self.max_reinjected_files) {
            if reinjection_tokens >= self.max_file_reinjection_tokens * self.max_reinjected_files {
                break;
            }
            let path = Path::new(file_path);
            if path.exists() {
                if let Ok(content) = tokio::fs::read_to_string(path).await {
                    let tokens = content.len() / 4;
                    let truncated = if tokens > self.max_file_reinjection_tokens {
                        let max_chars = self.max_file_reinjection_tokens * 4;
                        format!(
                            "{}\n... [truncated at {} tokens]",
                            &content[..max_chars],
                            self.max_file_reinjection_tokens
                        )
                    } else {
                        content
                    };

                    compacted.push(serde_json::json!({
                        "role": "user",
                        "content": format!("[Re-injected file: {}]\n\n{}", file_path, truncated)
                    }));
                    compacted.push(serde_json::json!({
                        "role": "assistant",
                        "content": format!("Noted: I've re-read {}.", file_path)
                    }));

                    reinjected_files.push(file_path.clone());
                    reinjection_tokens += tokens.min(self.max_file_reinjection_tokens);
                }
            }
        }

        let tokens_after: usize = compacted.iter().map(|m| m.to_string().len() / 4 + 4).sum();

        Ok(FullCompactResult {
            compacted_messages: compacted,
            tokens_before,
            tokens_after,
            files_reinjected: reinjected_files,
        })
    }

    async fn generate_full_summary(
        &self,
        messages: &[Value],
        provider: &dyn LLMProvider,
    ) -> anyhow::Result<String> {
        let mut condensed = String::new();
        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("?");
            match msg.get("content") {
                Some(Value::String(text)) => {
                    let preview = if text.len() > 500 {
                        format!("{}...", &text[..500])
                    } else {
                        text.clone()
                    };
                    condensed.push_str(&format!("[{}]: {}\n", role, preview));
                }
                Some(Value::Array(blocks)) => {
                    for block in blocks {
                        let type_str = block.get("type").and_then(|t| t.as_str()).unwrap_or("?");
                        match type_str {
                            "text" => {
                                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                                let preview = if text.len() > 300 {
                                    format!("{}...", &text[..300])
                                } else {
                                    text.to_string()
                                };
                                condensed.push_str(&format!("[{}]: {}\n", role, preview));
                            }
                            "tool_use" => {
                                let name =
                                    block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                                condensed.push_str(&format!("[tool_call: {}]\n", name));
                            }
                            "tool_result" => {
                                let content =
                                    block.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                let preview = if content.len() > 100 {
                                    format!("{}...", &content[..100])
                                } else {
                                    content.to_string()
                                };
                                condensed.push_str(&format!("[tool_result]: {}\n", preview));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        let prompt = format!(
            r#"Perform EMERGENCY context compression. Produce a comprehensive summary that preserves ALL critical info.
Include:
1. Original user request
2. Every file path read/written
3. All code changes made
4. All errors and resolutions
5. Current task status

Conversation: --- {condensed} ---
Produce the emergency summary now:"#,
            condensed = condensed
        );

        let messages = vec![serde_json::json!({ "role": "user", "content": prompt })];
        let response = provider
            .complete("Emergency summary mode.", &messages, &[])
            .await?;

        Ok(response
            .content
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                    b.get("text").and_then(|t| t.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn extract_recent_files(&self, messages: &[Value]) -> Vec<String> {
        let mut files = Vec::new();
        let mut seen = HashSet::new();

        for msg in messages.iter().rev() {
            if let Some(content) = msg.get("content") {
                if let Some(blocks) = content.as_array() {
                    for block in blocks {
                        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                            let tool_name =
                                block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if tool_name == "FileRead" || tool_name == "FileWrite" {
                                if let Some(path) = block
                                    .get("input")
                                    .and_then(|i| i.get("path"))
                                    .and_then(|p| p.as_str())
                                {
                                    if !seen.contains(path) {
                                        seen.insert(path.to_string());
                                        files.push(path.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        files
    }
}
