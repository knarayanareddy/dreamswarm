use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub struct DailyLog {
    base_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub kind: LogEntryKind,
    pub content: String,
    pub session_id: Option<String>,
    pub tools_used: Vec<String>,
    pub tokens_consumed: u64,
    pub cost_usd: f64,
    pub trust_level: f64,
    pub signals_present: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LogEntryKind {
    Observation,
    Decision,
    Action,
    ActionResult,
    Error,
    Timeout,
    Dream,
    UserReturn,
    Startup,
    Shutdown,
    TrustChange,
}

impl DailyLog {
    pub fn new(state_dir: &PathBuf) -> anyhow::Result<Self> {
        let base_dir = state_dir.join("logs");
        std::fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    pub fn append(&self, entry: &LogEntry) -> anyhow::Result<()> {
        let today = entry.timestamp.format("%Y-%m-%d").to_string();
        let path = self.base_dir.join(format!("{}.jsonl", today));
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let line = serde_json::to_string(entry)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    pub fn log_observation(
        &self,
        content: &str,
        signals: Vec<String>,
        trust_level: f64,
        session_id: Option<String>,
    ) -> anyhow::Result<()> {
        self.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Observation,
            content: content.to_string(),
            session_id,
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level,
            signals_present: signals,
        })
    }

    pub fn log_decision(&self, content: &str, trust_level: f64, session_id: Option<String>) -> anyhow::Result<()> {
        self.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Decision,
            content: content.to_string(),
            session_id,
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level,
            signals_present: vec![],
        })
    }

    pub fn log_action(
        &self,
        content: &str,
        tools_used: Vec<String>,
        tokens: u64,
        cost: f64,
        trust_level: f64,
        session_id: Option<String>,
    ) -> anyhow::Result<()> {
        self.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Action,
            content: content.to_string(),
            session_id,
            tools_used,
            tokens_consumed: tokens,
            cost_usd: cost,
            trust_level,
            signals_present: vec![],
        })
    }

    pub fn log_error(&self, content: &str, trust_level: f64, session_id: Option<String>) -> anyhow::Result<()> {
        self.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Error,
            content: content.to_string(),
            session_id,
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level,
            signals_present: vec![],
        })
    }

    pub fn log_timeout(&self, content: &str, trust_level: f64, session_id: Option<String>) -> anyhow::Result<()> {
        self.append(&LogEntry {
            timestamp: Utc::now(),
            kind: LogEntryKind::Timeout,
            content: content.to_string(),
            session_id,
            tools_used: vec![],
            tokens_consumed: 0,
            cost_usd: 0.0,
            trust_level,
            signals_present: vec![],
        })
    }

    pub fn read_today(&self) -> anyhow::Result<Vec<LogEntry>> {
        self.read_date(&Utc::now().format("%Y-%m-%d").to_string())
    }

    pub fn read_date(&self, date: &str) -> anyhow::Result<Vec<LogEntry>> {
        let path = self.base_dir.join(format!("{}.jsonl", date));
        if !path.exists() {
            return Ok(vec![]);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect())
    }

    pub fn tokens_used_today(&self) -> anyhow::Result<u64> {
        let entries = self.read_today()?;
        Ok(entries.iter().map(|e| e.tokens_consumed).sum())
    }

    pub fn cost_today(&self) -> anyhow::Result<f64> {
        let entries = self.read_today()?;
        Ok(entries.iter().map(|e| e.cost_usd).sum())
    }

    pub fn actions_today(&self) -> anyhow::Result<u64> {
        let entries = self.read_today()?;
        Ok(entries
            .iter()
            .filter(|e| e.kind == LogEntryKind::Action)
            .count() as u64)
    }

    /// Read log entries from the last `days` calendar days (inclusive of today).
    pub fn read_recent_days(&self, days: usize) -> anyhow::Result<Vec<LogEntry>> {
        let mut all = Vec::new();
        for offset in 0..days {
            let date = (Utc::now() - chrono::Duration::days(offset as i64))
                .format("%Y-%m-%d")
                .to_string();
            all.extend(self.read_date(&date)?);
        }
        all.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        Ok(all)
    }
}
