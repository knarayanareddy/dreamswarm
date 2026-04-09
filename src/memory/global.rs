use crate::memory::index::MemoryIndex;
use crate::memory::topics::TopicStore;
use crate::memory::writer::MemoryWriter;
use std::path::PathBuf;

/// The Global Cognitive Substrate.
/// Shared across different repositories on the same machine.
pub struct GlobalMemoryStore {
    pub index: MemoryIndex,
    pub topics: TopicStore,
    pub writer: MemoryWriter,
    pub base_dir: PathBuf,
}

impl GlobalMemoryStore {
    pub fn new() -> anyhow::Result<Self> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let base_dir = home.join(".dreamswarm").join("global");
        let memory_dir = base_dir.join("memory");

        std::fs::create_dir_all(&memory_dir)?;
        let topics_dir = memory_dir.join("topics");
        std::fs::create_dir_all(&topics_dir)?;

        let index_path = memory_dir.join("MEMORY.md");
        let index = MemoryIndex::new(index_path.clone());
        let topics = TopicStore::new(topics_dir.clone());
        let writer = MemoryWriter::new(index_path, topics_dir);

        Ok(Self {
            index,
            topics,
            writer,
            base_dir,
        })
    }

    /// Shunts a local L1 pointer to the global registry.
    /// This allows patterns discovered in one repo to be "vacinnated" across the machine.
    pub fn shunt_pointer(&self, topic: &str, path_rel: &str, summary: &str) -> anyhow::Result<()> {
        self.index.upsert_pointer(topic, path_rel, summary)?;
        tracing::info!(
            "Global Cognitive Relay: Shunted pattern '{}' to shared substrate",
            topic
        );
        Ok(())
    }
}
