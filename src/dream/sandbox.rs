use crate::query::engine::QueryEngine;

pub struct DreamSandbox {
    allowed_operations: Vec<SandboxOperation>,
    max_tokens: u64,
    max_cost_usd: f64,
    pub tokens_used: u64,
    pub cost_used: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SandboxOperation {
    ReadMemory,
    WriteMemory,
    ReadFile,
    SearchMemory,
}

#[derive(Debug, Clone)]
pub struct SandboxRequest {
    pub operation: SandboxOperation,
    pub target: String,
    pub content: Option<String>,
}

impl DreamSandbox {
    pub fn new(max_tokens: u64, max_cost_usd: f64) -> Self {
        Self {
            allowed_operations: vec![
                SandboxOperation::ReadMemory,
                SandboxOperation::WriteMemory,
                SandboxOperation::ReadFile,
                SandboxOperation::SearchMemory,
            ],
            max_tokens,
            max_cost_usd,
            tokens_used: 0,
            cost_used: 0.0,
        }
    }

    pub fn is_allowed(&self, operation: &SandboxOperation) -> bool {
        self.allowed_operations.contains(operation)
    }

    pub fn has_budget(&self) -> bool {
        self.tokens_used < self.max_tokens && self.cost_used < self.max_cost_usd
    }

    pub fn record_usage(&mut self, tokens: u64, cost: f64) {
        self.tokens_used += tokens;
        self.cost_used += cost;
    }

    pub fn validate(&self, request: &SandboxRequest) -> Result<(), String> {
        if !self.is_allowed(&request.operation) {
            return Err(format!(
                "Operation {:?} is not allowed in sandbox",
                request.operation
            ));
        }
        if !self.has_budget() {
            return Err("Sandbox budget exhausted".to_string());
        }
        if request.operation == SandboxOperation::ReadFile {
            let blocked = [".env", "secrets", "credentials", ".ssh", "id_rsa"];
            if blocked.iter().any(|b| request.target.contains(b)) {
                return Err(format!(
                    "Cannot read sensitive file '{}' in sandbox",
                    request.target
                ));
            }
        }
        Ok(())
    }

    pub async fn sandboxed_llm_call(
        &mut self,
        query_engine: &QueryEngine,
        system_prompt: &str,
        user_prompt: &str,
    ) -> anyhow::Result<String> {
        if !self.has_budget() {
            anyhow::bail!("Budget exhausted");
        }
        let messages = vec![serde_json::json!({ "role": "user", "content": user_prompt })];
        let response = query_engine.complete(system_prompt, &messages, &[]).await?;
        self.record_usage(response.usage.total_tokens, response.usage.cost_usd);
        let text = response
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
            .join("\n");
        Ok(text)
    }

    pub fn usage_stats(&self) -> (u64, f64) {
        (self.tokens_used, self.cost_used)
    }
}
