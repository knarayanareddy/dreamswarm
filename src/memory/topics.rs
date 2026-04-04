use chrono::Utc;
use std::path::PathBuf;
use std::io::Write;

#[derive(Debug, Clone)]
pub struct TopicStore {
    base_dir: PathBuf,
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
        Self {
            base_dir,
            max_entry_size: 2000,
            max_file_size: 50_000,
        }
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
                .last()
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
            } else if path.extension().map_or(false, |e| e == "md") {
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
