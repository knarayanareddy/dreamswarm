use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "FileRead"
    }

    fn description(&self) -> &str {
        "Read contents of a file. Returns the file content. Can optionally specify lines to read."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute or relative path to the file"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let path = input["path"].as_str().unwrap_or_default();
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Ok(ToolOutput {
                content,
                is_error: false,
            }),
            Err(e) => Ok(ToolOutput {
                content: e.to_string(),
                is_error: true,
            }),
        }
    }
}
