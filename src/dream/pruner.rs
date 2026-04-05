use crate::dream::{MemoryOperation, OperationKind, PruneReason};
use crate::memory::MemorySystem;
use std::path::PathBuf;

pub struct MemoryPruner {
    working_dir: PathBuf,
    _confidence_threshold: f64,
}

impl MemoryPruner {
    pub fn new(working_dir: PathBuf, confidence_threshold: f64) -> Self {
        Self {
            working_dir,
            _confidence_threshold: confidence_threshold,
        }
    }

    pub fn analyze(&self, memory: &MemorySystem) -> anyhow::Result<Vec<MemoryOperation>> {
        let mut prune_ops = Vec::new();
        let index_entries = memory.index.parse()?;
        for entry in index_entries {
            if let Some(content) = memory.topics.read(&entry.file_path)? {
                let content: String = content;
                if self.is_derivable_content(&content) {
                    prune_ops.push(MemoryOperation {
                        kind: OperationKind::Prune {
                            reason: PruneReason::Derivable,
                        },
                        topic: entry.topic.clone(),
                        subtopic: entry.subtopic.clone(),
                        content: String::new(),
                        reasoning: "Describes code structure that can be re-derived from source"
                            .to_string(),
                        confidence: 0.0,
                    });
                    continue;
                }
                if let Some(source_ref) = self.extract_source_reference(&content) {
                    let source_path = self.working_dir.join(&source_ref);
                    if !source_path.exists() {
                        prune_ops.push(MemoryOperation {
                            kind: OperationKind::Prune {
                                reason: PruneReason::Stale,
                            },
                            topic: entry.topic.clone(),
                            subtopic: entry.subtopic.clone(),
                            content: String::new(),
                            reasoning: format!(
                                "Referenced source file '{}' no longer exists",
                                source_ref
                            ),
                            confidence: 0.0,
                        });
                    }
                }
            }
        }
        tracing::info!("Pruner found {} entries to prune", prune_ops.len());
        Ok(prune_ops)
    }

    fn is_derivable_content(&self, content: &str) -> bool {
        let indicators = [
            "fn ", "pub fn ", "struct ", "enum ", "impl ", "trait ", "class ", "def ", "let ",
            "const ",
        ];
        let lines: Vec<&str> = content
            .lines()
            .filter(|l| {
                let t = l.trim();
                !t.is_empty() && !t.starts_with('#') && !t.starts_with("---")
            })
            .collect();
        if lines.is_empty() {
            return false;
        }
        let code_lines = lines
            .iter()
            .filter(|l| indicators.iter().any(|ind| l.contains(ind)))
            .count();
        code_lines as f64 / lines.len() as f64 > 0.6
    }

    fn extract_source_reference(&self, content: &str) -> Option<String> {
        for line in content.lines() {
            if line.to_lowercase().contains("source:") {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() >= 2 {
                    let path = parts[1].trim().split(':').next().unwrap_or("").trim();
                    if !path.is_empty() && path.contains('/') {
                        return Some(path.to_string());
                    }
                }
            }
        }
        None
    }
}
