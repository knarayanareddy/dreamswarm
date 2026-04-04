use crate::query::engine::QueryEngine;
use serde_json::Value;

pub struct Subagent {
    task_description: String,
    model_override: Option<String>,
    max_turns: u32,
    tool_restrictions: Option<Vec<String>>,
}

#[derive(Debug)]
pub struct SubagentResult {
    pub summary: String,
    pub tool_calls_made: Vec<String>,
    pub turns_taken: u32,
    pub tokens_used: u64,
    pub cost_usd: f64,
    pub success: bool,
}

impl Subagent {
    pub fn new(task_description: &str) -> Self {
        Self {
            task_description: task_description.to_string(),
            model_override: None,
            max_turns: 10,
            tool_restrictions: None,
        }
    }

    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = turns;
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model_override = Some(model.to_string());
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tool_restrictions = Some(tools);
        self
    }

    pub async fn execute(
        &self,
        query_engine: &QueryEngine,
        tools: &[Value],
        working_dir: &str,
    ) -> anyhow::Result<SubagentResult> {
        let system_prompt = format!(
            r#"You are a focused subagent executing a specific task. Your task: {}
RULES:
- Stay focused on the task. Do not deviate.
- Complete the task as efficiently as possible.
- When done, output a clear summary of what you did and the result.
- If you cannot complete the task, explain why.
- You have at most {} turns to complete this task.
- Working directory: {}
Begin executing the task now."#,
            self.task_description, self.max_turns, working_dir
        );

        let active_tools: Vec<Value> = if let Some(ref allowed) = self.tool_restrictions {
            tools.iter().filter(|t| {
                t.get("name").and_then(|n| n.as_str()).map(|name| allowed.contains(&name.to_string())).unwrap_or(false)
            }).cloned().collect()
        } else {
            tools.to_vec()
        };

        let mut messages: Vec<Value> = vec![serde_json::json!({
            "role": "user",
            "content": format!("Execute this task: {}", self.task_description)
        })];

        let mut total_tokens: u64 = 0;
        let mut total_cost: f64 = 0.0;
        let mut all_tool_calls: Vec<String> = Vec::new();
        let mut final_text = String::new();

        for _turn in 0..self.max_turns {
            let response = query_engine.complete(&system_prompt, &messages, &active_tools).await?;
            total_tokens += response.usage.total_tokens;
            total_cost += response.usage.cost_usd;

            let mut has_tool_calls = false;
            let mut turn_text = String::new();

            for block in &response.content {
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            turn_text.push_str(text);
                        }
                    }
                    Some("tool_use") => {
                        has_tool_calls = true;
                        let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                        all_tool_calls.push(name.to_string());
                    }
                    _ => {}
                }
            }

            if !has_tool_calls {
                final_text = turn_text;
                break;
            }

            messages.push(serde_json::json!({ "role": "assistant", "content": response.content }));
            for block in &response.content {
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let tool_id = block.get("id").and_then(|i| i.as_str()).unwrap_or("?");
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_id,
                            "content": "[Subagent tool execution - handled by parent runtime]"
                        }]
                    }));
                }
            }
            if !turn_text.is_empty() {
                final_text = turn_text;
            }
        }

        Ok(SubagentResult {
            summary: final_text,
            tool_calls_made: all_tool_calls,
            turns_taken: self.max_turns.min(messages.len() as u32 / 2),
            tokens_used: total_tokens,
            cost_usd: total_cost,
            success: true,
        })
    }
}
