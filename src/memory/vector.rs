use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};
use ndarray::Array1;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VectorEntry {
    pub id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub metadata: serde_json::Value,
}

pub struct VectorStore {
    model: TextEmbedding,
    path: PathBuf,
    entries: Vec<VectorEntry>,
}

impl VectorStore {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        info!("Initializing VectorStore with fastembed (BGE-Small)...");
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(true)
        )?;

        let entries = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            serde_json::from_str(&data).unwrap_or_else(|_| vec![])
        } else {
            vec![]
        };

        Ok(Self {
            model,
            path,
            entries,
        })
    }

    pub fn add(&mut self, id: String, text: String, metadata: serde_json::Value) -> anyhow::Result<()> {
        info!("Generating embedding for entry: {}", id);
        let embeddings = self.model.embed(vec![text.clone()], None)?;
        
        if let Some(embedding) = embeddings.first() {
            self.entries.push(VectorEntry {
                id,
                text,
                embedding: embedding.clone(),
                metadata,
            });
            self.save()?;
        }
        
        Ok(())
    }

    pub fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<(VectorEntry, f32)>> {
        info!("Searching vector space for: '{}'", query);
        let query_embeddings = self.model.embed(vec![query], None)?;
        let query_vec = match query_embeddings.first() {
            Some(v) => Array1::from_vec(v.clone()),
            None => return Ok(vec![]),
        };

        let mut results: Vec<(VectorEntry, f32)> = self.entries.iter().map(|entry| {
            let entry_vec = Array1::from_vec(entry.embedding.clone());
            let similarity = self.cosine_similarity(&query_vec, &entry_vec);
            (entry.clone(), similarity)
        }).collect();

        // Sort by similarity descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        
        Ok(results.into_iter().take(limit).collect())
    }

    fn cosine_similarity(&self, a: &Array1<f32>, b: &Array1<f32>) -> f32 {
        let dot = a.dot(b);
        let norm_a = a.dot(a).sqrt();
        let norm_b = b.dot(b).sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }

    fn save(&self) -> anyhow::Result<()> {
        let data = serde_json::to_string_pretty(&self.entries)?;
        std::fs::write(&self.path, data)?;
        Ok(())
    }
}
