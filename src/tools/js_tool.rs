use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

pub struct JSExecuteTool;

#[async_trait]
impl Tool for JSExecuteTool {
    fn name(&self) -> &str { "JSExecute" }
    fn description(&self) -> &str { "Execute Javascript and return the output via Node.js." }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "script": { "type": "string", "description": "The Javascript to execute" }
            },
            "required": ["script"]
        })
    }
    fn risk_level(&self) -> RiskLevel { RiskLevel::Dangerous }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let script = input["script"].as_str().unwrap_or_default();
        let output = Command::new("node").args(["-e", script]).output().await?;

        Ok(ToolOutput {
            content: String::from_utf8_lossy(&output.stdout).to_string() + &String::from_utf8_lossy(&output.stderr),
            is_error: !output.status.success(),
        })
    }
}
