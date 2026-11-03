//! Root-confined path resolution.
//!
//! Every tool that touches the filesystem resolves user/LLM supplied paths
//! through a [`PathGuard`]. The guard rejects absolute paths outside the
//! workspace, `..` traversal and symlinks whose target escapes the root.
//!
//! This is the single place where confinement is implemented; tools must not
//! join user paths onto a root themselves.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PathGuard {
    root: PathBuf,
    canonical_root: PathBuf,
}

impl PathGuard {
    /// Creates a guard for `root`, creating the directory if necessary.
    pub fn new(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let root = root.as_ref();
        std::fs::create_dir_all(root)?;
        let canonical_root = root
            .canonicalize()
            .map_err(|e| anyhow::anyhow!("cannot resolve workspace root {:?}: {}", root, e))?;
        Ok(Self {
            root: root.to_path_buf(),
            canonical_root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validates `user_path` and returns a path relative to the root.
    ///
    /// Absolute paths are accepted only when they already point inside the
    /// workspace. Any `..`, prefix or root component is rejected outright so
    /// that the lexical component can never climb out of the sandbox.
    fn relative_path(&self, user_path: &str) -> anyhow::Result<PathBuf> {
        let trimmed = user_path.trim();
        if trimmed.is_empty() {
            anyhow::bail!("path must not be empty");
        }
        if trimmed.contains('\0') {
            anyhow::bail!("path contains a NUL byte");
        }

        let candidate = Path::new(trimmed);
        let relative = if candidate.is_absolute() {
            candidate
                .strip_prefix(&self.root)
                .or_else(|_| candidate.strip_prefix(&self.canonical_root))
                .map_err(|_| {
                    anyhow::anyhow!(
                        "path '{}' is outside the workspace '{}'",
                        user_path,
                        self.root.display()
                    )
                })?
                .to_path_buf()
        } else {
            candidate.to_path_buf()
        };

        let mut safe = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => safe.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    anyhow::bail!("path '{}' contains a forbidden component", user_path)
                }
            }
        }

        if safe.as_os_str().is_empty() {
            anyhow::bail!("path '{}' must name a file or directory", user_path);
        }
        Ok(safe)
    }

    /// Resolves a path that is about to be written.
    ///
    /// The nearest existing ancestor is canonicalized before the path is
    /// accepted, which also catches symlinked parents.
    pub fn resolve_for_write(&self, user_path: &str) -> anyhow::Result<PathBuf> {
        let relative = self.relative_path(user_path)?;
        let target = self.canonical_root.join(&relative);

        let mut ancestor = target.as_path();
        while !ancestor.exists() {
            ancestor = ancestor
                .parent()
                .ok_or_else(|| anyhow::anyhow!("path '{}' has no existing ancestor", user_path))?;
        }

        let canonical_ancestor = ancestor.canonicalize()?;
        if !canonical_ancestor.starts_with(&self.canonical_root) {
            anyhow::bail!(
                "path '{}' escapes the workspace through a symlink",
                user_path
            );
        }

        Ok(target)
    }

    /// Resolves a path that must already exist.
    pub fn resolve_existing(&self, user_path: &str) -> anyhow::Result<PathBuf> {
        let target = self.resolve_for_write(user_path)?;
        if !target.exists() {
            anyhow::bail!("path '{}' does not exist", user_path);
        }
        // `resolve_for_write` already canonicalized the target itself when it
        // exists, but re-check explicitly so the invariant is local.
        let canonical = target.canonicalize()?;
        if !canonical.starts_with(&self.canonical_root) {
            anyhow::bail!(
                "path '{}' escapes the workspace through a symlink",
                user_path
            );
        }
        Ok(target)
    }
}

/// Returns true for credential-bearing paths that must never be read or
/// written by an agent.
pub fn is_sensitive_path(path: &Path) -> bool {
    const SENSITIVE_NAMES: [&str; 5] = [
        ".netrc",
        ".git-credentials",
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
    ];
    const SENSITIVE_DIRS: [&str; 4] = [".ssh", ".aws", ".gnupg", ".config"];

    for component in path.components() {
        if let Component::Normal(part) = component {
            let name = part.to_string_lossy().to_ascii_lowercase();
            if name.starts_with(".env") || SENSITIVE_NAMES.contains(&name.as_str()) {
                return true;
            }
            if SENSITIVE_DIRS.contains(&name.as_str()) {
                return true;
            }
        }
    }
    false
}

/// Produces a filesystem-safe single path segment from untrusted text.
pub fn sanitize_filename(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(&['-', '.'][..]);
    let limited: String = trimmed.chars().take(96).collect();
    if limited.is_empty() {
        "unnamed".to_string()
    } else {
        limited
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn allows_paths_inside_root() {
        let dir = tempdir().unwrap();
        let guard = PathGuard::new(dir.path()).unwrap();
        std::fs::write(dir.path().join("notes.txt"), "hello").unwrap();

        let resolved = guard.resolve_existing("notes.txt").unwrap();
        assert_eq!(resolved, dir.path().join("notes.txt"));

        let absolute = dir.path().join("notes.txt");
        assert!(guard
            .resolve_existing(absolute.to_str().unwrap())
            .is_ok());
    }

    #[test]
    fn rejects_parent_traversal() {
        let dir = tempdir().unwrap();
        let guard = PathGuard::new(dir.path().join("workspace")).unwrap();
        assert!(guard.resolve_for_write("../escape.txt").is_err());
        assert!(guard.resolve_for_write("nested/../../escape.txt").is_err());
    }

    #[test]
    fn rejects_absolute_outside_root() {
        let dir = tempdir().unwrap();
        let guard = PathGuard::new(dir.path().join("workspace")).unwrap();
        let outside = dir.path().join("outside.txt");
        assert!(guard
            .resolve_for_write(outside.to_str().unwrap())
            .is_err());
    }

    #[test]
    fn rejects_empty_path() {
        let dir = tempdir().unwrap();
        let guard = PathGuard::new(dir.path()).unwrap();
        assert!(guard.resolve_for_write("").is_err());
        assert!(guard.resolve_for_write("   ").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("workspace");
        std::fs::create_dir_all(&root).unwrap();
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        symlink(&outside, root.join("link")).unwrap();

        let guard = PathGuard::new(&root).unwrap();
        assert!(guard.resolve_for_write("link/secret.txt").is_err());
    }

    #[test]
    fn flags_sensitive_paths() {
        assert!(is_sensitive_path(Path::new(".env")));
        assert!(is_sensitive_path(Path::new("config/.env.local")));
        assert!(is_sensitive_path(Path::new("home/.ssh/id_rsa")));
        assert!(!is_sensitive_path(Path::new("src/environment.rs")));
    }

    #[test]
    fn sanitizes_filenames() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize_filename(""), "unnamed");
        assert_eq!(sanitize_filename("topic name"), "topic-name");
    }
}
