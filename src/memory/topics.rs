use chrono::Utc;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TopicStore {
    base_dir: PathBuf,
    archive_dir: PathBuf,
    max_entry_size: usize,
    max_file_size: usize,
}

#[derive(Debug, Clone)]
pub struct TopicEntry {
    pub timestamp: String,
    pub content: String,
    pub source: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Confidence {
    Verified,
    Observed,
    Inferred,
    Stale,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Confidence::Verified => write!(f, "✅ verified"),
            Confidence::Observed => write!(f, "👁 observed"),
            Confidence::Inferred => write!(f, "🔮 inferred"),
            Confidence::Stale => write!(f, "⚠ stale"),
        }
    }
}

impl TopicStore {
    pub fn new(base_dir: PathBuf) -> Self {
        let archive_dir = base_dir.parent().unwrap_or(&base_dir).join("archive");
        let _ = std::fs::create_dir_all(&archive_dir);
        Self {
            base_dir,
            archive_dir,
            max_entry_size: 2000,
            max_file_size: 50_000,
        }
    }

    /// Prunes files that haven't been touched in `older_than` days.
    /// If `Verified`, they are kept. Otherwise, they are archived.
    pub fn apply_decay(&self, older_than_days: u64) -> anyhow::Result<usize> {
        let mut decayed_count = 0;
        let files = self.list_all()?;
        let now = std::time::SystemTime::now();
        let seconds_in_day = 86400;

        for rel_path in files {
            let path = self.base_dir.join(&rel_path);
            let metadata = std::fs::metadata(&path)?;
            let modified = metadata.modified()?;
            let age = now.duration_since(modified)?.as_secs();

            if age > older_than_days * seconds_in_day {
                let content = std::fs::read_to_string(&path)?;
                // Only decay if not explicitly [✅ verified]
                if !content.contains("✅ verified") {
                    let archive_path = self.archive_dir.join(&rel_path);
                    if let Some(parent) = archive_path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(&path, &archive_path)?;
                    decayed_count += 1;
                    tracing::info!("Decayed topic to archive: {}", rel_path);
                }
            }
        }
        Ok(decayed_count)
    }

    pub fn read(&self, topic_path: &str) -> anyhow::Result<Option<String>> {
        let path = self.base_dir.join(topic_path);
        if !path.exists() {
            return Ok(None);
        }

        let canonical = path.canonicalize()?;
        let base_canonical = self.base_dir.canonicalize()?;
        if !canonical.starts_with(&base_canonical) {
            anyhow::bail!("Path traversal detected: {}", topic_path);
        }

        Ok(Some(std::fs::read_to_string(&path)?))
    }

    pub fn append(
        &self,
        topic_path: &str,
        content: &str,
        source: Option<&str>,
        confidence: Confidence,
    ) -> anyhow::Result<()> {
        let path = self.base_dir.join(topic_path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if path.exists() {
            let current_size = std::fs::metadata(&path)?.len() as usize;
            if current_size >= self.max_file_size {
                anyhow::bail!(
                    "Topic file '{}' is at capacity ({} bytes). Run memory consolidation.",
                    topic_path,
                    current_size
                );
            }
        }

        let truncated_content = if content.len() > self.max_entry_size {
            format!("{}... [truncated]", &content[..self.max_entry_size])
        } else {
            content.to_string()
        };

        let timestamp = Utc::now().format("%Y-%m-%d %H:%M UTC");
        let mut entry = format!("\n---\n_[{} | {}]_\n", timestamp, confidence);

        if let Some(src) = source {
            entry.push_str(&format!("_Source: {}_\n", src));
        }

        entry.push_str(&truncated_content);
        entry.push('\n');

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;

        if file.metadata()?.len() == 0 {
            let title = topic_path
                .trim_end_matches(".md")
                .split('/')
                .next_back()
                .unwrap_or("Unknown");
            writeln!(file, "# Memory: {}\n", title)?;
        }

        write!(file, "{}", entry)?;
        Ok(())
    }

    pub fn list_all(&self) -> anyhow::Result<Vec<String>> {
        let mut files = Vec::new();
        self.walk_dir(&self.base_dir, &self.base_dir, &mut files)?;
        files.sort();
        Ok(files)
    }

    fn walk_dir(
        &self,
        current: &PathBuf,
        base: &PathBuf,
        files: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        if !current.is_dir() {
            return Ok(());
        }


        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                self.walk_dir(&path, base, files)?;
            } else if path.extension().is_some_and(|e| e == "md") {
                let relative = path
                    .strip_prefix(base)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                files.push(relative);
            }
        }
        Ok(())
    }

    pub fn delete(&self, topic_path: &str) -> anyhow::Result<bool> {
        let path = self.base_dir.join(topic_path);
        if path.exists() {
            std::fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn estimate_tokens(&self, topic_path: &str) -> anyhow::Result<usize> {
        match self.read(topic_path)? {
            Some(content) => Ok(content.len() / 4),
            None => Ok(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_temporal_decay() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let topics_dir = dir.path().join("topics");
        let store = TopicStore::new(topics_dir);

        // 1. Create a fresh topic
        store.append("active.md", "This is fresh", None, Confidence::Observed)?;

        // 2. Create an old topic (unverified)
        let old_path = store.base_dir.join("old_stale.md");
        store.append("old_stale.md", "This is old", None, Confidence::Observed)?;
        
        // Manipulate mtime to 20 days ago
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(20 * 86400);
        filetime::set_file_mtime(&old_path, filetime::FileTime::from_system_time(old_time))?;

        // 3. Create an old topic (verified - should be kept)
        let verified_path = store.base_dir.join("old_verified.md");
        store.append("old_verified.md", "This is verified ✅ verified", None, Confidence::Verified)?;
        filetime::set_file_mtime(&verified_path, filetime::FileTime::from_system_time(old_time))?;

        // Run decay (14 day threshold)
        let decayed = store.apply_decay(14)?;
        
        assert_eq!(decayed, 1);
        assert!(!old_path.exists());
        assert!(store.archive_dir.join("old_stale.md").exists());
        assert!(verified_path.exists());
        assert!(store.base_dir.join("active.md").exists());

        Ok(())
    }
}
