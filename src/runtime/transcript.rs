//! Conversion of a [`Session`] into provider wire messages.
//!
//! Providers reject a conversation where a `tool_result` has no matching
//! `tool_use` (or vice versa). Sessions can contain such pairs when they were
//! written by an older build or resumed from the database, so this module
//! *repairs* the transcript before it is sent:
//!
//! * `tool_result` blocks without a preceding `tool_use` are dropped;
//! * `tool_use` blocks without a result get a synthetic error result, so the
//!   model is told the call never completed instead of seeing a dangling call.
//!
//! Repairs are logged; a healthy session is passed through unchanged.

use crate::runtime::session::{ContentBlock, MessageContent, Role, Session};
use serde_json::{json, Value};

/// A message in provider-neutral shape: a role plus ordered content blocks.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedMessage {
    pub role: &'static str,
    pub blocks: Vec<Value>,
}

impl NormalizedMessage {
    pub fn to_value(&self) -> Value {
        json!({ "role": self.role, "content": self.blocks })
    }

    fn tool_use_ids(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter(|b| b["type"] == "tool_use")
            .filter_map(|b| b["id"].as_str().map(str::to_string))
            .collect()
    }

    fn tool_result_ids(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter(|b| b["type"] == "tool_result")
            .filter_map(|b| b["tool_use_id"].as_str().map(str::to_string))
            .collect()
    }
}

fn text_block(text: &str) -> Value {
    json!({ "type": "text", "text": text })
}

fn tool_use_block(id: &str, name: &str, input: &Value) -> Value {
    json!({ "type": "tool_use", "id": id, "name": name, "input": input })
}

fn tool_result_block(id: &str, content: &str, is_error: bool) -> Value {
    json!({
        "type": "tool_result",
        "tool_use_id": id,
        "content": content,
        "is_error": is_error,
    })
}

fn block_to_value(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text(t) => text_block(t),
        ContentBlock::ToolUse { id, name, input } => tool_use_block(id, name, input),
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => tool_result_block(tool_use_id, content, *is_error),
    }
}

fn message_to_normalized(role: Role, content: &MessageContent) -> Option<NormalizedMessage> {
    let role = match role {
        Role::System => "system",
        Role::User | Role::ToolResult => "user",
        Role::Assistant => "assistant",
    };

    let blocks = match content {
        MessageContent::Text(text) => {
            if text.is_empty() {
                Vec::new()
            } else {
                vec![text_block(text)]
            }
        }
        MessageContent::ToolUse { id, name, input } => vec![tool_use_block(id, name, input)],
        MessageContent::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => vec![tool_result_block(tool_use_id, content, *is_error)],
        MessageContent::Mixed(blocks) => blocks.iter().map(block_to_value).collect(),
    };

    if blocks.is_empty() {
        None
    } else {
        Some(NormalizedMessage { role, blocks })
    }
}

/// Converts and repairs a session for the provider.
pub fn to_provider_messages(session: &Session) -> Vec<Value> {
    let mut normalized: Vec<NormalizedMessage> = Vec::new();

    for message in &session.messages {
        let Some(mut current) = message_to_normalized(message.role.clone(), &message.content)
        else {
            continue;
        };

        // 1. Drop tool_results that do not answer the immediately preceding
        //    assistant tool_use. The API cannot associate them, and resending
        //    them is what produced 400s on resumed sessions.
        let result_ids = current.tool_result_ids();
        if !result_ids.is_empty() {
            let expected = normalized
                .last()
                .map(|m| m.tool_use_ids())
                .unwrap_or_default();
            let usable: Vec<String> = result_ids
                .iter()
                .filter(|id| expected.contains(id))
                .cloned()
                .collect();
            if usable.len() != result_ids.len() {
                tracing::warn!(
                    "Dropping {} orphaned tool_result block(s) from the transcript",
                    result_ids.len() - usable.len()
                );
                current.blocks.retain(|b| {
                    b["type"] != "tool_result"
                        || b["tool_use_id"]
                            .as_str()
                            .is_some_and(|id| usable.iter().any(|u| u == id))
                });
                if current.blocks.is_empty() {
                    continue;
                }
            }
        }

        // 2. Answer any tool_use from the previous assistant turn that this
        //    message does not already answer.
        let answered = current.tool_result_ids();
        let pending: Vec<String> = normalized
            .last()
            .filter(|last| last.role == "assistant")
            .map(|last| {
                last.tool_use_ids()
                    .into_iter()
                    .filter(|id| !answered.iter().any(|a| a == id))
                    .collect()
            })
            .unwrap_or_default();

        if !pending.is_empty() {
            tracing::warn!(
                "Adding {} synthetic tool_result(s): the recorded conversation was incomplete",
                pending.len()
            );
            let synthetic: Vec<Value> = pending
                .iter()
                .map(|id| {
                    tool_result_block(
                        id,
                        "Tool execution was interrupted before a result was recorded.",
                        true,
                    )
                })
                .collect();
            if current.role == "user" && !current.tool_result_ids().is_empty() {
                // Keep the pair adjacent by answering in the same message.
                current.blocks.splice(0..0, synthetic);
            } else {
                normalized.push(NormalizedMessage {
                    role: "user",
                    blocks: synthetic,
                });
            }
        }

        // 3. Merge consecutive user messages so role alternation stays valid.
        if current.role == "user" && normalized.last().is_some_and(|last| last.role == "user") {
            if let Some(last) = normalized.last_mut() {
                last.blocks.append(&mut current.blocks);
            }
            continue;
        }

        normalized.push(current);
    }

    // A trailing assistant tool_use still needs a result.
    let pending = missing_results(&normalized);
    if !pending.is_empty() {
        normalized.push(NormalizedMessage {
            role: "user",
            blocks: pending
                .iter()
                .map(|id| {
                    tool_result_block(
                        id,
                        "Tool execution was interrupted before a result was recorded.",
                        true,
                    )
                })
                .collect(),
        });
    }

    normalized.iter().map(NormalizedMessage::to_value).collect()
}

