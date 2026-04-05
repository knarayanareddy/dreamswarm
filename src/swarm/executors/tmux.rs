use super::{TeammateExecutor, WorkerConfig};
use crate::swarm::{SpawnStrategy, WorkerInfo, WorkerStatus};
use async_trait::async_trait;
use chrono::Utc;
use tokio::process::Command;

pub struct TmuxExecutor {
    session_name: String,
}

impl TmuxExecutor {
    pub fn new(session_name: &str) -> Self {
        Self {
            session_name: session_name.to_string(),
        }
    }

    pub async fn is_available() -> bool {
        Command::new("tmux")
            .arg("-V")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn current_session() -> Option<String> {
        std::env::var("TMUX").ok().and_then(|_tmux_env| {
            std::process::Command::new("tmux")
                .args(["display-message", "-p", "#{session_name}"])
                .output()
                .ok()
                .and_then(|o| {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                })
        })
    }

    fn build_worker_command(&self, config: &WorkerConfig) -> String {
        let mut cmd = format!(
            "dreamswarm --mode {} --model {} --directory {} ",
            config.permission_mode,
            config.model.as_deref().unwrap_or("claude-sonnet-4-20250514"),
            config.working_dir
        );
        
        cmd.push_str(&format!(
            "worker --team '{}' ",
            config.team_name
        ));

        // Note: prompt is passed to the engine as initial context in Worker logic later
        // For now we use the global prompt flag
        if !config.instructions.is_empty() {
             cmd.push_str(&format!("--prompt '{}' ", config.instructions.replace('\'', "'\\''")));
        }
        if !config.role.is_empty() {
             cmd.push_str(&format!("--role '{}' ", config.role.replace('\'', "'\\''")));
        }
        
        cmd
    }
}

#[async_trait]
impl TeammateExecutor for TmuxExecutor {
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo> {
        let worker_cmd = self.build_worker_command(config);
        let output = Command::new("tmux")
            .args([
                "split-window",
                "-d",
                "-h",
                "-t",
                &self.session_name,
                "-P",
                "-F",
                "#{pane_id}",
                &worker_cmd,
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create tmux pane: {}", stderr);
        }

        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Command::new("tmux")
            .args([
                "select-pane",
                "-t",
                &pane_id,
                "-T",
                &format!("Worker: {}", config.name),
            ])
            .output()
            .await?;

        Ok(WorkerInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            role: config.role.clone(),
            status: WorkerStatus::Active,
            spawn_type: SpawnStrategy::TmuxPane,
            session_id: None,
            worktree_path: None,
            branch_name: None,
            tmux_pane_id: Some(pane_id),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn is_alive(&self, worker: &WorkerInfo) -> bool {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["has-session", "-t", pane_id])
                .output()
                .await
                .map(|o| o.status.success())
                .unwrap_or(false)
        } else {
            false
        }
    }

    async fn send_input(&self, worker: &WorkerInfo, input: &str) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["send-keys", "-t", pane_id, input, "Enter"])
                .output()
                .await?;
            Ok(())
        } else {
            anyhow::bail!("Worker has no tmux pane ID")
        }
    }

    async fn shutdown(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["send-keys", "-t", pane_id, "/quit", "Enter"])
                .output()
                .await?;
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            if self.is_alive(worker).await {
                self.force_kill(worker).await?;
            }
        }
        Ok(())
    }

    async fn force_kill(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["kill-pane", "-t", pane_id])
                .output()
                .await?;
        }
        Ok(())
    }

    async fn cleanup(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        self.force_kill(worker).await
    }

    fn strategy(&self) -> SpawnStrategy {
        SpawnStrategy::TmuxPane
    }
}
