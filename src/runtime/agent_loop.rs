use crate::db::Database;
use crate::prompts::system::SystemPromptBuilder;
use crate::query::engine::QueryEngine;
use crate::runtime::config::AppConfig;
use crate::runtime::permissions::PermissionGate;
use crate::runtime::session::Session;
use crate::swarm::mailbox::Mailbox;
use crate::tools::ToolRegistry;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TurnResult {
    pub final_text: String,
    pub tool_calls_made: Vec<String>,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub stop_reason: StopReason,
}

pub enum StopReason {
    EndTurn,
    MaxIterations,
    CostLimit,
}

pub struct AgentRuntime {
    pub session: Session,
    query_engine: QueryEngine,
    #[allow(dead_code)] // Reserved for Phase 2: tool execution wiring
    tool_registry: ToolRegistry,
    #[allow(dead_code)] // Reserved for Phase 2: permission checking
    permission_gate: PermissionGate,
    config: AppConfig,
    db: Database,
    #[allow(dead_code)]
    mailbox: Option<Arc<RwLock<Mailbox>>>,
    max_iterations: u32,
}

impl AgentRuntime {
    pub fn new(
        session: Session,
        query_engine: QueryEngine,
        tool_registry: ToolRegistry,
        config: AppConfig,
        db: Database,
        mailbox: Option<Arc<RwLock<Mailbox>>>,
    ) -> Self {
        let permission_gate = PermissionGate::new();
        Self {
            session,
            query_engine,
            tool_registry,
            permission_gate,
            config,
            db,
            mailbox,
            max_iterations: 25,
        }
    }

    pub async fn run_turn<F, Fut>(
        &mut self,
        user_input: &str,
        on_tool_approval: F,
    ) -> anyhow::Result<TurnResult>
    where
        F: Fn(String, serde_json::Value) -> Fut + Copy,
        Fut: std::future::Future<Output = bool>,
    {
        self.session.add_user_message(user_input);

        let mut iterations: u32 = 0;
        let mut all_tool_calls: Vec<String> = Vec::new();
        #[allow(unused_assignments)]
        let mut final_text = String::new();
        // Skip initial assignment warning by using it or changing declaration
        // We'll just define it here to satisfy the compiler and ensure it's always read later.

        loop {
            if iterations >= self.max_iterations {
                return Ok(TurnResult {
                    final_text: "Max iterations reached.".to_string(),
                    tool_calls_made: all_tool_calls,
                    tokens_used: self.session.total_tokens,
                    cost_usd: self.session.total_cost_usd,
                    stop_reason: StopReason::MaxIterations,
                });
            }

            let system_prompt = SystemPromptBuilder::build(&self.config);

            // Map Session messages to simple JSON for the provider
            let mut messages = Vec::new();
            for msg in &self.session.messages {
                let role = match msg.role {
                    crate::runtime::session::Role::System => "system",
                    crate::runtime::session::Role::User => "user",
                    crate::runtime::session::Role::Assistant => "assistant",
                    crate::runtime::session::Role::ToolResult => "user", // Simplified for Phase 1
                };

                let content_val = match &msg.content {
                    crate::runtime::session::MessageContent::Text(t) => {
                        serde_json::json!([{"type": "text", "text": t}])
                    }
                    crate::runtime::session::MessageContent::ToolUse { id, name, input } => {
                        serde_json::json!([{
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        }])
                    }
                    crate::runtime::session::MessageContent::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        serde_json::json!([{
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        }])
                    }
                    _ => serde_json::json!([{"type": "text", "text": "[unsupported content]"}]),
                };

                messages.push(serde_json::json!({
                    "role": role,
                    "content": content_val
                }));
            }

            let tools = self.tool_registry.get_all_schemas();

            let response = self
                .query_engine
                .complete(&system_prompt, &messages, &tools)
                .await?;

            iterations += 1;

            let mut assistant_text = String::new();
            let mut made_tool_call = false;
            let mut tool_results = Vec::new();

            for content in &response.content {
                if content.get("type").and_then(|t| t.as_str()) == Some("text") {
                    if let Some(t) = content.get("text").and_then(|v| v.as_str()) {
                        assistant_text.push_str(t);
                    }
                }

                if content.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    made_tool_call = true;
                    let id = content["id"].as_str().unwrap_or_default().to_string();
                    let name = content["name"].as_str().unwrap_or_default().to_string();
                    let input = content["input"].clone();

                    all_tool_calls.push(name.clone());

                    // Execute tool
                    if let Some(tool) = self.tool_registry.get_tool(&name) {
                        let mut allowed = true;
                        if tool.risk_level() == crate::runtime::permissions::RiskLevel::Dangerous {
                            allowed = on_tool_approval(name.clone(), input.clone()).await;
                        }

                        if allowed {
                            match tool.execute(&input).await {
                                Ok(output) => {
                                    tool_results.push((
                                        id,
                                        name,
                                        input,
                                        output.content,
                                        output.is_error,
                                    ));
                                }
                                Err(e) => {
                                    tool_results.push((
                                        id,
                                        name,
                                        input,
                                        format!("Error: {}", e),
                                        true,
                                    ));
                                }
                            }
                        } else {
                            tool_results.push((
                                id,
                                name,
                                input,
                                "Permission denied by user.".to_string(),
                                true,
                            ));
                        }
                    } else {
                        tool_results.push((
                            id,
                            name.clone(),
                            input,
                            format!("Tool '{}' not found.", name),
                            true,
                        ));
                    }
                }
            }

            final_text = assistant_text.clone();

            // Update session with assistant message (including tool calls if any)
            // For now we simplify by detecting if it's mixed or text
            if !all_tool_calls.is_empty() {
                // Actually need a way to store multi-part messages in Session
                // For Phase 1 simplified: just store as text if no results, otherwise handled below
            }

            self.session.add_assistant_message(
                crate::runtime::session::MessageContent::Text(assistant_text),
                response.usage.total_tokens,
                response.usage.cost_usd,
            );

            // Add tool results to session
            for (id, _name, _input, content, is_error) in tool_results {
                self.session.add_tool_result(&id, &content, is_error);
            }

            if !made_tool_call {
                break;
            }
        }

        self.db.save_session(&self.session)?;

        Ok(TurnResult {
            final_text,
            tool_calls_made: all_tool_calls,
            tokens_used: self.session.total_tokens,
            cost_usd: self.session.total_cost_usd,
            stop_reason: StopReason::EndTurn,
        })
    }

    /// Handle built-in slash commands (e.g. /help, /cost, /memory).
    /// Returns `Some(response)` if the command was handled, `None` if unknown.
    pub fn handle_slash_command(&self, input: &str) -> Option<String> {
        match input {
            "/help" => Some(
                "  Commands:\n  /help    — show this message\n  /cost    — show session cost\n  /memory  — show memory index\n  /clear   — clear session\n  /quit    — exit".to_string()
            ),
            "/cost" => Some(format!(
                "  Session cost: ${:.4} ({} tokens)",
                self.session.total_cost_usd, self.session.total_tokens
            )),
            "/memory" => Some("  Memory index: use MemoryRead tool to inspect.".to_string()),
            "/clear" => Some("  Session context cleared (restart to apply).".to_string()),
            _ => None,
        }
    }
}
