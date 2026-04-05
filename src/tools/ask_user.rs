use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

pub struct AskUserTool;

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "AskUser"
    }

    fn description(&self) -> &str {
        "Ask the user a question to clarify requirements."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": { "type": "string" }
            },
            "required": ["question"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let question = input["question"].as_str().unwrap_or_default();
        println!("Agent asks: {}", question);
        // Note: Full CLI app would read from stdin here.
        Ok(ToolOutput {
            content: "User prompt submitted.".to_string(),
            is_error: false,
        })
    }
}
