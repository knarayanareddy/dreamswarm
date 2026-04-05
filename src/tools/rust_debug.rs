use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;
use tokio::process::Command;

pub struct RustCheckTool;

#[async_trait]
impl Tool for RustCheckTool {
    fn name(&self) -> &str { "RustCheck" }
    fn description(&self) -> &str { "Run cargo check with JSON output and return structured error analysis for faster debugging." }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }
    fn risk_level(&self) -> RiskLevel { RiskLevel::Safe }
    async fn execute(&self, _input: &Value) -> anyhow::Result<ToolOutput> {
        let output = Command::new("cargo")
            .args(["check", "--message-format=json"])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut error_summary = String::new();
        
        for line in stdout.lines() {
            if let Ok(msg) = serde_json::from_str::<Value>(line) {
                if msg["reason"] == "compiler-message" && msg["message"]["level"] == "error" {
                    let text = msg["message"]["rendered"].as_str().unwrap_or("Unknown compiler error");
                    error_summary.push_str(&format!("---\n{}\n", text));
                }
            }
        }

        if error_summary.is_empty() {
            Ok(ToolOutput {
                content: "All check passed. No compilation errors found.".to_string(),
                is_error: false,
            })
        } else {
            Ok(ToolOutput {
                content: format!("Build Failed with the following structured errors:\n\n{}", error_summary),
                is_error: true,
            })
        }
    }
}
