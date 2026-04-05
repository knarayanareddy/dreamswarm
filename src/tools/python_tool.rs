use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

pub struct PythonExecuteTool;

#[async_trait]
impl Tool for PythonExecuteTool {
    fn name(&self) -> &str {
        "PythonExecute"
    }
    fn description(&self) -> &str {
        "Execute a Python script or expression and return the output."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "The Python code to execute" }
            },
            "required": ["code"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Dangerous
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let code = input["code"].as_str().unwrap_or_default();
        let output = Command::new("python3").args(["-c", code]).output().await?;

        Ok(ToolOutput {
            content: String::from_utf8_lossy(&output.stdout).to_string()
                + &String::from_utf8_lossy(&output.stderr),
            is_error: !output.status.success(),
        })
    }
}
