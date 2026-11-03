use super::command::{is_safe_identifier, percent_encode};
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

    /// Builds the worker argv.
    ///
    /// Values are passed as separate arguments; none of them is concatenated
    /// into a shell string. Every value that tmux will re-parse is
    /// percent-encoded so a mission containing spaces, quotes or `;` cannot
    /// smuggle additional tmux commands.
    fn build_worker_args(&self, config: &WorkerConfig) -> anyhow::Result<Vec<String>> {
        if !is_safe_identifier(&config.team_name) {
            anyhow::bail!("invalid team name '{}'", config.team_name);
        }
        if !is_safe_identifier(&config.role) {
            anyhow::bail!("invalid role '{}'", config.role);
        }

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
            percent_encode(&config.working_dir),
            "worker".to_string(),
            "--team".to_string(),
            percent_encode(&config.team_name),
        ];

        if !config.instructions.is_empty() {
            args.push("--prompt".to_string());
            args.push(percent_encode(&config.instructions));
        }
        if !config.role.is_empty() {
            args.push("--role".to_string());
            args.push(percent_encode(&config.role));
        }

        Ok(args)
    }
}

#[async_trait]
impl TeammateExecutor for TmuxExecutor {
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo> {
        if !is_safe_identifier(&self.session_name) {
            anyhow::bail!("invalid tmux session name '{}'", self.session_name);
        }
        let worker_args = self.build_worker_args(config)?;

        // No shell is involved: tmux execs the argv directly.
        let output = Command::new("tmux")
            .arg("split-window")
            .arg("-d")
            .arg("-h")
            .arg("-t")
            .arg(&self.session_name)
            .arg("-P")
            .arg("-F")
            .arg("#{pane_id}")
            .args(&worker_args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create tmux pane: {}", stderr);
        }

        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let title = format!("Worker: {}", config.name);
        Command::new("tmux")
            .args(["select-pane", "-t", pane_id.as_str(), "-T", title.as_str()])
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
            instructions: String::new(),
            tmux_pane_id: Some(pane_id),
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
            false
        }
    }

    async fn send_input(&self, worker: &WorkerInfo, input: &str) -> anyhow::Result<()> {
        if let Some(ref pane_id) = worker.tmux_pane_id {
            Command::new("tmux")
                .args(["send-keys", "-t", pane_id, "--", input, "Enter"])
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

#[cfg(test)]
mod tests {
    use super::*;

    fn config(instructions: &str) -> WorkerConfig {
        WorkerConfig {
            id: "w-1".into(),
            name: "coder".into(),
            role: "coder".into(),
            team_name: "team-1".into(),
            instructions: instructions.into(),
            model: Some("claude-sonnet-4-20250514".into()),
            permission_mode: "default".into(),
            working_dir: "/tmp/project".into(),
            state_dir: std::path::PathBuf::from("/tmp/state"),
            remote_host: None,
        }
    }

    #[test]
    fn mission_text_cannot_inject_tmux_commands() {
        let executor = TmuxExecutor::new("dreamswarm");
        let args = executor
            .build_worker_args(&config("fix bug; kill-server # $(reboot)"))
            .unwrap();
        let joined = args.join(" ");
        assert!(!joined.contains(';'));
        assert!(!joined.contains('#'));
        assert!(!joined.contains("$("));
        // The prompt survives as one opaque argument.
        let prompt_index = args.iter().position(|a| a == "--prompt").unwrap();
        assert!(args[prompt_index + 1].contains("%3B"));
    }

    #[test]
    fn rejects_unsafe_team_and_role() {
        let executor = TmuxExecutor::new("dreamswarm");
        let mut cfg = config("");
        cfg.team_name = "team;kill-server".into();
        assert!(executor.build_worker_args(&cfg).is_err());

        let mut cfg = config("");
        cfg.role = "coder $(id)".into();
        assert!(executor.build_worker_args(&cfg).is_err());
    }

    #[test]
    fn command_uses_worker_subcommand() {
        let executor = TmuxExecutor::new("dreamswarm");
        let args = executor.build_worker_args(&config("do work")).unwrap();
        assert_eq!(args[0], "dreamswarm");
        assert!(args.contains(&"worker".to_string()));
        assert!(args.contains(&"--team".to_string()));
    }
}
