use dreamswarm::swarm::coordinator::SwarmCoordinator;
use dreamswarm::swarm::TeamConfig;
use std::fs;
use std::path::PathBuf;

#[tokio::test]
async fn test_multi_repo_orchestration() -> anyhow::Result<()> {
    let dummy_root = PathBuf::from("/tmp/dreamswarm-dummy-final");
    if !dummy_root.exists() {
        fs::create_dir_all(&dummy_root)?;
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&dummy_root)
            .output()?;
        fs::write(dummy_root.join("README.md"), "dummy")?;
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&dummy_root)
            .output()?;
        std::process::Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(&dummy_root)
            .output()?;
    }

    let config = TeamConfig {
        linked_repositories: vec![dummy_root.to_string_lossy().to_string()],
        ..Default::default()
    };

    let working_dir = std::env::current_dir()?.to_string_lossy().to_string();
    let state_dir = PathBuf::from("./.dreamswarm-test-state-final");
    fs::create_dir_all(&state_dir)?;

    let mut coordinator = SwarmCoordinator::new(config, &working_dir, state_dir)?;

    let worker = coordinator
        .spawn_worker("test-worker", "coder", "echo hello")
        .await?;

    // Check if the worktree exists and contains both the main repo and the dummy repo
    if let Some(ref path) = worker.worktree_path {
        let parent_path = PathBuf::from(path).parent().unwrap().to_path_buf();
        let primary_path = PathBuf::from(&working_dir);
        let primary_name = primary_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let dummy_name = dummy_root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        assert!(
            parent_path.join(&primary_name).exists(),
            "Primary repo worktree should exist"
        );
        assert!(
            parent_path.join(&dummy_name).exists(),
            "Linked repo worktree should exist"
        );
    }

    Ok(())
}
