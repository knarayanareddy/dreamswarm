use serde_json::Value;

pub struct MicroCompact {
    /// Tool outputs older than this many turns are eligible for trimming
    pub trim_after_turns: u64,
    /// Max lines to keep in trimmed tool outputs
    pub max_trimmed_lines: usize,
    /// Max characters for any single tool output
    pub max_output_chars: usize,
}

impl Default for MicroCompact {
    fn default() -> Self {
        Self {
            trim_after_turns: 5,
            max_trimmed_lines: 20,
            max_output_chars: 3000,
        }
    }
}

impl MicroCompact {
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply micro-compaction to the message array in-place.
    /// Returns the number of tokens saved (estimated).
    pub fn compact(&self, messages: &mut Vec<Value>, current_turn: u64) -> usize {
        let mut tokens_saved: usize = 0;
        let mut seen_file_reads: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        for (idx, msg) in messages.iter_mut().enumerate() {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            if role == "user" {
                if let Some(content) = msg.get_mut("content") {
                    if let Some(blocks) = content.as_array_mut() {
                        for block in blocks.iter_mut() {
                            if block.get("type").and_then(|t| t.as_str()) == Some("tool_result") {
                                tokens_saved += self.compact_tool_result(
                                    block,
                                    idx,
                                    current_turn,
                                    &mut seen_file_reads,
                                );
                            }
                        }
                    }
                }
            }
        }
        tokens_saved
    }

    fn compact_tool_result(
        &self,
        block: &mut Value,
        message_index: usize,
        current_turn: u64,
        _seen_reads: &mut std::collections::HashMap<String, usize>,
    ) -> usize {
        let content = match block.get("content").and_then(|c| c.as_str()) {
            Some(c) => c.to_string(),
            None => return 0,
        };
        let original_tokens = content.len() / 4;

        // RULE 1: Truncate very large tool outputs
        if content.len() > self.max_output_chars {
            let lines: Vec<&str> = content.lines().collect();
            let keep = self.max_trimmed_lines;
            let trimmed = if lines.len() > keep * 2 {
                let first: Vec<&str> = lines[..keep].to_vec();
                let last: Vec<&str> = lines[lines.len() - keep..].to_vec();
                format!(
                    "{}\n\n... [{} lines omitted] ...\n\n{}",
                    first.join("\n"),
                    lines.len() - keep * 2,
                    last.join("\n")
                )
            } else {
                format!("{}\n... [truncated]", &content[..self.max_output_chars])
            };
            block["content"] = Value::String(trimmed.clone());
            let new_tokens = trimmed.len() / 4;
            return original_tokens.saturating_sub(new_tokens);
        }

        // RULE 3: Compress old "no matches" / empty results
        if content.contains("No matches found") || content.contains("(no output)") {
            if message_index < (current_turn as usize).saturating_sub(3) {
                let compressed = "[no results]";
                block["content"] = Value::String(compressed.to_string());
                let new_tokens = compressed.len() / 4;
                return original_tokens.saturating_sub(new_tokens);
            }
        }
        0
    }
}
