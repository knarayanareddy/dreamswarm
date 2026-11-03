use super::path_guard::{is_sensitive_path, PathGuard};
use super::{Tool, ToolOutput};
use crate::runtime::permissions::RiskLevel;
use async_trait::async_trait;
use serde_json::Value;

/// Writes files confined to the session workspace.
///
/// Previously any absolute path was accepted and a missing `content` argument
/// silently truncated the target file to zero bytes.
pub struct FileWriteTool {
    guard: PathGuard,
}

impl FileWriteTool {
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
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "FileWrite"
    }

    fn description(&self) -> &str {
        "Write content to a file inside the workspace. Overwrites if it exists."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file, relative to the workspace root"
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
        let requested = match input["path"].as_str() {
            Some(p) => p,
            None => return Ok(Self::error("Missing required argument: path")),
        };
        // A missing `content` used to fall back to "" and erase the file.
        let content = match input["content"].as_str() {
            Some(c) => c,
            None => return Ok(Self::error("Missing required argument: content")),
        };

        let path = match self.guard.resolve_for_write(requested) {
            Ok(p) => p,
            Err(e) => return Ok(Self::error(format!("Access denied: {}", e))),
        };

        if is_sensitive_path(&path) {
            return Ok(Self::error(format!(
                "Access denied: '{}' is a protected credential path",
                requested
            )));
        }

        if let Some(parent) = path.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return Ok(Self::error(format!("Failed to create parent directory: {}", e)));
            }
        }

        match tokio::fs::write(&path, content).await {
            Ok(_) => Ok(ToolOutput {
                content: "File written successfully".to_string(),
                is_error: false,
            }),
            Err(e) => Ok(Self::error(format!("Failed to write file: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn tool(dir: &std::path::Path) -> FileWriteTool {
        FileWriteTool::new(PathGuard::new(dir).unwrap())
    }

    #[tokio::test]
    async fn writes_inside_workspace() {
        let dir = tempdir().unwrap();
        let out = tool(dir.path())
            .execute(&serde_json::json!({"path": "nested/a.txt", "content": "hi"}))
            .await
            .unwrap();
        assert!(!out.is_error);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("nested/a.txt")).unwrap(),
            "hi"
        );
    }

    #[tokio::test]
    async fn refuses_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        let out = tool(&root)
            .execute(&serde_json::json!({"path": "../escape.txt", "content": "x"}))
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(!dir.path().join("escape.txt").exists());
    }

    #[tokio::test]
    async fn missing_content_does_not_truncate() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), "important").unwrap();
        let out = tool(dir.path())
            .execute(&serde_json::json!({"path": "keep.txt"}))
            .await
            .unwrap();
        assert!(out.is_error);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("keep.txt")).unwrap(),
            "important"
        );
    }

    #[tokio::test]
    async fn refuses_deny_listed_paths() {
        let dir = tempdir().unwrap();
        let out = tool(dir.path())
            .execute(&serde_json::json!({"path": ".env", "content": "X=1"}))
            .await
            .unwrap();
        assert!(out.is_error);
        assert!(!dir.path().join(".env").exists());
    }
}
