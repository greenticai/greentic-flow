use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Normalize a user-supplied path and ensure it stays within an allowed root.
/// Rejects paths that escape the root via absolute paths, `..`, or symlinks.
pub fn normalize_under_root(root: &Path, candidate: &Path) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize root {}", root.display()))?;
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let canon = joined
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", joined.display()))?;

    if !canon.starts_with(&root) {
        anyhow::bail!(
            "path escapes root ({}): {}",
            root.display(),
            canon.display()
        );
    }

    Ok(canon)
}
