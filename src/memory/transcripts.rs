use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TranscriptStore {
    base_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub session_id: String,
    pub turn_number: u64,
    pub role: String,
    pub content_preview: String,
    pub tool_calls: Vec<String>,
    pub tokens: u64,
    pub timestamp: String,
}

impl TranscriptStore {
    pub fn new(base_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&base_dir).ok();
        Self { base_dir }
    }

    pub fn archive_turn(
        &self,
        session_id: &str,
        turn_number: u64,
        role: &str,
        content: &str,
        tool_calls: &[String],
        tokens: u64,
    ) -> anyhow::Result<()> {
        let date = Utc::now().format("%Y-%m-%d");
        let session_prefix = if session_id.len() >= 8 {
            &session_id[..8]
        } else {
            session_id
        };
        let filename = format!("{}-{}.jsonl", date, session_prefix);
        let path = self.base_dir.join(&filename);

        let preview = if content.len() > 500 {
            format!("{}...", &content[..500])
        } else {
            content.to_string()
        };

        let entry = TranscriptEntry {
            session_id: session_id.to_string(),
            turn_number,
            role: role.to_string(),
            content_preview: preview,
            tool_calls: tool_calls.to_vec(),
            tokens,
            timestamp: Utc::now().to_rfc3339(),
        };

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        let line = serde_json::to_string(&entry)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    pub fn list_transcripts(&self) -> anyhow::Result<Vec<PathBuf>> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.base_dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
            .collect();

        files.sort_by(|a, b| b.cmp(a)); // Most recent first
        Ok(files)
    }

    pub fn read_transcript(&self, path: &PathBuf) -> anyhow::Result<Vec<TranscriptEntry>> {
        let content = std::fs::read_to_string(path)?;
        let entries: Vec<TranscriptEntry> = content
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        Ok(entries)
    }

    pub fn recent_transcripts(&self, days: u32) -> anyhow::Result<Vec<TranscriptEntry>> {
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
        let mut all_entries = Vec::new();

        for path in self.list_transcripts()? {
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if filename < cutoff_str.as_str() {
                break;
            }
            all_entries.extend(self.read_transcript(&path)?);
        }
        Ok(all_entries)
    }

    pub fn cleanup(&self, max_age_days: u32) -> anyhow::Result<u32> {
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
        let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
        let mut removed = 0;

        for path in self.list_transcripts()? {
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if filename < cutoff_str.as_str() {
                std::fs::remove_file(&path)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}
