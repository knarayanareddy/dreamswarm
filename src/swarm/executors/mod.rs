use crate::swarm::{SpawnStrategy, WorkerInfo};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub id: String,
    pub name: String,
    pub role: String,
    pub team_name: String,
    pub instructions: String,
    pub model: Option<String>,
    pub permission_mode: String,
    pub working_dir: String,
    pub remote_host: Option<String>,
}

#[async_trait]
pub trait TeammateExecutor: Send + Sync {
    /// Spawn a new worker
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo>;
    /// Check if a worker is still alive
    async fn is_alive(&self, worker: &WorkerInfo) -> bool;
    /// Send a signal/message to a worker
    async fn send_input(&self, worker: &WorkerInfo, input: &str) -> anyhow::Result<()>;
    /// Gracefully shut down a worker
    async fn shutdown(&self, worker: &WorkerInfo) -> anyhow::Result<()>;
    /// Force-kill a worker
    async fn force_kill(&self, worker: &WorkerInfo) -> anyhow::Result<()>;
    /// Clean up all resources for a worker
    async fn cleanup(&self, worker: &WorkerInfo) -> anyhow::Result<()>;
    /// Get the spawn strategy this executor handles
    fn strategy(&self) -> SpawnStrategy;
}

pub mod in_process;
pub mod ssh;
pub mod tmux;
pub mod worktree;

pub fn detect_best_executor() -> SpawnStrategy {
    // Check for tmux
    if std::env::var("TMUX").is_ok() {
        return SpawnStrategy::TmuxPane;
    }
    // Check if git is available for worktree mode
    if std::process::Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return SpawnStrategy::GitWorktree;
    }
    // Fallback: in-process
    SpawnStrategy::InProcess
}
