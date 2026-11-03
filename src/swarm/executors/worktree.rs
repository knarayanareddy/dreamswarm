use super::command::{is_safe_identifier, percent_encode};
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

    /// Builds the worker argv for an already-created worktree.
    ///
    /// The previous version produced `cd <path> && dreamswarm ... --prompt
    /// '<mission>'` and handed it to a shell. Besides the injection risk, it
    /// omitted the `worker` subcommand, so every "worker" started the
    /// interactive TUI instead of the delegated agent.
    fn build_worker_args(
        &self,
        config: &WorkerConfig,
        working_dir: &std::path::Path,
    ) -> anyhow::Result<Vec<String>> {
        if !is_safe_identifier(&config.team_name) {
            anyhow::bail!("invalid team name '{}'", config.team_name);
        }
        if !is_safe_identifier(&config.role) {
            anyhow::bail!("invalid role '{}'", config.role);
        }

        let working_dir = working_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("worktree path is not valid UTF-8"))?;

        let mut args = vec![
            "dreamswarm".to_string(),
            "--mode".to_string(),
            percent_encode(&config.permission_mode),
            "--model".to_string(),
            percent_encode(
                config
                    .model
                    .as_deref()
                    .unwrap_or("claude-sonnet-4-20250514"),
            ),
            "--directory".to_string(),
            percent_encode(working_dir),
            "worker".to_string(),
            "--team".to_string(),
            percent_encode(&config.team_name),
            "--role".to_string(),
            percent_encode(&config.role),
        ];

        if !config.instructions.is_empty() {
            args.push("--prompt".to_string());
            args.push(percent_encode(&config.instructions));
        }

        Ok(args)
    }
}

#[async_trait]
impl TeammateExecutor for WorktreeExecutor {
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo> {
        if !is_safe_identifier(&config.team_name) {
            anyhow::bail!("invalid team name '{}'", config.team_name);
        }
        if !is_safe_identifier(&self.tmux_session) {
            anyhow::bail!("invalid tmux session name '{}'", self.tmux_session);
        }
        // A branch is a git ref: derive it from validated identifiers only.
        let branch_name = format!(
            "dreamswarm/{}/{}",
            config.team_name,
            crate::tools::path_guard::sanitize_filename(&config.name)
        );
        let mega_workspace = self.worktrees_dir.join(&config.id);
        std::fs::create_dir_all(&mega_workspace)?;

        let primary_repo_name = self
            .repo_root
            .file_name()
            .map(|n| crate::tools::path_guard::sanitize_filename(&n.to_string_lossy()))
            .unwrap_or_else(|| "repo".to_string());
        let worktree_path = mega_workspace.join(&primary_repo_name);
        let base_branch = self.current_branch().await?;

        let worktree_path_str = worktree_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("worktree path is not valid UTF-8"))?;
        let output = Command::new("git")
            .args([
                "worktree",
                "add",
                "-b",
                branch_name.as_str(),
                worktree_path_str,
                base_branch.as_str(),
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

            let linked_wt_str = linked_wt_path
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("linked worktree path is not valid UTF-8"))?;
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
                    branch_name.as_str(),
                    linked_wt_str,
                    linked_base_branch.as_str(),
                ])
                .current_dir(&repo_path)
                .output()
                .await?;
        }

        // Launch the worker as argv (no shell), inside its worktree.
        let worker_args = self.build_worker_args(config, &mega_workspace)?;

        let pane_output = Command::new("tmux")
            .arg("split-window")
            .arg("-d")
            .arg("-h")
            .arg("-t")
            .arg(&self.tmux_session)
            .arg("-P")
            .arg("-F")
            .arg("#{pane_id}")
            .arg("-c")
            .arg(&mega_workspace)
            .args(&worker_args)
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
