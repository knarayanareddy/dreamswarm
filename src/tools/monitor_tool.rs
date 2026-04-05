use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;

pub struct MonitorTool;

#[async_trait]
impl Tool for MonitorTool {
    fn name(&self) -> &str {
        "Monitor"
    }
    fn description(&self) -> &str {
        "Set up continuous monitoring for a resource. KAIROS-exclusive. \
        The daemon will watch the specified resource and notify or act \
        when conditions are met."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "enum": ["file", "command", "url", "git_branch"], "description": "What to monitor" },
                "value": { "type": "string", "description": "The specific resource (file path, command, URL, or branch)" },
                "condition": { "type": "string", "description": "When to trigger (e.g., 'changes', 'fails', 'contains ERROR')" },
                "action": { "type": "string", "enum": ["notify", "run_tests", "log"], "description": "What to do when triggered" }
            },
            "required": ["target", "value", "condition"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Moderate
    }
    fn command_signature(&self, input: &Value) -> String {
        let target = input.get("target").and_then(|v| v.as_str()).unwrap_or("?");
        format!("monitor:{}", target)
    }
    fn describe_call(&self, input: &Value) -> String {
        let target = input.get("target").and_then(|v| v.as_str()).unwrap_or("?");
        let value = input.get("value").and_then(|v| v.as_str()).unwrap_or("?");
        format!("Monitor {} '{}'", target, value)
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let target = input
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing target"))?;
        let value = input
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing value"))?;
        let condition = input
            .get("condition")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing condition"))?;
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("notify");

        let monitor_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".dreamswarm")
            .join("daemon")
            .join("monitors");
        std::fs::create_dir_all(&monitor_dir)?;
        let monitor_config = serde_json::json!({
            "target": target, "value": value, "condition": condition, "action": action,
            "created_at": chrono::Utc::now().to_rfc3339(), "enabled": true
        });
        let monitor_id = &uuid::Uuid::new_v4().to_string()[..8];
        let path = monitor_dir.join(format!("{}.json", monitor_id));
        std::fs::write(&path, serde_json::to_string_pretty(&monitor_config)?)?;

        Ok(ToolOutput {
            content: format!(
                "Monitor configured:\nID: {}\nTarget: {} '{}'\nCondition: {}\nAction: {}\n",
                monitor_id, target, value, condition, action
            ),
            is_error: false,
        })
    }
}
