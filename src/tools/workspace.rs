//! Per-task working-directory context. This is not a filesystem sandbox.
use std::path::{Path, PathBuf};

tokio::task_local! {
    static WORKING_DIR: PathBuf;
}

pub async fn scope<F: std::future::Future>(dir: PathBuf, future: F) -> F::Output {
    WORKING_DIR.scope(dir, future).await
}

pub fn resolve(path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        return path.to_path_buf();
    }
    WORKING_DIR
        .try_with(|dir| dir.join(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub fn working_dir() -> Option<PathBuf> {
    WORKING_DIR.try_with(Clone::clone).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::tools::bash_tool::BashTool;
    use crate::tools::{file_read::FileReadTool, file_write::FileWriteTool, Tool};
    use serde_json::json;

    #[tokio::test]
    async fn simultaneous_contexts_use_distinct_directories() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        async fn run(dir: PathBuf, value: &'static str) {
            scope(dir, async move {
                FileWriteTool
                    .execute(&json!({"path":"same.txt", "content":value}))
                    .await
                    .unwrap();
                tokio::task::yield_now().await;
                let output = FileReadTool
                    .execute(&json!({"path":"same.txt"}))
                    .await
                    .unwrap();
                assert_eq!(output.content, value);
                #[cfg(unix)]
                {
                    let output = BashTool
                        .execute(&json!({"command":"cat same.txt"}))
                        .await
                        .unwrap();
                    assert_eq!(output.content, value);
                }
            })
            .await;
        }
        tokio::join!(
            run(first.path().into(), "first"),
            run(second.path().into(), "second")
        );
        assert_eq!(
            std::fs::read_to_string(first.path().join("same.txt")).unwrap(),
            "first"
        );
        assert_eq!(
            std::fs::read_to_string(second.path().join("same.txt")).unwrap(),
            "second"
        );
    }
}
