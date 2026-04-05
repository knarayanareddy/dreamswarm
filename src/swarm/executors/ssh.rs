use crate::swarm::executors::{TeammateExecutor, WorkerConfig};
use crate::swarm::{SpawnStrategy, WorkerInfo, WorkerStatus};
use async_trait::async_trait;
use chrono::Utc;
use std::process::Stdio;
use tokio::process::Command;
use tracing::{error, info, warn};

pub struct SshExecutor {
    pub remote_hosts: Vec<String>,
}

impl SshExecutor {
    pub fn new(remote_hosts: Vec<String>) -> Self {
        Self { remote_hosts }
    }

    async fn sync_files(&self, host: &str, local_dir: &str) -> anyhow::Result<()> {
        info!("Syncing files to {} via rsync...", host);
        let status = Command::new("rsync")
            .args([
                "-avz",
                "--exclude",
                "target",
                "--exclude",
                ".git",
                &format!("{}/", local_dir),
                &format!("{}:{}/", host, local_dir),
            ])
            .status()
            .await?;

        if !status.success() {
            warn!("rsync failed for {}, falling back to scp (slower)...", host);
            // Fallback scp logic could go here, but rsync is preferred for swarms
            anyhow::bail!("rsync failed to sync files to remote host {}", host);
        }
        Ok(())
    }
}

#[async_trait]
impl TeammateExecutor for SshExecutor {
    async fn spawn(&self, config: &WorkerConfig) -> anyhow::Result<WorkerInfo> {
        let host = config
            .remote_host
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No remote host provided for SSH executor"))?;

        // 1. Sync project files to remote
        self.sync_files(host, &config.working_dir).await?;

        // 2. Spawn agent via SSH
        info!("Spawning worker '{}' on remote host {}", config.name, host);
        let mut cmd = Command::new("ssh");
        cmd.args([
            host,
            &format!(
                "cd {} && cargo run -- agent --team {} --id {} --name '{}' --role '{}'",
                config.working_dir,
                config.team_name,
                config.id,
                config.name,
                config.role
            ),
        ]);

        // Run in background on remote
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = cmd.spawn()?;
        
        // We don't wait for it to finish, as agents are long-running
        // However, we should check if it started successfully
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                anyhow::bail!("SSH spawn failed immediately on host {}", host);
            }
        }

        Ok(WorkerInfo {
            id: config.id.clone(),
            name: config.name.clone(),
            role: config.role.clone(),
            status: WorkerStatus::Spawning,
            spawn_type: SpawnStrategy::SSH,
            session_id: None,
            worktree_path: None,
            branch_name: None,
            tmux_pane_id: None,
            remote_host: Some(host.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn is_alive(&self, worker: &WorkerInfo) -> bool {
        let host = match &worker.remote_host {
            Some(h) => h,
            None => return false,
        };

        // Check if the agent process is still running on remote
        let output = Command::new("ssh")
            .args([
                host,
                &format!("ps aux | grep 'dreamswarm' | grep '{}' | grep -v grep", worker.id),
            ])
            .output()
            .await;

        match output {
            Ok(out) => out.status.success() && !out.stdout.is_empty(),
            Err(_) => false,
        }
    }

    async fn send_input(&self, worker: &WorkerInfo, input: &str) -> anyhow::Result<()> {
        // Mailbox handles most communication, but we could implement raw stdin here if needed
        warn!("send_input not implemented for SSH executor (use mailbox instead)");
        Ok(())
    }

    async fn shutdown(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        let host = worker.remote_host.as_deref().unwrap_or("");
        info!("Shutting down remote worker {} on {}", worker.id, host);
        // We usually send Shutdown via mailbox, but this is a fallback
        let _ = Command::new("ssh")
            .args([
                host,
                &format!("pkill -f 'dreamswarm.*--id {}'", worker.id),
            ])
            .status()
            .await;
        Ok(())
    }

    async fn force_kill(&self, worker: &WorkerInfo) -> anyhow::Result<()> {
        let host = worker.remote_host.as_deref().unwrap_or("");
        warn!("Force-killing remote worker {} on {}", worker.id, host);
        let _ = Command::new("ssh")
            .args([
                host,
                &format!("pkill -9 -f 'dreamswarm.*--id {}'", worker.id),
            ])
            .status()
            .await;
        Ok(())
    }

    async fn cleanup(&self, _worker: &WorkerInfo) -> anyhow::Result<()> {
        // Optional: clean up temporary workdirs on remote
        Ok(())
    }

    fn strategy(&self) -> SpawnStrategy {
        SpawnStrategy::SSH
    }
}
