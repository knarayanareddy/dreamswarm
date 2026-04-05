use super::{Tool, ToolOutput};
use crate::memory::MemorySystem;
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct GrepTool {
    pub memory: Arc<RwLock<MemorySystem>>,
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "Grep"
    }

    fn description(&self) -> &str {
        "Search files using ripgrep. Setting 'semantic' to true uses relevant indexed files."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string" },
                "path": { "type": "string" },
                "semantic": { "type": "boolean", "default": false }
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
        let semantic = input["semantic"].as_bool().unwrap_or(false);

        let mut cmd = tokio::process::Command::new("rg");
        cmd.arg("-n").arg(pattern);

        if semantic {
            let memory = self.memory.read().await;
            let entries = memory.index.find_relevant(pattern)?;
            if !entries.is_empty() {
                for entry in entries.iter().take(5) {
                    cmd.arg(&entry.file_path);
                }
            } else {
                cmd.arg(path);
            }
        } else {
            cmd.arg(path);
        }

        let output = cmd.output().await?;

        let is_error = !output.status.success() && output.status.code() != Some(1); // 1 = no matches
        let content = String::from_utf8_lossy(if is_error {
            &output.stderr
        } else {
            &output.stdout
        })
        .to_string();

        Ok(ToolOutput { content, is_error })
    }
}
