use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search files using ripgrep."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" }
            },
            "required": ["pattern", "path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let pattern = input["pattern"].as_str().unwrap_or_default();
        let path = input["path"].as_str().unwrap_or_default();
        
        let output = tokio::process::Command::new("rg")
            .arg("-n")
            .arg(pattern)
            .arg(path)
            .output()
            .await?;

        let is_error = !output.status.success() && output.status.code() != Some(1); // 1 = no matches
        let content = String::from_utf8_lossy(if is_error { &output.stderr } else { &output.stdout }).to_string();
        
        Ok(ToolOutput { content, is_error })
    }
}
