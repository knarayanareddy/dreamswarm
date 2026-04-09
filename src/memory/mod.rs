pub mod index;
pub mod loader;
pub mod search;
pub mod topics;
pub mod transcripts;
pub mod vector;
pub mod writer;

use std::path::PathBuf;

/// The unified 3-layer memory system.
///
/// Layer 1: `index`  — always-loaded pointers (~150 chars/line)
/// Layer 2: `topics` — on-demand content files
/// Layer 3: `transcripts` — archived turns, never loaded directly
pub struct MemorySystem {
    pub index: index::MemoryIndex,
    pub topics: topics::TopicStore,
    pub transcripts: transcripts::TranscriptStore,
    pub writer: writer::MemoryWriter,
    pub search: search::MemorySearch,
    pub loader: loader::MemoryLoader,
    memory_dir: PathBuf,
}

impl MemorySystem {
    pub fn new(memory_dir: PathBuf) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&memory_dir)?;
        std::fs::create_dir_all(memory_dir.join("topics"))?;
        std::fs::create_dir_all(memory_dir.join("transcripts"))?;
        std::fs::create_dir_all(memory_dir.join("conflicts"))?;

        let index = index::MemoryIndex::new(memory_dir.join("MEMORY.md"));
        let topics = topics::TopicStore::new(memory_dir.join("topics"));
        let transcripts = transcripts::TranscriptStore::new(memory_dir.join("transcripts"));
        let search = search::MemorySearch::new(memory_dir.clone());
        let loader = loader::MemoryLoader::new(memory_dir.clone());
        let writer =
            writer::MemoryWriter::new(memory_dir.join("MEMORY.md"), memory_dir.join("topics"));

        Ok(Self {
            index,
            topics,
            transcripts,
            writer,
            search,
            loader,
            memory_dir,
        })
    }

    /// Triggers temporal decay across the topic store.
    pub fn manage_decay(&self, older_than_days: u64) -> anyhow::Result<usize> {
        self.topics.apply_decay(older_than_days)
    }

    /// Returns the path to the root memory directory.
    pub fn memory_dir(&self) -> &PathBuf {
        &self.memory_dir
    }

    /// Resolves a knowledge conflict.
    pub fn resolve_conflict(
        &self,
        ticket_id: &str,
        action: ConflictResolution,
        content: Option<&str>,
    ) -> anyhow::Result<()> {
        let conflicts_dir = self.memory_dir.join("conflicts");
        let resolved_dir = conflicts_dir.join("resolved");
        let _ = std::fs::create_dir_all(&resolved_dir);

        let ticket_path = conflicts_dir.join(ticket_id);
        if !ticket_path.exists() {
            anyhow::bail!("Conflict ticket not found: {}", ticket_id);
        }

        if action == ConflictResolution::AcceptProposed {
            if let Some(_c) = content {
                // Extract topic path from ticket id or pass it in.
                // Simplified: we'll use a placeholder logic or assume the TUI knows the path.
                // For now, we'll just log and move. The TUI will handle topic updates via writer.
            }
        }

        // Archive the ticket
        let archive_path = resolved_dir.join(ticket_id);
        std::fs::rename(ticket_path, archive_path)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConflictResolution {
    AcceptProposed,
    KeepExisting,
    Archive,
}
