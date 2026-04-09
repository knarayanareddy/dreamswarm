use crate::query::engine::QueryEngine;

pub struct Autopilot;

impl Autopilot {
    /// Discovers the top-3 most relevant L3 Themes for a given task description.
    pub async fn get_relevant_themes(
        query: &str,
        memory: &crate::memory::MemorySystem,
        _query_engine: &QueryEngine,
    ) -> anyhow::Result<Vec<String>> {
        let themes_dir = memory.memory_dir().join("themes");
        if !themes_dir.exists() {
            return Ok(vec![]);
        }

        // 1. Search Memory Index for relevant themes
        let relevant_entries = memory.index.find_relevant(query)?;

        // 2. Filter for L3 theme pointers
        let mut theme_paths = Vec::new();
        for entry in relevant_entries {
            if entry.file_path.starts_with("themes/") {
                let full_path = memory.memory_dir().join(&entry.file_path);
                if full_path.exists() {
                    theme_paths.push(full_path);
                }
            }
            if theme_paths.len() >= 3 {
                break;
            }
        }

        // 3. Read and return contents
        let mut contents = Vec::new();
        for path in theme_paths {
            if let Ok(content) = std::fs::read_to_string(path) {
                contents.push(content);
            }
        }

        Ok(contents)
    }

    /// Enriches a task description with L3 architectural context.
    pub fn enrich_task_with_context(description: &str, themes: &[String]) -> String {
        if themes.is_empty() {
            return description.to_string();
        }

        let mut enriched = format!(
            "{}\n\n---\n## 🧠 AUTHOPILOT: ARCHITECTURAL CONTEXT (L3)\n",
            description
        );
        for (i, theme) in themes.iter().enumerate() {
            enriched.push_str(&format!("\n### Perspective {}\n{}\n", i + 1, theme));
        }
        enriched.push_str("\n---\n");
        enriched
    }
}
