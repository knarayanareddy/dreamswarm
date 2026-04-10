use opendal::{services::S3, Operator};
use std::path::PathBuf;
use tokio::fs;

pub struct S3Relay {
    op: Operator,
    local_dir: PathBuf,
}

impl S3Relay {
    pub async fn new(
        endpoint: &str,
        bucket: &str,
        region: &str,
        access_key: &str,
        secret_key: &str,
        local_dir: PathBuf,
    ) -> anyhow::Result<Self> {
        // Configure the S3 service using OpenDAL to provide 
        // a backend-agnostic object storage interface.
        let builder = S3::default()
            .endpoint(endpoint)
            .bucket(bucket)
            .region(region)
            .access_key_id(access_key)
            .secret_access_key(secret_key);

        let op = Operator::new(builder)?.finish();
        Ok(Self { op, local_dir })
    }

    /// Synchronizes the local memory substrate to the S3 bucket (Upstream Sync).
    pub async fn sync_up(&self) -> anyhow::Result<()> {
        let topics_dir = self.local_dir.join("topics");
        if !topics_dir.exists() {
            return Ok(());
        }

        let mut entries = fs::read_dir(topics_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                let name = entry.file_name().to_string_lossy().to_string();
                let content = fs::read(&path).await?;
                // Perform near-real-time push of topic fragments
                self.op.write(&format!("topics/{}", name), content).await?;
            }
        }
        
        // Push the core Memory Index (Layer 1)
        let index_path = self.local_dir.join("MEMORY.md");
        if index_path.exists() {
            let content = fs::read(&index_path).await?;
            self.op.write("MEMORY.md", content).await?;
        }

        tracing::info!("Federated Relay: Successfully synced knowledge to the hive.");
        Ok(())
    }

    /// Fetches the latest global substrate from the S3 bucket (Downstream Sync).
    pub async fn sync_down(&self) -> anyhow::Result<()> {
        // Phase 7 Downstream logic:
        // This will eventually perform a diff-based pull to keep the local machine
        // "vaccinated" with patterns discovered by other nodes in the hive.
        Ok(())
    }
}
