//! Cross-process advisory file locking.
//!
//! Workers run as separate processes (tmux panes, SSH sessions), so an
//! in-process `Mutex` is not enough to serialise mailbox and task-list writes.
//! These helpers take an exclusive `flock` on a sidecar lock file and release it
//! on drop (including on panic).

use std::fs::File;
use std::path::{Path, PathBuf};

pub struct FileLock {
    file: File,
    path: PathBuf,
}

impl FileLock {
    /// Acquires an exclusive lock, creating the lock file if needed.
    pub fn acquire(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        lock_exclusive(&file)?;
        Ok(Self { file, path })
    }

    /// Acquires the lock for `target` by placing `<target>.lock` beside it.
    pub fn for_target(target: impl AsRef<Path>) -> anyhow::Result<Self> {
        let mut lock_path = target.as_ref().as_os_str().to_owned();
        lock_path.push(".lock");
        Self::acquire(PathBuf::from(lock_path))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        unlock(&self.file);
    }
}

#[cfg(unix)]
fn lock_exclusive(file: &File) -> anyhow::Result<()> {
    use std::os::unix::io::AsRawFd;
    // SAFETY: `flock` only inspects the descriptor, which stays open for the
    // lifetime of the guard.
    loop {
        let rc = unsafe { libc_flock(file.as_raw_fd(), LOCK_EX) };
        if rc == 0 {
            return Ok(());
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(anyhow::anyhow!("could not acquire file lock: {}", err));
        }
    }
}

#[cfg(unix)]
fn unlock(file: &File) {
    use std::os::unix::io::AsRawFd;
    unsafe {
        libc_flock(file.as_raw_fd(), LOCK_UN);
    }
}

#[cfg(unix)]
const LOCK_EX: i32 = 2;
#[cfg(unix)]
const LOCK_UN: i32 = 8;

#[cfg(unix)]
extern "C" {
    #[link_name = "flock"]
    fn libc_flock(fd: i32, operation: i32) -> i32;
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &File) -> anyhow::Result<()> {
    // Windows has no stable flock equivalent without a new dependency; the
    // callers additionally use atomic rename, which still prevents torn files.
    Ok(())
}

#[cfg(not(unix))]
fn unlock(_file: &File) {}

/// Writes `contents` to `target` atomically: unique temp file in the same
/// directory, then `rename`.
///
/// A fixed `.tmp` name (the previous approach) let two writers truncate each
/// other's temp file and publish a corrupted result.
pub fn write_atomic(target: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> anyhow::Result<()> {
    use std::io::Write;

    let target = target.as_ref();
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("path {:?} has no parent directory", target))?;
    std::fs::create_dir_all(parent)?;

    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let unique = format!(
        ".{}.{}.{}.tmp",
        file_name,
        std::process::id(),
        uuid_fragment()
    );
    let temp_path = parent.join(unique);

    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        std::fs::rename(&temp_path, target)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

/// Appends `payload` to `target` under an exclusive lock.
pub fn append_locked(target: impl AsRef<Path>, payload: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let target = target.as_ref();
    let _lock = FileLock::for_target(target)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(target)?;
    file.write_all(payload)?;
    file.flush()?;
    Ok(())
}

fn uuid_fragment() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_replaces_contents() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("state.json");
        write_atomic(&target, b"first").unwrap();
        write_atomic(&target, b"second").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "second");
    }

    #[test]
    fn atomic_write_leaves_no_temp_files() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("state.json");
        write_atomic(&target, b"data").unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files must be renamed or removed");
    }

    #[test]
    fn appends_are_not_lost() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("inbox.jsonl");
        let lines = Arc::new(AtomicUsize::new(0));

        std::thread::scope(|scope| {
            for _ in 0..4 {
                let target = target.clone();
                let lines = lines.clone();
                scope.spawn(move || {
                    for _ in 0..50 {
                        append_locked(&target, b"{\"n\":1}\n").unwrap();
                        lines.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });

        let content = std::fs::read_to_string(&target).unwrap();
        assert_eq!(content.lines().count(), 200);
        assert_eq!(lines.load(Ordering::SeqCst), 200);
    }

    #[test]
    fn lock_file_is_scoped_to_the_target() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("inbox.jsonl");
        {
            let _lock = FileLock::for_target(&target).unwrap();
            assert!(dir.path().join("inbox.jsonl.lock").exists());
        }
        // A second acquisition succeeds after the guard is dropped.
        let _second = FileLock::for_target(&target).unwrap();
    }
}
