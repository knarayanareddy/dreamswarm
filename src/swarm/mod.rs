use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamConfig {
    pub team_name: String,
    pub max_workers: usize,
    pub spawn_strategy: SpawnStrategy,
    pub merge_strategy: MergeStrategy,
    pub worker_mode: String,
    pub worker_model: Option<String>,
    pub timeout_seconds: u64,
}

impl Default for TeamConfig {
    fn default() -> Self {
        Self {
            team_name: format!("team-{}", &uuid::Uuid::new_v4().to_string()[..8]),
            max_workers: 4,
            spawn_strategy: SpawnStrategy::InProcess,
            merge_strategy: MergeStrategy::LeadReview,
            worker_mode: "default".to_string(),
            worker_model: None,
            timeout_seconds: 600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SpawnStrategy {
    InProcess,
    TmuxPane,
    GitWorktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MergeStrategy {
    CherryPick,
    OctopusMerge,
    Sequential,
    LeadReview,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerInfo {
    pub id: String,
    pub name: String,
    pub role: String,
    pub status: WorkerStatus,
    pub spawn_type: SpawnStrategy,
    pub session_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub tmux_pane_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WorkerStatus {
    Spawning,
    Idle,
    Active,
    Completed,
    Failed(String),
    ShuttingDown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    Chat {
        text: String,
    },
    TaskAssignment {
        task_id: String,
        instructions: String,
    },
    TaskResult {
        task_id: String,
        result: String,
    },
    StatusUpdate {
        status: WorkerStatus,
    },
    ShutdownRequest,
    ShutdownAck,
    ModeSetRequest {
        mode: String,
    },
    ApprovalRequest {
        action: String,
        description: String,
    },
    ApprovalResponse {
        approved: bool,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamState {
    pub config: TeamConfig,
    pub workers: Vec<WorkerInfo>,
    pub status: TeamStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TeamStatus {
    Active,
    Completing,
    Completed,
    Failed(String),
}

pub mod coordinator;
pub mod executors;
pub mod lifecycle;
pub mod mailbox;
pub mod result_merger;
pub mod subagent;
pub mod task_list;
