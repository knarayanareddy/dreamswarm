//! Authenticated loopback control channel between the API and a running daemon.
//!
//! On Unix the endpoint file is created with owner-only permissions; on other
//! platforms it inherits the state directory's access controls. An acknowledgement
//! means the daemon loop stopped and coordinator cleanup returned successfully,
//! not merely that a message was queued. A stale endpoint after a crash must be
//! removed manually.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Serialize, Deserialize)]
struct Endpoint {
    port: u16,
    token: String,
}

pub struct ControlServer {
    listener: TcpListener,
    token: String,
    path: PathBuf,
}

impl ControlServer {
    pub async fn bind(state_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let token = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let path = state_dir.join("control.json");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&path).context("Cannot create daemon control endpoint; another daemon may be running, or a stale control.json needs removal")?;
        use std::io::Write;
        let endpoint = Endpoint {
            port: listener.local_addr()?.port(),
            token: token.clone(),
        };
        let write_result = (|| -> Result<()> {
            serde_json::to_writer(&mut file, &endpoint)?;
            file.flush()?;
            Ok(())
        })();
        if let Err(error) = write_result {
            let _ = std::fs::remove_file(&path);
            return Err(error);
        }
        Ok(Self {
            listener,
            token,
            path,
        })
    }

    /// Ignore malformed or unauthenticated connections, rather than stopping.
    pub async fn accept_stop(&self) -> Result<TcpStream> {
        loop {
            let (stream, _) = self.listener.accept().await?;
            let mut reader = BufReader::new(stream).take(256);
            let mut line = Vec::new();
            let read =
                tokio::time::timeout(Duration::from_secs(2), reader.read_until(b'\n', &mut line))
                    .await;
            if matches!(read, Ok(Ok(n)) if n > 0)
                && line == format!("STOP {}\n", self.token).as_bytes()
            {
                return Ok(reader.into_inner().into_inner());
            }
        }
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn acknowledge(mut stream: TcpStream) -> Result<()> {
    stream.write_all(b"STOPPED\n").await?;
    stream.shutdown().await?;
    Ok(())
}

pub async fn request_stop(state_dir: &Path) -> Result<()> {
    let path = state_dir.join("control.json");
    let endpoint: Endpoint = serde_json::from_slice(
        &std::fs::read(&path).context("No daemon control endpoint available")?,
    )?;
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, endpoint.port)).await?;
        stream
            .write_all(format!("STOP {}\n", endpoint.token).as_bytes())
            .await?;
        let mut response = String::new();
        BufReader::new(stream)
            .take(256)
            .read_line(&mut response)
            .await?;
        if response.trim_end() != "STOPPED" {
            anyhow::bail!("Daemon did not acknowledge shutdown");
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("Timed out waiting for daemon shutdown")??;
    Ok(())
}
