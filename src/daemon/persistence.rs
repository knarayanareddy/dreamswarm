use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::swarm::TeamState;
use chrono::{DateTime, Utc};

#[derive(Serialize, Deserialize)]
pub struct DaemonProcessState {
    pub active_swarms: Vec<TeamState>,
    pub last_checkpoint: DateTime<Utc>,
    pub pid: u32,
}

pub struct PersistenceManager {
    checkpoint_path: PathBuf,
}

impl PersistenceManager {
    pub fn new(state_dir: PathBuf) -> Self {
        // We store the process-level checkpoint in the daemon's core state directory.
        Self {
            checkpoint_path: state_dir.join("CHECKPOINT.json"),
        }
    }

    /// Persists the full state of the daemon to disk.
    pub fn checkpoint(&self, swarms: Vec<TeamState>) -> anyhow::Result<()> {
        let state = DaemonProcessState {
            active_swarms: swarms,
            last_checkpoint: Utc::now(),
            pid: std::process::id(),
        };
        
        let content = serde_json::to_string_pretty(&state)?;
        std::fs::write(&self.checkpoint_path, content)?;
        
        tracing::info!("Persistence: Process-level checkpoint created at {}", self.checkpoint_path.display());
        Ok(())
    }

    /// Attempts to load the last known process state for a Warm-Start.
    pub fn load_last_state(&self) -> anyhow::Result<DaemonProcessState> {
        let content = std::fs::read_to_string(&self.checkpoint_path)?;
        let state = serde_json::from_str(&content)?;
        Ok(state)
    }

    pub fn exists(&self) -> bool {
        self.checkpoint_path.exists()
    }
}
