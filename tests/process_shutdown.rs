#![cfg(unix)]
use dreamswarm::tools::{bash_tool::BashTool, Tool};
use std::time::Duration;

#[tokio::test]
async fn cancelling_a_shell_stops_ordinary_descendants() {
    let temp = tempfile::tempdir().unwrap();
    let started = temp.path().join("started");
    let survived = temp.path().join("survived");
    let command = format!(
        "(touch '{}'; sleep 3; touch '{}') & wait",
        started.display(),
        survived.display()
    );
    let task = tokio::spawn(async move {
        BashTool
            .execute(&serde_json::json!({ "command": command }))
            .await
    });
    tokio::time::timeout(Duration::from_secs(10), async {
        while !started.exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("descendant never started");
    task.abort();
    let _ = task.await;
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(
        !survived.exists(),
        "descendant continued after cancellation"
    );
}
