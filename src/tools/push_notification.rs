use crate::runtime::permissions::RiskLevel;
use crate::tools::{Tool, ToolOutput};
use async_trait::async_trait;
use serde_json::Value;

pub struct PushNotificationTool;

#[async_trait]
impl Tool for PushNotificationTool {
    fn name(&self) -> &str {
        "PushNotification"
    }
    fn description(&self) -> &str {
        "Send a push notification to the user's desktop. KAIROS-exclusive tool. \
        Use sparingly — only for important events that need the user's attention."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Notification title" },
                "message": { "type": "string", "description": "Notification body" },
                "urgency": {
                    "type": "string",
                    "enum": ["low", "medium", "high", "critical"],
                    "description": "Urgency level"
                }
            },
            "required": ["title", "message"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    fn command_signature(&self, _: &Value) -> String {
        "notify".into()
    }
    fn describe_call(&self, input: &Value) -> String {
        let msg = input.get("message").and_then(|v| v.as_str()).unwrap_or("?");
        format!("Send notification: {}", &msg[..msg.len().min(60)])
    }
    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let title = input
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("DreamSwarm");
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing message"))?;

        #[cfg(target_os = "macos")]
        {
            let _ = tokio::process::Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "display notification \"{}\" with title \"{}\"",
                        message.replace('"', "\\\""),
                        title.replace('"', "\\\"")
                    ),
                ])
                .output()
                .await;
        }
        #[cfg(target_os = "linux")]
        {
            let _ = tokio::process::Command::new("notify-send")
                .args([title, message])
                .output()
                .await;
        }

        Ok(ToolOutput {
            content: format!("Notification sent: {} - {}", title, message),
            is_error: false,
        })
    }
}
