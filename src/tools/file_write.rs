use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "FileWrite"
    }

    fn description(&self) -> &str {
        "Write content to a file. Overwrites if it exists."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Moderate
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let path = input["path"].as_str().unwrap_or_default();
        let content = input["content"].as_str().unwrap_or_default();
        match tokio::fs::write(path, content).await {
            Ok(_) => Ok(ToolOutput { content: "File written successfully".to_string(), is_error: false }),
            Err(e) => Ok(ToolOutput { content: format!("Failed to write file: {}", e), is_error: true }),
        }
    }
}
