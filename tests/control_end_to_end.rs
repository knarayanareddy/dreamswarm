//! Exercises the actual HTTP server and a separate daemon process.
use std::process::Stdio;
use std::time::Duration;
use tokio::process::{Child, Command};

fn start(home: &std::path::Path, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_dreamswarm"))
        .args(args)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn origin_auth_launch_and_cross_process_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut daemon = start(tmp.path(), &["--provider", "mock", "daemon", "run"]);
    let mut api = start(
        tmp.path(),
        &[
            "--provider",
            "mock",
            "api",
            "start",
            "--port",
            &port.to_string(),
        ],
    );
    let base = format!("http://127.0.0.1:{}", port);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(40))
        .build()
        .unwrap();
    let dashboard = tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(response) = client.get(&base).send().await {
                let endpoint_ready =
                    std::fs::read(tmp.path().join(".dreamswarm/daemon/control.json"))
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
                        .is_some();
                if response.status().is_success() && endpoint_ready {
                    break response.text().await.unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("daemon and API did not start");
    // A process that cannot read the endpoint secret must not be able to stop it.
    use tokio::io::AsyncWriteExt;
    let endpoint: serde_json::Value = serde_json::from_slice(
        &std::fs::read(tmp.path().join(".dreamswarm/daemon/control.json")).unwrap(),
    )
    .unwrap();
    let mut unauthenticated = tokio::net::TcpStream::connect((
        std::net::Ipv4Addr::LOCALHOST,
        endpoint["port"].as_u64().unwrap() as u16,
    ))
    .await
    .unwrap();
    unauthenticated
        .write_all(b"STOP wrong-token\n")
        .await
        .unwrap();
    drop(unauthenticated);
    assert!(daemon.try_wait().unwrap().is_none());
    let token = dashboard
        .split("const apiToken = \"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let config_url = format!("{}/api/v1/config", base);
    assert_eq!(client.get(&config_url).send().await.unwrap().status(), 401);
    assert_eq!(
        client
            .get(&config_url)
            .bearer_auth(token)
            .header("Origin", "https://evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(&config_url)
            .bearer_auth(token)
            .header("Host", format!("evil.example:{}", port))
            .send()
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(
        client
            .get(&config_url)
            .bearer_auth(token)
            .header("Origin", base.as_str())
            .send()
            .await
            .unwrap()
            .status(),
        200
    );

    let launch = format!("{}/api/v1/swarm/launch", base);
    let mut team = dreamswarm::swarm::TeamConfig::default();
    team.max_workers = 1;
    let first: serde_json::Value = client
        .post(&launch)
        .bearer_auth(token)
        .json(&serde_json::json!({"mission":"Describe this project", "team_config":team}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first["workers_spawned"], 1, "{first}");
    let second: serde_json::Value = client
        .post(&launch)
        .bearer_auth(token)
        .json(&serde_json::json!({"mission":"another"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(second["error"].as_str().unwrap().contains("already active"));

    let tasks_url = format!("{}/api/v1/swarm/tasks", base);
    let tasks = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let tasks: serde_json::Value = client
                .get(&tasks_url)
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            if tasks[0]["status"] == "Completed" {
                break tasks;
            }
            if tasks[0]["status"].get("Failed").is_some() {
                panic!("Worker failed: {tasks}");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("worker did not finish");
    assert!(tasks[0]["result"].as_str().unwrap().contains("Mock Agent"));

    let response = client
        .post(format!("{}/api/v1/control/stop", base))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    let status = tokio::time::timeout(Duration::from_secs(15), daemon.wait())
        .await
        .expect("daemon did not exit")
        .unwrap();
    assert!(status.success());
    assert!(!tmp.path().join(".dreamswarm/daemon/control.json").exists());
    let absent = client
        .post(format!("{}/api/v1/control/stop", base))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(absent.status(), 503);
    api.kill().await.unwrap();
}
