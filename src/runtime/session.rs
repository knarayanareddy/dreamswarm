use crate::db::Database;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub messages: Vec<Message>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub turn_count: u32,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
    pub token_count: Option<u64>,
    pub turn_number: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Role {
    System,
    User,
    Assistant,
    ToolResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageContent {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    Mixed(Vec<ContentBlock>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            messages: Vec::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            turn_count: 0,
            total_tokens: 0,
            total_cost_usd: 0.0,
            summary: None,
        }
    }

    pub fn resume(db: &Database, session_id: &str) -> anyhow::Result<Self> {
        db.load_session(session_id)
    }

    pub fn add_user_message(&mut self, text: &str) {
        self.turn_count += 1;
        self.messages.push(Message {
            role: Role::User,
            content: MessageContent::Text(text.to_string()),
            timestamp: Utc::now(),
            token_count: None,
            turn_number: self.turn_count,
        });
        self.updated_at = Utc::now();
    }

    pub fn add_assistant_message(&mut self, content: MessageContent, tokens: u64, cost: f64) {
        self.messages.push(Message {
            role: Role::Assistant,
            content,
            timestamp: Utc::now(),
            token_count: Some(tokens),
            turn_number: self.turn_count,
        });
        self.total_tokens += tokens;
        self.total_cost_usd += cost;
        self.updated_at = Utc::now();
    }

    /// Appends tool results for one assistant turn.
    ///
    /// All results for a single assistant message must reach the provider in
    /// *one* user message; emitting one message per result produces a
    /// conversation where a `tool_result` is separated from the `tool_use` that
    /// spawned it, which the APIs reject.
    pub fn add_tool_results(&mut self, results: &[(String, String, bool)]) {
        if results.is_empty() {
            return;
        }
        // Only results that answer the immediately preceding assistant turn may
        // be recorded. A stray id would leave a `tool_result` with no
        // `tool_use`, which both providers reject with a 400.
        let expected = self
            .messages
            .last()
            .map(|message| match &message.content {
                MessageContent::ToolUse { id, .. } => vec![id.clone()],
                MessageContent::Mixed(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, .. } => Some(id.clone()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            })
            .unwrap_or_default();

        let blocks: Vec<ContentBlock> = results
            .iter()
            .filter(|(tool_use_id, _, _)| expected.iter().any(|id| id == tool_use_id))
            .map(
                |(tool_use_id, content, is_error)| ContentBlock::ToolResult {
                    tool_use_id: tool_use_id.clone(),
                    content: content.clone(),
                    is_error: *is_error,
                },
            )
            .collect();
        if blocks.is_empty() {
            tracing::warn!("Dropped {} tool_result block(s) that answer no pending tool_use", results.len());
            return;
        }
        self.messages.push(Message {
            role: Role::User,
            content: MessageContent::Mixed(blocks),
            timestamp: Utc::now(),
            token_count: None,
            turn_number: self.turn_count,
        });
        self.updated_at = Utc::now();
    }

    /// Appends a single tool result (kept for callers that have one result).
    pub fn add_tool_result(&mut self, tool_use_id: &str, content: &str, is_error: bool) {
        self.add_tool_results(&[(tool_use_id.to_string(), content.to_string(), is_error)]);
    }

    /// Returns true when every `tool_use` has a matching `tool_result` in the
    /// immediately following message.
    ///
    /// The agent loop uses this to guarantee it never sends an orphaned result
    /// (400 from Anthropic/OpenAI) or an unanswered tool call.
    pub fn tool_pairs_are_consistent(&self) -> bool {
        let mut index = 0;
        while index < self.messages.len() {
            let uses: Vec<&str> = match &self.messages[index].content {
                MessageContent::ToolUse { id, .. } => vec![id.as_str()],
                MessageContent::Mixed(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };

            if uses.is_empty() {
                index += 1;
                continue;
            }

            let results: Vec<&str> = match self.messages.get(index + 1).map(|m| &m.content) {
                Some(MessageContent::ToolResult { tool_use_id, .. }) => {
                    vec![tool_use_id.as_str()]
                }
                Some(MessageContent::Mixed(blocks)) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };

            if uses.len() != results.len() || !uses.iter().all(|u| results.contains(u)) {
                return false;
            }
            index += 2;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_use(id: &str) -> MessageContent {
        MessageContent::ToolUse {
            id: id.to_string(),
            name: "Bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        }
    }

    #[test]
    fn tool_results_are_grouped_into_one_user_message() {
        let mut session = Session::new();
        session.add_user_message("list files");
        session.add_assistant_message(tool_use("call-1"), 10, 0.0);

        session.add_tool_results(&[
            ("call-1".to_string(), "a.txt".to_string(), false),
            // Answers no pending tool_use, so it must not be recorded.
            ("call-2".to_string(), "boom".to_string(), true),
        ]);

        let last = session.messages.last().unwrap();
        assert_eq!(last.role, Role::User);
        match &last.content {
            MessageContent::Mixed(blocks) => assert_eq!(blocks.len(), 1),
            other => panic!("expected grouped tool results, got {:?}", other),
        }
        assert_eq!(session.messages.len(), 3);
        assert!(session.tool_pairs_are_consistent());
    }

    #[test]
    fn orphaned_tool_use_is_detected() {
        let mut session = Session::new();
        session.add_user_message("list files");
        session.add_assistant_message(tool_use("call-1"), 10, 0.0);
        assert!(!session.tool_pairs_are_consistent());
    }

    #[test]
    fn mismatched_result_ids_are_detected() {
        let mut session = Session::new();
        session.add_user_message("list files");
        session.add_assistant_message(tool_use("call-1"), 10, 0.0);
        session.add_tool_result("call-other", "a.txt", false);
        assert!(!session.tool_pairs_are_consistent());
    }

    #[test]
    fn plain_conversations_are_consistent() {
        let mut session = Session::new();
        session.add_user_message("hi");
        session.add_assistant_message(MessageContent::Text("hello".into()), 5, 0.0);
        assert!(session.tool_pairs_are_consistent());
    }
}
