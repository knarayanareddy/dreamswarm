use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod analyzer;
pub mod collector;
pub mod engine;
pub mod planner;
pub mod pruner;
pub mod report;
pub mod sandbox;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamConfig {
    pub max_tokens: u64,
    pub max_cost_usd: f64,
    pub max_duration_secs: u64,
    pub lookback_days: u32,
    pub min_idle_secs: u64,
    pub auto_trigger: bool,
    pub max_entries_per_cycle: usize,
    pub prune_confidence_threshold: f64,
}

impl Default for DreamConfig {
    fn default() -> Self {
        Self {
            max_tokens: 50_000,
            max_cost_usd: 1.0,
            max_duration_secs: 300,
            lookback_days: 7,
            min_idle_secs: 1800,
            auto_trigger: true,
            max_entries_per_cycle: 200,
            prune_confidence_threshold: 0.3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawObservation {
    pub source: ObservationSource,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub session_id: Option<String>,
    pub tools_involved: Vec<String>,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObservationSource {
    DaemonLog,
    SessionTranscript,
    ToolOutput,
    UserStatement,
    AgentInference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryOperation {
    pub kind: OperationKind,
    pub topic: String,
    pub subtopic: String,
    pub content: String,
    pub reasoning: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationKind {
    Merge {
        source_entries: Vec<String>,
    },
    Update {
        existing_path: String,
    },
    Create,
    Prune {
        reason: PruneReason,
    },
    Confirm {
        from_confidence: String,
        to_confidence: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PruneReason {
    Contradicted,
    Stale,
    Derivable,
    Duplicate,
    LowConfidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DreamReport {
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_secs: u64,
    pub observations_collected: usize,
    pub operations_planned: usize,
    pub operations_applied: usize,
    pub entries_merged: usize,
    pub entries_created: usize,
    pub entries_pruned: usize,
    pub entries_confirmed: usize,
    pub contradictions_resolved: usize,
    pub tokens_consumed: u64,
    pub cost_usd: f64,
    pub memory_before_hash: String,
    pub memory_after_hash: String,
    pub errors: Vec<String>,
}
