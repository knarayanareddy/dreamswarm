use std::path::PathBuf;
use tokio::process::Command;

pub struct DaemonProcess {
    state_dir: PathBuf,
}

impl DaemonProcess {
    pub fn new(state_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&state_dir).ok();
        Self { state_dir }
    }

    pub async fn start(&self, extra_args: &[&str]) -> anyhow::Result<()> {
        if self.is_running().await? {
            anyhow::bail!("Daemon is already running. Use 'dreamswarm daemon stop' first.");
        }
        let session_name = "dreamswarm-daemon";
        let mut cmd_parts = vec!["dreamswarm", "daemon", "run"];
        cmd_parts.extend_from_slice(extra_args);
        let cmd_str = cmd_parts.join(" ");

        let output = Command::new("tmux")
            .args(["new-session", "-d", "-s", session_name, "-x", "200", "-y", "50", &cmd_str])
            .output()
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to start daemon: {}", stderr);
        }

        let pid = std::process::id();
        let state = serde_json::json!({
            "pid": pid,
            "tmux_session": session_name,
            "started_at": chrono::Utc::now().to_rfc3339(),
            "status": "running"
        });
        std::fs::write(self.state_dir.join("daemon_state.json"), serde_json::to_string_pretty(&state)?)?;
        tracing::info!("Daemon started in tmux session '{}'", session_name);
        println!("🌙 KAIROS daemon started in background.");
        println!("Attach: tmux attach -t {}", session_name);
        println!("Stop: dreamswarm daemon stop");
        Ok(())
    }

    pub async fn stop(&self) -> anyhow::Result<()> {
        if !self.is_running().await? {
            println!("Daemon is not running.");
            return Ok(());
        }
        let session_name = "dreamswarm-daemon";
        let _ = Command::new("tmux").args(["send-keys", "-t", session_name, "C-c", "Enter"]).output().await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let _ = Command::new("tmux").args(["kill-session", "-t", session_name]).output().await;
        
        if let Ok(state_content) = std::fs::read_to_string(self.state_dir.join("daemon_state.json")) {
            if let Ok(mut state) = serde_json::from_str::<serde_json::Value>(&state_content) {
                state["status"] = serde_json::json!("stopped");
                state["stopped_at"] = serde_json::json!(chrono::Utc::now().to_rfc3339());
                std::fs::write(self.state_dir.join("daemon_state.json"), serde_json::to_string_pretty(&state)?)?;
            }
        }
        println!("🛑 KAIROS daemon stopped.");
        Ok(())
    }

    pub async fn is_running(&self) -> anyhow::Result<bool> {
        let output = Command::new("tmux").args(["has-session", "-t", "dreamswarm-daemon"]).output().await;
        Ok(output.map(|o| o.status.success()).unwrap_or(false))
    }

    pub async fn status(&self) -> anyhow::Result<crate::daemon::DaemonStatus> {
        let running = self.is_running().await?;
        let state_path = self.state_dir.join("daemon_state.json");
        let file_state: Option<serde_json::Value> = if state_path.exists() {
            std::fs::read_to_string(&state_path).ok().and_then(|s| serde_json::from_str(&s).ok())
        } else { None };

        let started_at = file_state.as_ref().and_then(|s| s.get("started_at")).and_then(|v| v.as_str())
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|dt| dt.with_timezone(&chrono::Utc));
        let pid = file_state.as_ref().and_then(|s| s.get("pid")).and_then(|v| v.as_u64()).map(|p| p as u32);
        
        let daily_log = crate::daemon::daily_log::DailyLog::new(&self.state_dir)?;
        
        Ok(crate::daemon::DaemonStatus {
            running,
            pid,
            started_at,
            last_heartbeat: None,
            ticks_total: 0,
            actions_taken: daily_log.actions_today().unwrap_or(0),
            observations_logged: 0,
            tokens_used_today: daily_log.tokens_used_today().unwrap_or(0),
            cost_today_usd: daily_log.cost_today().unwrap_or(0.0),
            trust_level: 1.0, 
            user_idle_since: None,
            next_dream_at: None,
        })
    }
}
