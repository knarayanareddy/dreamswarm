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

    /// Returns the path to the root memory directory.
    pub fn memory_dir(&self) -> &PathBuf {
        &self.memory_dir
    }
}
