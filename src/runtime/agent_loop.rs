use crate::db::Database;
use crate::prompts::system::SystemPromptBuilder;
use crate::query::engine::QueryEngine;
use crate::runtime::config::AppConfig;
use crate::runtime::permissions::PermissionGate;
use crate::runtime::session::Session;
use crate::tools::ToolRegistry;

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
    tool_registry: ToolRegistry,
    permission_gate: PermissionGate,
    config: AppConfig,
    db: Database,
    max_iterations: u32,
}

impl AgentRuntime {
    pub fn new(
        session: Session,
        query_engine: QueryEngine,
        tool_registry: ToolRegistry,
        config: AppConfig,
        db: Database,
    ) -> Self {
        let permission_gate = PermissionGate::new();
        Self {
            session,
            query_engine,
            tool_registry,
            permission_gate,
            config,
            db,
            max_iterations: 25,
        }
    }

    pub async fn run_turn(&mut self, user_input: &str) -> anyhow::Result<TurnResult> {
        self.session.add_user_message(user_input);

        let mut iterations: u32 = 0;
        let all_tool_calls: Vec<String> = Vec::new();
        let mut final_text = String::new();

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
            let messages = Vec::new(); // Mapped from self.session.messages mapped to Provider API
            let tools = Vec::new(); // Mapped from self.tool_registry

            let response = self
                .query_engine
                .complete(&system_prompt, &messages, &tools)
                .await?;

            iterations += 1;
            
            // Check for tool calls
            let mut made_tool_call = false;
            let mut assistant_text = String::new();
            
            for content in &response.content {
                // Here we would parse and execute the tool calls
                if content.get("type").and_then(|t| t.as_str()) == Some("text") {
                    assistant_text.push_str(content["text"].as_str().unwrap_or(""));
                }
                
                if content.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    made_tool_call = true;
                    // let tool_name = content["name"].as_str().unwrap_or("");
                    // let input = &content["input"];
                    // ... Execute tools by looking up in registry ...
                    // self.permission_gate.check(...)
                }
            }

            final_text = assistant_text;

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
