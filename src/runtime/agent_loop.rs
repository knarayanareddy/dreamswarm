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
    mailbox: Option<Arc<RwLock<Mailbox>>>,
    max_iterations: u32,
    /// Approval boundary for unattended workers. `None` means every `Ask`
    /// decision is denied; workers must not auto-approve dangerous tools.
    auto_approver: Option<AutoApprover>,
}

/// Decides whether an unattended worker may run an `Ask`-level tool without a
/// human present. Returning `false` turns the call into an error tool result.
pub type AutoApprover =
    Arc<dyn Fn(&str, &serde_json::Value) -> anyhow::Result<bool> + Send + Sync>;

impl AgentRuntime {
    pub fn new(
        session: Session,
        query_engine: QueryEngine,
        tool_registry: ToolRegistry,
        config: AppConfig,
        db: Database,
        mailbox: Option<Arc<RwLock<Mailbox>>>,
    ) -> Self {
        let permission_gate = PermissionGate::new(
            config.permission_mode,
            &config.allow_patterns,
            &config.deny_patterns,
        );
        Self {
            session,
            query_engine,
            tool_registry,
            permission_gate,
            config,
            db,
            mailbox,
            max_iterations: 25,
            auto_approver: None,
        }
    }

    /// Sets the approval boundary used for worker sessions (`run_worker_turn`).
    ///
    /// Without an approver, unattended sessions deny every `Ask` decision.
    pub fn with_auto_approver(mut self, approver: Option<AutoApprover>) -> Self {
        self.auto_approver = approver;
        self
    }

    /// Runs a turn without a human at the keyboard.
    ///
    /// `Ask` decisions are resolved by the configured [`AutoApprover`]; when no
    /// approver is set they are denied. This replaces the previous blanket
    /// `true` closure, which let a worker execute unsandboxed Python and
    /// JavaScript without any review.
    pub async fn run_worker_turn(&mut self, user_input: &str) -> anyhow::Result<TurnResult> {
        let approver = self.auto_approver.clone();
        let mailbox = self.mailbox.clone();
        let mut on_tool_approval = move |name: String, input: serde_json::Value| {
            let approver = approver.clone();
            let mailbox = mailbox.clone();
            async move {
                match approver {
                    Some(check) => match check(&name, &input) {
                        Ok(approved) => approved,
                        Err(e) => {
                            tracing::warn!("Approval channel failed for '{}': {}", name, e);
                            false
                        }
                    },
                    None => {
                        // Fall back to asking the lead directly (and denying
                        // if that is impossible) rather than auto-approving.
                        match mailbox {
                            Some(mbox) => crate::swarm::approval::enqueue_approval_request(
                                mbox, &name, &input,
                            )
                            .await
                            .unwrap_or_else(|e| {
                                tracing::warn!("Could not enqueue approval for '{}': {}", name, e);
                                false
                            }),
                            None => {
                                tracing::warn!(
                                    "Denying '{}': no worker approval policy is configured",
                                    name
                                );
                                false
                            }
                        }
                    }
                }
            }
        };
        self.run_turn(user_input, &mut on_tool_approval).await
    }

    pub async fn run_turn<F, Fut>(
        &mut self,
        user_input: &str,
        on_tool_approval: F,
    ) -> anyhow::Result<TurnResult>
    where
        F: FnMut(String, serde_json::Value) -> Fut,
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

            let system_prompt = SystemPromptBuilder::build(&self.config, None);

            // Build provider messages from the persisted transcript. This
            // also repairs legacy sessions (orphaned results) and matches
            // consecutive tool results into a single user message.
            let messages = crate::runtime::transcript::to_provider_messages(&self.session);

            let tools = self.tool_registry.get_all_schemas();

            let response = self
                .query_engine
                .complete(&system_prompt, &messages, &tools)
                .await?;

            iterations += 1;

            let mut assistant_text = String::new();
            let mut made_tool_call = false;
            // Tool calls must be persisted *with* their results, otherwise the
            // next request contains a tool_result with no matching tool_use.
            let mut assistant_blocks: Vec<crate::runtime::session::ContentBlock> = Vec::new();
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
                    assistant_blocks.push(crate::runtime::session::ContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });

                    // Execute tool
                    if let Some(tool) = self.tool_registry.get_tool(&name) {
                        let signature = tool.command_signature(&input);
                        let permission =
                            self.permission_gate
                                .check(&name, tool.risk_level(), &signature);

                        let mut final_allowed = false;
                        let mut error_content: Option<String> = None;

                        match permission {
                            crate::runtime::permissions::Permission::Allow => {
                                final_allowed = true;
                            }
                            crate::runtime::permissions::Permission::Deny(reason) => {
                                error_content = Some(format!("Security blocked: {}", reason));
                            }
                            crate::runtime::permissions::Permission::Ask => {
                                final_allowed = on_tool_approval(name.clone(), input.clone()).await;
                                if !final_allowed {
                                    error_content = Some("Permission denied by user.".to_string());
                                }
                            }
                        }

                        if final_allowed {
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
                                error_content.unwrap_or_else(|| "Execution blocked.".into()),
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

            // Persist the assistant turn exactly as the provider produced it.
            // Dropping the tool_use blocks left the following tool_results
            // orphaned, which is an invalid conversation on the next request.
            let assistant_content = if assistant_blocks.is_empty() {
                crate::runtime::session::MessageContent::Text(assistant_text)
            } else {
                let mut blocks = Vec::with_capacity(assistant_blocks.len() + 1);
                if !assistant_text.is_empty() {
                    blocks.push(crate::runtime::session::ContentBlock::Text(assistant_text));
                }
                blocks.extend(assistant_blocks);
                crate::runtime::session::MessageContent::Mixed(blocks)
            };
            self.session.add_assistant_message(
                assistant_content,
                response.usage.total_tokens,
                response.usage.cost_usd,
            );

            // One user message carrying every tool_result for that call.
            let results: Vec<(String, String, bool)> = tool_results
                .into_iter()
                .map(|(id, _name, _input, content, is_error)| (id, content, is_error))
                .collect();
            self.session.add_tool_results(&results);

            if !made_tool_call {
                break;
            }
        }

        debug_assert!(
            self.session.tool_pairs_are_consistent(),
            "session contains orphaned tool_use/tool_result blocks"
        );
        if !self.session.tool_pairs_are_consistent() {
            anyhow::bail!(
                "refusing to send an inconsistent tool conversation: a tool_result is missing its tool_use"
            );
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
