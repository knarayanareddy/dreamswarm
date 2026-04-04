use crate::query::engine::{CompletionResponse, LLMProvider};
use serde_json::Value;

pub struct AutoCompact {
    pub reserved_buffer_tokens: usize,
    pub max_summary_tokens: usize,
    pub preserve_recent_turns: usize,
    pub consecutive_failures: u32,
    pub max_failures: u32,
}

impl Default for AutoCompact {
    fn default() -> Self {
        Self {
            reserved_buffer_tokens: 13_000,
            max_summary_tokens: 20_000,
            preserve_recent_turns: 3,
            consecutive_failures: 0,
            max_failures: 3,
        }
    }
}

#[derive(Debug)]
pub struct CompactionSummary {
    pub summary_text: String,
    pub tokens_before: usize,
    pub tokens_after: usize,
    pub turns_compressed: usize,
    pub turns_preserved: usize,
}

impl AutoCompact {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_disabled(&self) -> bool {
        self.consecutive_failures >= self.max_failures
    }

    pub fn reset(&mut self) {
        self.consecutive_failures = 0;
    }

    pub async fn compact(
        &mut self,
        messages: &[Value],
        provider: &dyn LLMProvider,
    ) -> anyhow::Result<CompactionSummary> {
        if self.is_disabled() {
            anyhow::bail!("AutoCompact circuit breaker tripped after {} failures", self.max_failures);
        }

        let total_messages = messages.len();
        let preserve_count = self.count_recent_turns(messages, self.preserve_recent_turns);
        let split_point = total_messages.saturating_sub(preserve_count);

        if split_point <= 1 {
            anyhow::bail!("Not enough messages to compact");
        }

        let old_messages = &messages[..split_point];
        let tokens_before = self.estimate_tokens_array(old_messages);
        let summary_prompt = self.build_summary_prompt(old_messages);

        let summary_messages = vec![serde_json::json!({ "role": "user", "content": summary_prompt })];

        match provider.complete(
            "You are a precise conversation summarizer. Your job is to compress conversation history while preserving all critical information.",
            &summary_messages,
            &[]
        ).await {
            Ok(response) => {
                let summary_text = self.extract_text(&response);
                let tokens_after = summary_text.len() / 4;
                self.consecutive_failures = 0;
                Ok(CompactionSummary {
                    summary_text,
                    tokens_before,
                    tokens_after,
                    turns_compressed: split_point,
                    turns_preserved: preserve_count,
                })
            }
            Err(e) => {
                self.consecutive_failures += 1;
                tracing::warn!("AutoCompact failed ({}/{}): {}", self.consecutive_failures, self.max_failures, e);
                Err(e)
            }
        }
    }

    fn build_summary_prompt(&self, messages: &[Value]) -> String {
        let mut conversation = String::new();
        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("unknown");
            match msg.get("content") {
                Some(Value::String(text)) => {
                    conversation.push_str(&format!("[{}]: {}\n\n", role, text));
                }
                Some(Value::Array(blocks)) => {
                    for block in blocks {
                        match block.get("type").and_then(|t| t.as_str()) {
                            Some("text") => {
                                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                    conversation.push_str(&format!("[{}]: {}\n\n", role, text));
                                }
                            }
                            Some("tool_use") => {
                                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("?");
                                conversation.push_str(&format!("[assistant called tool: {}]\n\n", name));
                            }
                            Some("tool_result") => {
                                let content = block.get("content").and_then(|c| c.as_str()).unwrap_or("");
                                let preview = if content.len() > 200 { format!("{}...", &content[..200]) } else { content.to_string() };
                                conversation.push_str(&format!("[tool_result]: {}\n\n", preview));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        format!(
            r#"Summarize the following conversation history into a structured summary.
Your summary MUST preserve:
1. **All file paths** mentioned or modified (exact paths)
2. **All decisions made** and their rationale
3. **All errors encountered** and how they were resolved
4. **Current task status** — what remains
5. **Key code changes** — what files were edited and what changed
6. **Important discoveries** — architecture insights, known issues found

Format your summary as:
## Task
## Progress
## Files Modified
## Errors & Resolutions
## Open Items
## Key Insights

Conversation to summarize:
---
{conversation}
---
Produce the structured summary now. Be concise but miss nothing important."#,
            conversation = conversation
        )
    }

    fn extract_text(&self, response: &CompletionResponse) -> String {
        response.content.iter().filter_map(|block| {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                block.get("text").and_then(|t| t.as_str()).map(String::from)
            } else {
                None
            }
        }).collect::<Vec<_>>().join("\n")
    }

    fn count_recent_turns(&self, messages: &[Value], turns: usize) -> usize {
        let mut turn_count = 0;
        let mut msg_count = 0;
        for msg in messages.iter().rev() {
            msg_count += 1;
            if msg.get("role").and_then(|r| r.as_str()) == Some("user") {
                let is_tool_result = msg.get("content").and_then(|c| c.as_array()).map(|blocks| {
                    blocks.iter().all(|b| b.get("type").and_then(|t| t.as_str()) == Some("tool_result"))
                }).unwrap_or(false);
                
                if !is_tool_result {
                    turn_count += 1;
                    if turn_count >= turns {
                        return msg_count;
                    }
                }
            }
        }
        msg_count
    }

    fn estimate_tokens_array(&self, messages: &[Value]) -> usize {
        messages.iter().map(|m| m.to_string().len() / 4 + 4).sum()
    }
}
