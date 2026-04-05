use crate::memory::index::MemoryIndex;
use crate::memory::topics::{Confidence, TopicStore};
use std::path::PathBuf;

pub struct MemoryWriter {
    index: MemoryIndex,
    topics: TopicStore,
    working_dir: Option<PathBuf>,
}

#[derive(Debug)]
pub struct WriteResult {
    pub stored: bool,
    pub reason: String,
    pub topic_path: String,
}

impl MemoryWriter {
    pub fn new(index_path: PathBuf, topics_dir: PathBuf) -> Self {
        Self {
            index: MemoryIndex::new(index_path),
            topics: TopicStore::new(topics_dir),
            working_dir: None,
        }
    }

    pub fn set_working_dir(&mut self, dir: PathBuf) {
        self.working_dir = Some(dir);
    }

    pub fn store(
        &self,
        topic: &str,
        subtopic: &str,
        content: &str,
        source: Option<&str>,
        confidence: Confidence,
    ) -> anyhow::Result<WriteResult> {
        // RULE 1: Check if this is derivable from the codebase
        if self.is_derivable(content) {
            return Ok(WriteResult {
                stored: false,
                reason: "Content can be re-derived from the codebase. Not storing.".to_string(),
                topic_path: format!("{}/{}.md", topic, subtopic),
            });
        }

        // RULE 2: Content must be non-trivial
        if content.trim().len() < 10 {
            return Ok(WriteResult {
                stored: false,
                reason: "Content too short to be useful.".to_string(),
                topic_path: format!("{}/{}.md", topic, subtopic),
            });
        }

        // RULE 3: Write to topic file FIRST
        let topic_path = format!(
            "{}/{}.md",
            Self::sanitize_path(topic),
            Self::sanitize_path(subtopic)
        );
        self.topics
            .append(&topic_path, content, source, confidence)?;

        // RULE 4: Update the index with a POINTER
        let description = Self::summarize_for_index(content);
        self.index
            .upsert_pointer(topic, &topic_path, &description)?;

        Ok(WriteResult {
            stored: true,
            reason: "Successfully stored.".to_string(),
            topic_path,
        })
    }

    pub fn store_batch(
        &self,
        entries: &[(String, String, String, Option<String>, Confidence)],
    ) -> anyhow::Result<Vec<WriteResult>> {
        entries
            .iter()
            .map(|(topic, subtopic, content, source, confidence)| {
                self.store(
                    topic,
                    subtopic,
                    content,
                    source.as_deref(),
                    confidence.clone(),
                )
            })
            .collect()
    }

    pub fn remove(&self, topic: &str, subtopic: &str) -> anyhow::Result<bool> {
        let topic_path = format!(
            "{}/{}.md",
            Self::sanitize_path(topic),
            Self::sanitize_path(subtopic)
        );
        self.index.remove_pointer(&topic_path)?;
        self.topics.delete(&topic_path)?;
        Ok(true)
    }

    fn is_derivable(&self, content: &str) -> bool {
        let code_indicators = [
            "fn ",
            "pub fn",
            "def ",
            "class ",
            "function ",
            "const ",
            "let ",
            "import ",
            "from ",
            "require(",
            "#include",
            "package ",
        ];
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return false;
        }

        let code_lines = lines
            .iter()
            .filter(|line| {
                let trimmed = line.trim();
                code_indicators.iter().any(|ind| trimmed.starts_with(ind))
                    || trimmed.starts_with("//")
                    || trimmed.starts_with("#")
                    || trimmed.starts_with("/*")
                    || (trimmed.contains('{') && trimmed.contains('}'))
            })
            .count();

        let ratio = code_lines as f64 / lines.len() as f64;
        ratio > 0.6
    }

    fn summarize_for_index(content: &str) -> String {
        let max_len = 120;
        let first_line = content
            .lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("---"))
            .unwrap_or("(no description)");

        if first_line.len() <= max_len {
            first_line.to_string()
        } else {
            format!("{}...", &first_line[..max_len - 3])
        }
    }

    fn sanitize_path(name: &str) -> String {
        name.to_lowercase()
            .replace([' ', '/', '\\'], "-")
            .replace("..", "")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
            .collect()
    }
}
