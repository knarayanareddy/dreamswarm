use super::path_guard::{is_sensitive_path, PathGuard};
use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

/// Reads files confined to the session workspace.
///
/// The tool used to accept any absolute path and was declared `Safe`, which let
/// a model read `~/.ssh/id_rsa` (or the daemon config holding API keys) without
/// a permission prompt. It now resolves every path through [`PathGuard`] and
/// refuses credential-bearing files outright.
pub struct FileReadTool {
    guard: PathGuard,
}

impl FileReadTool {
    pub fn new(guard: PathGuard) -> Self {
        Self { guard }
    }

    fn error(message: impl Into<String>) -> ToolOutput {
        ToolOutput {
            content: message.into(),
            is_error: true,
        }
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "FileRead"
    }

    fn description(&self) -> &str {
        "Read contents of a file inside the workspace. Returns the file content."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file, relative to the workspace root"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Safe
    }

    async fn execute(&self, input: &Value) -> anyhow::Result<ToolOutput> {
        let requested = match input["path"].as_str() {
            Some(p) => p,
            None => return Ok(Self::error("Missing required argument: path")),
        };

        let path = match self.guard.resolve_existing(requested) {
            Ok(p) => p,
            Err(e) => return Ok(Self::error(format!("Access denied: {}", e))),
        };

        if is_sensitive_path(&path) {
            return Ok(Self::error(format!(
                "Access denied: '{}' is a protected credential path",
                requested
            )));
        }

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => Ok(ToolOutput {
                content,
                is_error: false,
            }),
            Err(e) => Ok(Self::error(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn tool(dir: &std::path::Path) -> FileReadTool {
        FileReadTool::new(PathGuard::new(dir).unwrap())
    }

    #[tokio::test]
    async fn reads_file_inside_workspace() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "hello world").unwrap();
        let out = tool(dir.path())
            .execute(&serde_json::json!({"path": "hello.txt"}))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(out.content, "hello world");
    }

    #[tokio::test]
    async fn refuses_traversal_and_absolute_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        let tool = tool(&root);

        for path in ["../secret.txt", "/etc/hostname"] {
            let out = tool
                .execute(&serde_json::json!({ "path": path }))
                .await
                .unwrap();
            assert!(out.is_error, "path {path} should have been rejected");
            assert!(out.content.contains("Access denied"));
        }
    }

    #[tokio::test]
    async fn refuses_environment_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".env"), "SECRET=1").unwrap();
        let out = tool(dir.path())
            .execute(&serde_json::json!({"path": ".env"}))
            .await
            .unwrap();
        assert!(out.is_error);
    }

    #[tokio::test]
    async fn missing_path_argument_is_an_error() {
        let dir = tempdir().unwrap();
        let out = tool(dir.path()).execute(&serde_json::json!({})).await.unwrap();
        assert!(out.is_error);
    }
}
