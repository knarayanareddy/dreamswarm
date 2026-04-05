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

    pub fn add_tool_result(&mut self, tool_use_id: &str, content: &str, is_error: bool) {
        self.messages.push(Message {
            role: Role::ToolResult,
            content: MessageContent::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error,
            },
            timestamp: Utc::now(),
            token_count: None,
            turn_number: self.turn_count,
        });
        self.updated_at = Utc::now();
    }
}
