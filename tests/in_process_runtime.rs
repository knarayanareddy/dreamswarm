use dreamswarm::runtime::config::AppConfig;
use dreamswarm::swarm::executors::{in_process::InProcessExecutor, TeammateExecutor, WorkerConfig};
use dreamswarm::swarm::mailbox::Mailbox;
use dreamswarm::swarm::MessageContent;
use std::time::Duration;

#[tokio::test]
async fn assigned_task_runs_agent_and_reports_result() {
    let tmp = tempfile::tempdir().unwrap();
    let app = AppConfig {
        provider: "mock".into(),
        state_dir: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let executor = InProcessExecutor::with_config(app);
    let config = WorkerConfig {
        id: "worker-1".into(),
        name: "worker".into(),
        role: "tester".into(),
        team_name: "test".into(),
        instructions: "Be helpful".into(),
        model: None,
        permission_mode: "default".into(),
        working_dir: std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        state_dir: tmp.path().to_path_buf(),
        remote_host: None,
    };
    let worker = executor.spawn(&config).await.unwrap();
    let mut lead = Mailbox::new(tmp.path().to_path_buf(), "test", "lead").unwrap();
    lead.send_task_assignment(&worker.id, "task-1", "Describe your capabilities")
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        'wait: loop {
            for message in lead.receive().unwrap() {
                match message.content {
                    MessageContent::TaskResult { task_id, result } => {
                        break 'wait (task_id, result)
                    }
                    MessageContent::TaskFailed { error, .. } => panic!("worker failed: {error}"),
                    _ => continue,
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    // Assert the model's response, not a mailbox acknowledgement.
    let (id, text) = result.unwrap();
    assert_eq!(id, "task-1");
    assert!(text.contains("Mock Agent"), "{text}");
    executor.force_kill(&worker).await.unwrap();
    assert!(!executor.is_alive(&worker).await);
}

#[tokio::test]
async fn worker_applies_approved_file_edit() {
    use axum::{extract::State, routing::post, Json, Router};
    use serde_json::{json, Value};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    #[derive(Clone)]
    struct Fake {
        calls: Arc<AtomicUsize>,
        path: String,
    }
    async fn complete(State(fake): State<Fake>, Json(_): Json<Value>) -> Json<Value> {
        if fake.calls.fetch_add(1, Ordering::SeqCst) % 2 == 0 {
            Json(
                json!({"content":[{"type":"tool_use","id":"write-1","name":"FileWrite","input":{"path":fake.path,"content":"written by worker"}}],"usage":{"input_tokens":1,"output_tokens":1},"stop_reason":"tool_use"}),
            )
        } else {
            Json(
                json!({"content":[{"type":"text","text":"Edit complete"}],"usage":{"input_tokens":1,"output_tokens":1},"stop_reason":"end_turn"}),
            )
        }
    }
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let output = project.join("result.txt");
    let fake = Fake {
        calls: Arc::new(AtomicUsize::new(0)),
        path: "result.txt".into(),
    };
    let app = Router::new()
        .route("/v1/messages", post(complete))
        .with_state(fake);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    std::env::set_var("ANTHROPIC_API_KEY", "test");
    std::env::set_var(
        "ANTHROPIC_BASE_URL",
        format!("http://{}", listener.local_addr().unwrap()),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let app_config = AppConfig {
        state_dir: tmp.path().to_path_buf(),
        permission_mode: dreamswarm::runtime::permissions::AgentMode::AcceptEdits,
        ..Default::default()
    };
    let executor = InProcessExecutor::with_config(app_config);
    let config = WorkerConfig {
        id: "editor".into(),
        name: "editor".into(),
        role: "coder".into(),
        team_name: "edit-test".into(),
        instructions: "Implement the task".into(),
        model: None,
        permission_mode: "accept-edits".into(),
        working_dir: project.to_string_lossy().into_owned(),
        state_dir: tmp.path().to_path_buf(),
        remote_host: None,
    };
    let worker = executor.spawn(&config).await.unwrap();
    let mut lead = Mailbox::new(tmp.path().to_path_buf(), "edit-test", "lead").unwrap();
    lead.send_task_assignment(&worker.id, "task-edit", "Create the result file")
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(20), async {
        'wait: loop {
            for message in lead.receive().unwrap() {
                match message.content {
                    MessageContent::TaskResult { result, .. } => break 'wait result,
                    MessageContent::TaskFailed { error, .. } => panic!("worker failed: {error}"),
                    _ => {}
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(result, "Edit complete");
    assert_eq!(
        std::fs::read_to_string(output).unwrap(),
        "written by worker"
    );
    let second_project = tmp.path().join("other-project");
    std::fs::create_dir(&second_project).unwrap();
    let mut second_config = config.clone();
    second_config.id = "editor-2".into();
    second_config.working_dir = second_project.to_string_lossy().into_owned();
    let second_worker = executor.spawn(&second_config).await.unwrap();
    lead.send_task_assignment(&second_worker.id, "second-edit", "Create the result file")
        .unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let messages = lead.receive().unwrap();
            if messages.iter().any(|message| matches!(&message.content, MessageContent::TaskResult { task_id, .. } if task_id == "second-edit")) {
                break;
            }
            for message in messages {
                if let MessageContent::TaskFailed { error, .. } = message.content {
                    panic!("second worker failed: {error}");
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await.expect("second worker did not finish");
    assert_eq!(
        std::fs::read_to_string(second_project.join("result.txt")).unwrap(),
        "written by worker"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("result.txt")).unwrap(),
        "written by worker"
    );
    executor.force_kill(&worker).await.unwrap();
    executor.force_kill(&second_worker).await.unwrap();
    server.abort();
}

#[test]
fn coordinator_rejects_overlapping_worker_directories() {
    use dreamswarm::swarm::{coordinator::SwarmCoordinator, TeamConfig};
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("one");
    let nested = first.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let config = TeamConfig {
        team_name: format!("directories-{}", uuid::Uuid::new_v4()),
        max_workers: 2,
        worker_directories: vec![
            first.to_string_lossy().into_owned(),
            nested.to_string_lossy().into_owned(),
        ],
        ..Default::default()
    };
    assert!(SwarmCoordinator::new(config, ".", temp.path().join("state")).is_err());
}
