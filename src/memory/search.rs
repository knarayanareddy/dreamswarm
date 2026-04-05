use crate::memory::index::MemoryIndex;
use crate::memory::topics::TopicStore;
use std::path::PathBuf;

pub struct MemorySearch {
    index: MemoryIndex,
    topics: TopicStore,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub topic_path: String,
    pub topic_name: String,
    pub snippet: String,
    pub relevance: f64,
    pub line_number: usize,
}

impl MemorySearch {
    pub fn new(memory_dir: PathBuf) -> Self {
        Self {
            index: MemoryIndex::new(memory_dir.join("MEMORY.md")),
            topics: TopicStore::new(memory_dir.join("topics")),
        }
    }

    pub fn search(&self, query: &str, max_results: usize) -> anyhow::Result<Vec<SearchResult>> {
        let mut results = Vec::new();

        // Phase 1: Find relevant index entries
        let index_matches = self.index.find_relevant(query)?;

        // Phase 2: Search within matched topic files
        for entry in &index_matches {
            if let Some(content) = self.topics.read(&entry.file_path)? {
                let file_results =
                    self.search_in_content(&entry.file_path, &entry.topic, &content, query);
                results.extend(file_results);
            }
        }

        // Phase 3: Broader search if few results
        if results.len() < max_results {
            let all_files = self.topics.list_all()?;
            let searched_paths: Vec<&str> =
                index_matches.iter().map(|e| e.file_path.as_str()).collect();

            for file_path in &all_files {
                if searched_paths.contains(&file_path.as_str()) {
                    continue;
                }
                if let Some(content) = self.topics.read(file_path)? {
                    let file_results = self.search_in_content(
                        file_path,
                        file_path.split('/').next().unwrap_or("Unknown"),
                        &content,
                        query,
                    );
                    results.extend(file_results);
                }
            }
        }

        // Sort by relevance
        results.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
        results.truncate(max_results);

        Ok(results)
    }

    fn search_in_content(
        &self,
        topic_path: &str,
        topic_name: &str,
        content: &str,
        query: &str,
    ) -> Vec<SearchResult> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        let mut results = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line_lower = line.to_lowercase();
            let matching_words: usize = query_words
                .iter()
                .filter(|word| line_lower.contains(*word))
                .count();

            if matching_words > 0 {
                let relevance = matching_words as f64 / query_words.len() as f64;
                let snippet = self.build_snippet(content, line_num, 1);

                results.push(SearchResult {
                    topic_path: topic_path.to_string(),
                    topic_name: topic_name.to_string(),
                    snippet,
                    relevance,
                    line_number: line_num + 1,
                });
            }
        }

        results
    }

    fn build_snippet(&self, content: &str, target_line: usize, context: usize) -> String {
        let lines: Vec<&str> = content.lines().collect();
        let start = target_line.saturating_sub(context);
        let end = (target_line + context + 1).min(lines.len());

        lines[start..end]
            .iter()
            .enumerate()
            .map(|(i, line)| {
                if start + i == target_line {
                    format!(">>> {}", line)
                } else {
                    format!("    {}", line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}
