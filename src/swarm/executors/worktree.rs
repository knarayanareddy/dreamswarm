use super::{TeammateExecutor, WorkerConfig};
use crate::swarm::{SpawnStrategy, WorkerInfo, WorkerStatus};
use async_trait::async_trait;
use chrono::Utc;
use std::path::PathBuf;
use tokio::process::Command;

pub struct WorktreeExecutor {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    tmux_session: String,
    linked_repositories: Vec<String>,
}

impl WorktreeExecutor {
    pub fn new(
        repo_root: PathBuf,
        tmux_session: &str,
        linked_repositories: Vec<String>,
    ) -> anyhow::Result<Self> {
        let worktrees_dir = repo_root.join(".dreamswarm-worktrees");
        std::fs::create_dir_all(&worktrees_dir)?;
        Ok(Self {
            repo_root,
            worktrees_dir,
            tmux_session: tmux_session.to_string(),
            linked_repositories,
        })
    }

    pub async fn is_available(working_dir: &str) -> bool {
        Command::new("git")
            .args(["rev-parse", "--is-inside-work-tree"])
            .current_dir(working_dir)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn current_branch(&self) -> anyhow::Result<String> {
        let output = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(&self.repo_root)
            .output()
            .await?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

#[async_trait]
impl TeammateExecutor for WorktreeExecutor {
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo> {
        let branch_name = format!(
            "dreamswarm/{}/{}",
            config.team_name,
            config.name.to_lowercase().replace(' ', "-")
        );
        let mega_workspace = self.worktrees_dir.join(&config.id);
        std::fs::create_dir_all(&mega_workspace)?;

        let primary_repo_name = self
            .repo_root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let worktree_path = mega_workspace.join(&primary_repo_name);
        let base_branch = self.current_branch().await?;

        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                &branch_name,
                worktree_path.to_str().unwrap(),
                &base_branch,
            ])
            .current_dir(&self.repo_root)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create git worktree for primary repo: {}", stderr);
        }

        // Handle linked repositories
        for repo_path_str in &self.linked_repositories {
            let repo_path = PathBuf::from(repo_path_str);
            if !repo_path.exists() {
                continue;
            }
            let repo_name = repo_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let linked_wt_path = mega_workspace.join(&repo_name);

            let branch_cmd = Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&repo_path)
                .output()
                .await?;
            let linked_base_branch = String::from_utf8_lossy(&branch_cmd.stdout)
                .trim()
                .to_string();

            Command::new("git")
                .args([
                    "worktree",
                    "add",
                    "-b",
                    &branch_name,
                    linked_wt_path.to_str().unwrap(),
                    &linked_base_branch,
                ])
                .current_dir(&repo_path)
                .output()
                .await?;
        }

        let worker_cmd = format!(
            "cd {} && dreamswarm --mode {} --directory {} --prompt '{}'",
            mega_workspace.to_string_lossy(),
            config.permission_mode,
            mega_workspace.to_string_lossy(),
            config.instructions.replace('\'', "'\\''"),
        );

        let pane_output = Command::new("tmux")
            .args([
                "split-window",
                "-d",
                "-h",
                "-t",
                &self.tmux_session,
                "-P",
                "-F",
                "#{pane_id}",
                &worker_cmd,
            ])
            .output()
            .await?;

        let pane_id = if pane_output.status.success() {
            Some(
                String::from_utf8_lossy(&pane_output.stdout)
                    .trim()
                    .to_string(),
            )
        } else {
            tracing::warn!("Failed to create tmux pane for worktree worker, running detached");
            None
        };

        Ok(WorkerInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            role: config.role.clone(),
            status: WorkerStatus::Active,
            spawn_type: SpawnStrategy::GitWorktree,
            session_id: None,
            worktree_path: Some(worktree_path.to_string_lossy().to_string()),
            branch_name: Some(branch_name),
            instructions: String::new(),
            tmux_pane_id: pane_id,
            remote_host: None,
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
            worker
                .worktree_path
                .as_ref()
                .map(|p| std::path::Path::new(p).exists())
                .unwrap_or(false)
        }
    }

    async fn send_input(&self, worker: &WorkerInfo, input: &str) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["send-keys", "-t", pane_id, input, "Enter"])
                .output()
                .await?;
        }
        Ok(())
    }

    async fn shutdown(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["send-keys", "-t", pane_id, "/quit", "Enter"])
                .output()
                .await?;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if self.is_alive(worker).await {
                self.force_kill(worker).await?;
            }
        }
        Ok(())
    }

    async fn force_kill(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            let _ = Command::new("tmux")
                .args(["kill-pane", "-t", pane_id])
                .output()
                .await;
        }
        Ok(())
    }

    async fn cleanup(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        self.force_kill(worker).await?;
        if let Some(ref worktree_path) = worker.worktree_path {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", worktree_path])
                .current_dir(&self.repo_root)
                .output()
                .await;
        }
        if let Some(ref branch) = worker.branch_name {
            let _ = Command::new("git")
                .args(["branch", "-D", branch])
                .current_dir(&self.repo_root)
                .output()
                .await;
        }
        Ok(())
    }

    fn strategy(&self) -> SpawnStrategy {
        SpawnStrategy::GitWorktree
    }
}
