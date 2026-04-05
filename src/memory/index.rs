use chrono::Utc;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MemoryIndex {
    path: PathBuf,
    max_line_length: usize,
    max_total_lines: usize,
}

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub topic: String,
    pub subtopic: String,
    pub file_path: String,
    pub description: String,
}

impl MemoryIndex {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            max_line_length: 150,
            max_total_lines: 200,
        }
    }

    pub fn load_raw(&self) -> anyhow::Result<String> {
        if !self.path.exists() {
            return Ok(String::from(
                "# DreamSwarm Memory Index\n\n_No memories stored yet._\n",
            ));
        }
        Ok(std::fs::read_to_string(&self.path)?)
    }

    pub fn estimate_tokens(&self) -> anyhow::Result<usize> {
        let content = self.load_raw()?;
        Ok(content.len() / 4)
    }

    pub fn parse(&self) -> anyhow::Result<Vec<IndexEntry>> {
        let content = self.load_raw()?;
        let mut entries = Vec::new();
        let mut current_topic = String::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("## ") {
                current_topic = trimmed[3..].trim().to_string();
                continue;
            }
            if trimmed.starts_with("→ ") || trimmed.starts_with("-> ") {
                let rest = if trimmed.starts_with("→ ") {
                    &trimmed[4..]
                } else {
                    &trimmed[3..]
                };
                if let Some(dash_pos) = rest.find(" — ") {
                    let file_path = rest[..dash_pos].trim().to_string();
                    let description = rest[dash_pos + 5..].trim().to_string();
                    let subtopic = file_path
                        .split('/')
                        .next_back()
                        .unwrap_or(&file_path)
                        .trim_end_matches(".md")
                        .to_string();
                    entries.push(IndexEntry {
                        topic: current_topic.clone(),
                        subtopic,
                        file_path,
                        description,
                    });
                } else if let Some(dash_pos) = rest.find(" - ") {
                    let file_path = rest[..dash_pos].trim().to_string();
                    let description = rest[dash_pos + 3..].trim().to_string();
                    let subtopic = file_path
                        .split('/')
                        .next_back()
                        .unwrap_or(&file_path)
                        .trim_end_matches(".md")
                        .to_string();
                    entries.push(IndexEntry {
                        topic: current_topic.clone(),
                        subtopic,
                        file_path,
                        description,
                    });
                }
            }
        }
        Ok(entries)
    }

    pub fn upsert_pointer(
        &self,
        topic: &str,
        file_path: &str,
        description: &str,
    ) -> anyhow::Result<()> {
        let pointer_line = format!("→ {} — {}", file_path, description);
        let truncated = if pointer_line.len() > self.max_line_length {
            format!("{}...", &pointer_line[..self.max_line_length - 3])
        } else {
            pointer_line
        };

        let mut topics: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if self.path.exists() {
            let content = std::fs::read_to_string(&self.path)?;
            let mut current_topic = String::new();
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("## ") {
                    current_topic = trimmed[3..].trim().to_string();
                    topics.entry(current_topic.clone()).or_default();
                } else if trimmed.starts_with("→ ") || trimmed.starts_with("-> ") {
                    let is_same_file = trimmed.contains(file_path);
                    if !is_same_file {
                        topics
                            .entry(current_topic.clone())
                            .or_default()
                            .push(trimmed.to_string());
                    }
                }
            }
        }
        topics.entry(topic.to_string()).or_default().push(truncated);
        self.write_index(&topics)?;
        Ok(())
    }

    pub fn remove_pointer(&self, file_path: &str) -> anyhow::Result<bool> {
        if !self.path.exists() {
            return Ok(false);
        }
        let content = std::fs::read_to_string(&self.path)?;
        let mut topics: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut current_topic = String::new();
        let mut found = false;

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("## ") {
                current_topic = trimmed[3..].trim().to_string();
                topics.entry(current_topic.clone()).or_default();
            } else if trimmed.starts_with("→ ") || trimmed.starts_with("-> ") {
                if trimmed.contains(file_path) {
                    found = true;
                } else {
                    topics
                        .entry(current_topic.clone())
                        .or_default()
                        .push(trimmed.to_string());
                }
            }
        }
        if found {
            self.write_index(&topics)?;
        }
        Ok(found)
    }

    pub fn list_topics(&self) -> anyhow::Result<Vec<String>> {
        let entries = self.parse()?;
        let mut topics: Vec<String> = entries
            .iter()
            .map(|e| e.topic.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        topics.sort();
        Ok(topics)
    }

    pub fn find_relevant(&self, query: &str) -> anyhow::Result<Vec<IndexEntry>> {
        let entries = self.parse()?;
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();

        let mut scored: Vec<(IndexEntry, usize)> = entries
            .into_iter()
            .filter_map(|entry| {
                let haystack = format!("{} {} {}", entry.topic, entry.subtopic, entry.description)
                    .to_lowercase();
                let score: usize = query_words
                    .iter()
                    .filter(|word| haystack.contains(*word))
                    .count();
                if score > 0 {
                    Some((entry, score))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        Ok(scored.into_iter().map(|(entry, _)| entry).collect())
    }

    fn write_index(&self, topics: &BTreeMap<String, Vec<String>>) -> anyhow::Result<()> {
        let mut content = String::from("# DreamSwarm Memory Index\n");
        content.push_str(&format!(
            "_Last updated: {}_\n",
            Utc::now().format("%Y-%m-%d %H:%M UTC")
        ));
        let mut total_lines = 0;

        for (topic, pointers) in topics {
            if pointers.is_empty() {
                continue;
            }
            content.push_str(&format!("\n## {}\n", topic));
            total_lines += 1;
            for pointer in pointers {
                if total_lines >= self.max_total_lines {
                    content.push_str("\n_⚠ Memory index at capacity. Run memory consolidation._\n");
                    break;
                }
                content.push_str(&format!("{}\n", pointer));
                total_lines += 1;
            }
        }
        std::fs::write(&self.path, content)?;
        Ok(())
    }
}