/// Returns tool_use ids in the last message that have no result yet.
fn missing_results(messages: &[NormalizedMessage]) -> Vec<String> {
    let Some(last) = messages.last() else {
        return Vec::new();
    };
    if last.role != "assistant" {
        return Vec::new();
    }
    last.tool_use_ids()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::session::MessageContent;

    fn session_with_tools() -> Session {
        let mut session = Session::new();
        session.add_user_message("list files");
        session.add_assistant_message(
            MessageContent::Mixed(vec![
                ContentBlock::Text("I will list them.".into()),
                ContentBlock::ToolUse {
                    id: "call-1".into(),
                    name: "Bash".into(),
                    input: json!({"command": "ls"}),
                },
            ]),
            10,
            0.0,
        );
        session.add_tool_results(&[("call-1".to_string(), "a.txt".to_string(), false)]);
        session
    }

    #[test]
    fn mixed_assistant_content_keeps_text_and_tool_use() {
        let messages = to_provider_messages(&session_with_tools());
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["type"], "text");
        assert_eq!(messages[1]["content"][1]["type"], "tool_use");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    }

    #[test]
    fn orphaned_tool_result_is_dropped() {
        let mut session = Session::new();
        session.add_user_message("hello");
        session.add_tool_result("call-missing", "stale output", false);

        let messages = to_provider_messages(&session);
        assert_eq!(messages.len(), 1, "the orphan must not be sent");
        assert_eq!(messages[0]["content"][0]["text"], "hello");
    }

    #[test]
    fn unanswered_tool_use_gets_a_synthetic_result() {
        let mut session = Session::new();
        session.add_user_message("list files");
        session.add_assistant_message(
            MessageContent::ToolUse {
                id: "call-1".into(),
                name: "Bash".into(),
                input: json!({"command": "ls"}),
            },
            10,
            0.0,
        );

        let messages = to_provider_messages(&session);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "call-1");
        assert_eq!(messages[2]["content"][0]["is_error"], true);
    }

    #[test]
    fn multiple_results_stay_in_one_user_message() {
        let mut session = Session::new();
        session.add_user_message("two calls");
        session.add_assistant_message(
            MessageContent::Mixed(vec![
                ContentBlock::ToolUse {
                    id: "a".into(),
                    name: "Bash".into(),
                    input: json!({}),
                },
                ContentBlock::ToolUse {
                    id: "b".into(),
                    name: "Bash".into(),
                    input: json!({}),
                },
            ]),
            10,
            0.0,
        );
        session.add_tool_results(&[
            ("a".to_string(), "one".to_string(), false),
            ("b".to_string(), "two".to_string(), true),
        ]);

        let messages = to_provider_messages(&session);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[2]["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn empty_messages_are_skipped() {
        let mut session = Session::new();
        session.add_user_message("hi");
        session.add_assistant_message(MessageContent::Text(String::new()), 0, 0.0);
        let messages = to_provider_messages(&session);
        assert_eq!(messages.len(), 1);
    }
}
