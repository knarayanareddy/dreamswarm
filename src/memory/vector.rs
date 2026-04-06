use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use ndarray::Array1;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::info;

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
        Self::ensure_ort_lib_path();
        info!("Initializing VectorStore with fastembed (BGE-Small)...");
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(true),
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

    pub fn add(
        &mut self,
        id: String,
        text: String,
        metadata: serde_json::Value,
    ) -> anyhow::Result<()> {
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

    pub fn search(&mut self, query: &str, limit: usize) -> anyhow::Result<Vec<(VectorEntry, f32)>> {
        info!("Searching vector space for: '{}'", query);
        let query_embeddings = self.model.embed(vec![query], None)?;
        let query_vec = match query_embeddings.first() {
            Some(v) => Array1::from_vec(v.clone()),
            None => return Ok(vec![]),
        };

        let mut results: Vec<(VectorEntry, f32)> = self
            .entries
            .iter()
            .map(|entry| {
                let entry_vec = Array1::from_vec(entry.embedding.clone());
                let similarity = self.cosine_similarity(&query_vec, &entry_vec);
                (entry.clone(), similarity)
            })
            .collect();

        // Sort by similarity descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results.into_iter().take(limit).collect())
    }

    fn ensure_ort_lib_path() {
        if std::env::var("ORT_DYLIB_PATH").is_ok() {
            return;
        }

        let target_dir = std::path::PathBuf::from("target");
        if !target_dir.exists() {
            return;
        }

        let pattern = if cfg!(target_os = "windows") {
            "onnxruntime.dll"
        } else if cfg!(target_os = "macos") {
            "libonnxruntime.dylib"
        } else {
            "libonnxruntime.so"
        };

        fn find_file(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(found) = find_file(&path, name) {
                            return Some(found);
                        }
                    } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                        return Some(path);
                    }
                }
            }
            None
        }

        if let Some(path) = find_file(&target_dir, pattern) {
            let path_str = path.to_string_lossy().into_owned();
            info!("Self-Healing: Found ONNX Runtime at {}", path_str);
            std::env::set_var("ORT_DYLIB_PATH", path_str);
        }
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
