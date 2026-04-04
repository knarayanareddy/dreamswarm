use crate::daemon::daily_log::{DailyLog, LogEntry, LogEntryKind};
use crate::dream::{DreamConfig, ObservationSource, RawObservation};
use crate::memory::MemorySystem;
use chrono::Utc;
use std::path::PathBuf;

pub struct ObservationCollector {
    config: DreamConfig,
    daemon_state_dir: PathBuf,
}

impl ObservationCollector {
    pub fn new(config: DreamConfig, daemon_state_dir: PathBuf) -> Self {
        Self { config, daemon_state_dir }
    }

    pub fn collect(&self, memory: &MemorySystem) -> anyhow::Result<Vec<RawObservation>> {
        let mut observations = Vec::new();
        observations.extend(self.collect_from_daemon_logs()?);
        observations.extend(self.collect_from_transcripts(memory)?);
        observations.extend(self.collect_from_current_memory(memory)?);
        observations.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        observations.truncate(self.config.max_entries_per_cycle);
        tracing::info!("Collected {} observations", observations.len());
        Ok(observations)
    }

    fn collect_from_daemon_logs(&self) -> anyhow::Result<Vec<RawObservation>> {
        let daily_log = DailyLog::new(&self.daemon_state_dir)?;
        let entries = daily_log.read_recent_days(self.config.lookback_days)?;
        let observations: Vec<RawObservation> = entries.iter()
            .filter(|e| self.is_relevant_log_entry(e))
            .map(|e| RawObservation {
                source: ObservationSource::DaemonLog,
                content: e.content.clone(),
                timestamp: e.timestamp,
                session_id: None,
                tools_involved: e.tools_used.clone(),
                confidence: self.log_entry_confidence(e),
            })
            .collect();
        Ok(observations)
    }

    fn collect_from_transcripts(&self, memory: &MemorySystem) -> anyhow::Result<Vec<RawObservation>> {
        let entries = memory.transcripts.recent_transcripts(self.config.lookback_days)?;
        let observations: Vec<RawObservation> = entries.iter()
            .filter(|e| self.is_significant_transcript(e))
            .map(|e| RawObservation {
                source: match e.role.as_str() {
                    "user" => ObservationSource::UserStatement,
                    "assistant" => ObservationSource::AgentInference,
                    _ => ObservationSource::ToolOutput,
                },
                content: e.content_preview.clone(),
                timestamp: chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                session_id: Some(e.session_id.clone()),
                tools_involved: e.tool_calls.clone(),
                confidence: 0.6,
            })
            .collect();
        Ok(observations)
    }

    fn collect_from_current_memory(&self, memory: &MemorySystem) -> anyhow::Result<Vec<RawObservation>> {
        let mut observations = Vec::new();
        let index_entries = memory.index.parse()?;
        for entry in index_entries {
            if let Some(content) = memory.topics.read(&entry.file_path)? {
                observations.push(RawObservation {
                    source: ObservationSource::AgentInference,
                    content: format!("[EXISTING MEMORY] {}/{}: {}", entry.topic, entry.subtopic, &content[..content.len().min(500)]),
                    timestamp: Utc::now(),
                    session_id: None,
                    tools_involved: vec![],
                    confidence: 0.8,
                });
            }
        }
        Ok(observations)
    }

    fn is_relevant_log_entry(&self, entry: &LogEntry) -> bool {
        matches!(entry.kind, LogEntryKind::Observation | LogEntryKind::Action | LogEntryKind::ActionResult | LogEntryKind::Error)
            && !entry.content.is_empty() && entry.content.len() > 20
    }

    fn log_entry_confidence(&self, entry: &LogEntry) -> f64 {
        match entry.kind {
            LogEntryKind::ActionResult => 0.9,
            LogEntryKind::Action => 0.8,
            LogEntryKind::Observation => 0.6,
            LogEntryKind::Error => 0.7,
            _ => 0.5,
        }
    }

    fn is_significant_transcript(&self, entry: &crate::memory::transcripts::TranscriptEntry) -> bool {
        entry.content_preview.len() > 50 && !entry.content_preview.starts_with("[no output]")
    }
}
