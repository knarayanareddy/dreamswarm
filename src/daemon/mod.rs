use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

pub mod brief_mode;
pub mod daily_log;
pub mod heartbeat;
pub mod initiative;
pub mod kairos;
pub mod process;
pub mod schedule;
pub mod signals;
pub mod trust;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub heartbeat_interval: Duration,
    pub blocking_budget: Duration,
    pub dream_idle_threshold: Duration,
    pub daily_token_budget: u64,
    pub daily_cost_budget: f64,
    pub state_dir: PathBuf,
    pub notifications_enabled: bool,
    pub brief_mode: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let state_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dreamswarm")
            .join("daemon");
        Self {
            heartbeat_interval: Duration::from_secs(30),
            blocking_budget: Duration::from_secs(15),
            dream_idle_threshold: Duration::from_secs(1800),
            daily_token_budget: 500_000,
            daily_cost_budget: 10.0,
            state_dir,
            notifications_enabled: true,
            brief_mode: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Initiative {
    Act(ProactiveAction),
    Observe(String),
    Sleep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProactiveAction {
    FixBuildError {
        file: String,
        error: String,
    },
    RespondToPR {
        repo: String,
        pr_number: u64,
        analysis: String,
    },
    RunTests {
        reason: String,
        changed_files: Vec<String>,
    },
    UpdateDocs {
        files: Vec<String>,
        reason: String,
    },
    SendNotification {
        message: String,
        urgency: Urgency,
    },
    CustomAction {
        description: String,
        tool_calls: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub started_at: Option<DateTime<Utc>>,
    pub last_heartbeat: Option<DateTime<Utc>>,
    pub ticks_total: u64,
    pub actions_taken: u64,
    pub observations_logged: u64,
    pub tokens_used_today: u64,
    pub cost_today_usd: f64,
    pub trust_level: f64,
    pub user_idle_since: Option<DateTime<Utc>>,
    pub next_dream_at: Option<DateTime<Utc>>,
}
