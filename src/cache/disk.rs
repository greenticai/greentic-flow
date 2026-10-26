use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::cache::PruneReport;
use crate::cache::engine_profile::EngineProfile;
use crate::cache::keys::ArtifactKey;
use crate::cache::metadata::ArtifactMetadata;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct DiskCache {
    root: PathBuf,
    profile: EngineProfile,
    disk_max_bytes: Option<u64>,
    warned_untrusted: Arc<AtomicBool>,
}

impl DiskCache {
    pub fn new(root: PathBuf, profile: EngineProfile, disk_max_bytes: Option<u64>) -> Self {
        Self {
            root,
            profile,
            disk_max_bytes,
            warned_untrusted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Cached artifacts are deserialized as native code, so the cache must not
    /// be controllable by other users. The cache root, its `artifacts` and
    /// `tmp` directories must be owned by the current effective user and not
    /// group/world writable; every existing ancestor must be owned by the
    /// current user or root and not world writable (a sticky bit, as on `/tmp`,
    /// is accepted for ancestors). Unix only.
    fn root_is_trusted(&self) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let euid = rustix::process::geteuid().as_raw();
            let own_dirs = [
                self.root.clone(),
                self.root.join("artifacts"),
                self.root.join("tmp"),
            ];
            for dir in &own_dirs {
                match fs::metadata(dir) {
                    Ok(meta) => {
                        if meta.uid() != euid || meta.mode() & 0o022 != 0 {
                            return false;
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return false,
                }
            }
            for dir in self.root.ancestors().skip(1) {
                if dir.as_os_str().is_empty() {
                    continue;
                }
                match fs::metadata(dir) {
                    Ok(meta) => {
                        let owner_ok = meta.uid() == euid || meta.uid() == 0;
                        // Group write is tolerated on ancestors (user-private groups
                        // make `0775` home/cache dirs common); world write is not.
                        let writable_by_others = meta.mode() & 0o002 != 0;
                        let sticky = meta.mode() & 0o1000 != 0;
                        if !owner_ok || (writable_by_others && !sticky) {
                            return false;
                        }
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return false,
                }
            }
            true
        }
        #[cfg(not(unix))]
        {
            true
        }
    }

    /// Tighten group/world-writable cache directories that we own to `0700`
    /// so a previously loose cache can be used again. Directories owned by
    /// someone else are left alone (they stay untrusted).
    fn repair_own_dirs(&self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let euid = rustix::process::geteuid().as_raw();
            for dir in [
                self.root.clone(),
                self.root.join("artifacts"),
                self.root.join("tmp"),
            ] {
                let Ok(meta) = fs::metadata(&dir) else {
                    continue;
                };
                if meta.uid() == euid && meta.mode() & 0o022 != 0 {
                    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
                }
            }
        }
    }

    fn warn_untrusted_once(&self) {
        if !self.warned_untrusted.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                root = %self.root.display(),
                "disabling disk component cache: directory is not owned by the current user, \
                 or it or an ancestor is group/world writable"
            );
        }
    }

    pub fn try_read(&self, key: &ArtifactKey) -> Result<Option<Vec<u8>>> {
        let paths = self.paths_for(key)?;
        if !self.root_is_trusted() {
            self.warn_untrusted_once();
            return Ok(None);
        }
        // A directory repaired from a loose mode may still hold files planted by
        // someone else; only entries owned by us and not group/world writable are used.
        if (paths.meta_path.exists() && !entry_file_trusted(&paths.meta_path))
            || (paths.artifact_path.exists() && !entry_file_trusted(&paths.artifact_path))
        {
            self.delete_entry(&paths)?;
            return Ok(None);
        }
        if !paths.meta_path.exists() {
            if paths.artifact_path.exists() {
                let _ = fs::remove_file(&paths.artifact_path);
            }
            return Ok(None);
        }
        let meta = match fs::read_to_string(&paths.meta_path) {
            Ok(raw) => match serde_json::from_str::<ArtifactMetadata>(&raw) {
                Ok(meta) => meta,
                Err(_) => {
                    self.delete_entry(&paths)?;
                    return Ok(None);
                }
            },
            Err(_) => {
                self.delete_entry(&paths)?;
                return Ok(None);
            }
        };
        if meta.validate_for_profile(&self.profile).is_err() {
            self.delete_entry(&paths)?;
            return Ok(None);
        }
        if meta.wasm_digest != key.wasm_digest {
            self.delete_entry(&paths)?;
            return Ok(None);
        }
        if !paths.artifact_path.exists() {
            self.delete_entry(&paths)?;
            return Ok(None);
        }
        let artifact_bytes = fs::read(&paths.artifact_path).ok();
        let Some(artifact_bytes) = artifact_bytes else {
            self.delete_entry(&paths)?;
            return Ok(None);
        };
        if meta.artifact_bytes != artifact_bytes.len() as u64 {
            self.delete_entry(&paths)?;
            return Ok(None);
        }
        self.update_access(&paths, meta).ok();
        Ok(Some(artifact_bytes))
    }

    pub fn write_atomic(
        &self,
        key: &ArtifactKey,
        bytes: &[u8],
        meta: &ArtifactMetadata,
    ) -> Result<()> {
        meta.validate_for_profile(&self.profile)
            .context("cache metadata does not match engine profile")?;
        if meta.wasm_digest != key.wasm_digest {
            bail!("cache metadata digest does not match artifact key");
        }
        let paths = self.paths_for(key)?;
        self.repair_own_dirs();
        if !self.root_is_trusted() {
            // Same policy as reads: an untrusted cache is neither read nor written.
            self.warn_untrusted_once();
            return Ok(());
        }
        create_private_dir_all(&paths.artifacts_dir).with_context(|| {
            format!(
                "failed to create cache dir {}",
                paths.artifacts_dir.display()
            )
        })?;
        create_private_dir_all(&paths.tmp_dir).with_context(|| {
            format!("failed to create cache tmp dir {}", paths.tmp_dir.display())
        })?;
        let tmp_artifact = paths.tmp_path("artifact");
        let tmp_meta = paths.tmp_path("meta");
        let result = (|| -> Result<()> {
            write_private_file(&tmp_artifact, bytes)
                .with_context(|| format!("failed to write {}", tmp_artifact.display()))?;
            let meta_json =
                serde_json::to_vec_pretty(meta).context("failed to serialize cache metadata")?;
            write_private_file(&tmp_meta, meta_json)
                .with_context(|| format!("failed to write {}", tmp_meta.display()))?;
            // The metadata file is the commit marker (readers check it first), so
            // the artifact must be in place before the metadata.
            fs::rename(&tmp_artifact, &paths.artifact_path)
                .with_context(|| format!("failed to rename {}", paths.artifact_path.display()))?;
            if let Err(err) = fs::rename(&tmp_meta, &paths.meta_path) {
                // Do not leave an artifact without metadata behind.
                let _ = fs::remove_file(&paths.artifact_path);
                return Err(err)
                    .with_context(|| format!("failed to rename {}", paths.meta_path.display()));
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp_artifact);
            let _ = fs::remove_file(&tmp_meta);
        }
        result
    }

    pub fn approx_size_bytes(&self) -> Result<u64> {
        let artifacts_dir = self.root.join("artifacts");
        if !artifacts_dir.exists() {
            return Ok(0);
        }
        let mut total = 0u64;
        for entry in fs::read_dir(&artifacts_dir)
            .with_context(|| format!("failed to read {}", artifacts_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("cwasm") {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            total = total.saturating_add(size);
        }
        Ok(total)
    }

    pub fn prune_to_limit(&self, dry_run: bool) -> Result<PruneReport> {
        if !self.root_is_trusted() {
            self.warn_untrusted_once();
            return Ok(PruneReport {
                removed_entries: 0,
                removed_bytes: 0,
            });
        }
        let Some(limit) = self.disk_max_bytes else {
            return Ok(PruneReport {
                removed_entries: 0,
                removed_bytes: 0,
            });
        };
        let artifacts_dir = self.root.join("artifacts");
        if !artifacts_dir.exists() {
            return Ok(PruneReport {
                removed_entries: 0,
                removed_bytes: 0,
            });
        }
        let canonical_artifacts_dir = artifacts_dir
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", artifacts_dir.display()))?;
        let mut entries = Vec::new();
        let mut total_bytes = 0u64;
        for entry in fs::read_dir(&artifacts_dir)
            .with_context(|| format!("failed to read {}", artifacts_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(raw) => raw,
                Err(_) => continue,
            };
            let meta: ArtifactMetadata = match serde_json::from_str(&raw) {
                Ok(meta) => meta,
                Err(_) => continue,
            };
            let access = meta.last_access_time();
            let artifact_path = path.with_extension("cwasm");
            let canonical_artifact = match artifact_path.canonicalize() {
                Ok(path) if path.starts_with(&canonical_artifacts_dir) => path,
                _ => continue,
            };
            let size = fs::metadata(&canonical_artifact)
                .map(|m| m.len())
                .unwrap_or(0);
            total_bytes = total_bytes.saturating_add(size);
            entries.push((access, meta, canonical_artifact, path, size));
        }
        entries.sort_by_key(|(access, _, _, _, _)| {
            access.map(|ts| ts.timestamp()).unwrap_or(i64::MIN)
        });
        let mut removed_entries = 0u64;
        let mut removed_bytes = 0u64;
        let mut remaining = total_bytes;
        for (_access, _meta, artifact_path, meta_path, size) in entries {
            if remaining <= limit {
                break;
            }
            if !dry_run {
                let _ = fs::remove_file(&artifact_path);
                let _ = fs::remove_file(&meta_path);
            }
            removed_entries = removed_entries.saturating_add(1);
            removed_bytes = removed_bytes.saturating_add(size);
            remaining = remaining.saturating_sub(size);
        }
        Ok(PruneReport {
            removed_entries,
            removed_bytes,
        })
    }

    pub fn artifact_count(&self) -> Result<u64> {
        let artifacts_dir = self.root.join("artifacts");
        if !artifacts_dir.exists() {
            return Ok(0);
        }
        let mut count = 0u64;
        for entry in fs::read_dir(&artifacts_dir)
            .with_context(|| format!("failed to read {}", artifacts_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("cwasm") {
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }

    pub fn delete(&self, key: &ArtifactKey) -> Result<()> {
        let paths = self.paths_for(key)?;
        self.delete_entry(&paths)
    }

    fn update_access(&self, paths: &DiskPaths, mut meta: ArtifactMetadata) -> Result<()> {
        meta.touch();
        let tmp = paths.tmp_path("meta");
        let json = serde_json::to_vec_pretty(&meta)?;
        let _ = create_private_dir_all(&paths.tmp_dir);
        write_private_file(&tmp, json)?;
        if let Err(err) = fs::rename(&tmp, &paths.meta_path) {
            let _ = fs::remove_file(&tmp);
            return Err(err.into());
        }
        Ok(())
    }

    fn delete_entry(&self, paths: &DiskPaths) -> Result<()> {
        let _ = fs::remove_file(&paths.artifact_path);
        let _ = fs::remove_file(&paths.meta_path);
        Ok(())
    }

    fn paths_for(&self, key: &ArtifactKey) -> Result<DiskPaths> {
        if key.engine_profile_id != self.profile.engine_profile_id {
            bail!("artifact key engine_profile_id mismatch");
        }
        let artifacts_dir = self.root.join("artifacts");
        let tmp_dir = self.root.join("tmp");
        let name = digest_to_filename(&key.wasm_digest);
        let artifact_path = artifacts_dir.join(format!("{}.cwasm", name));
        let meta_path = artifacts_dir.join(format!("{}.json", name));
        Ok(DiskPaths {
            artifacts_dir,
            tmp_dir,
            artifact_path,
            meta_path,
        })
    }
}

struct DiskPaths {
    artifacts_dir: PathBuf,
    tmp_dir: PathBuf,
    artifact_path: PathBuf,
    meta_path: PathBuf,
}

impl DiskPaths {
    fn tmp_path(&self, suffix: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        self.tmp_path_at(now, suffix)
    }

    /// Clock is injected so that tests can force identical timestamps; uniqueness must
    /// then come from the process-wide counter alone.
    fn tmp_path_at(&self, now: u128, suffix: &str) -> PathBuf {
        let pid = std::process::id();
        let seq = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        self.tmp_dir
            .join(format!("tmp_{}_{}_{}_{}", pid, now, seq, suffix))
    }
}

/// Cache entries are loaded as native code: require a regular file owned by the
/// current user that is not group/world writable. Unix only.
fn entry_file_trusted(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let euid = rustix::process::geteuid().as_raw();
        match fs::symlink_metadata(path) {
            Ok(meta) => meta.is_file() && meta.uid() == euid && meta.mode() & 0o022 == 0,
            Err(_) => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        true
    }
}

/// Write `bytes` to `path`, creating it with mode `0600` on Unix.
fn write_private_file(path: &Path, bytes: impl AsRef<[u8]>) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes.as_ref())
}

/// Create `path` (and parents) with owner-only permissions on unix.
fn create_private_dir_all(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path)
    }
}

fn digest_to_filename(digest: &str) -> String {
    let sanitized: String = digest
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "artifact".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tmp_path_tests {
    use super::*;

    #[test]
    fn tmp_path_is_unique_even_within_one_clock_tick() {
        let paths = DiskPaths {
            artifacts_dir: PathBuf::from("a"),
            tmp_dir: PathBuf::from("t"),
            artifact_path: PathBuf::from("x"),
            meta_path: PathBuf::from("m"),
        };
        // Identical pid and identical timestamp: only the TMP_COUNTER segment can keep the
        // names apart.
        let same_tick = 42_u128;
        let names: std::collections::HashSet<PathBuf> = (0..1000)
            .map(|_| paths.tmp_path_at(same_tick, "meta"))
            .collect();
        assert_eq!(names.len(), 1000, "tmp paths must never repeat");

        let first = paths.tmp_path("meta");
        let second = paths.tmp_path("meta");
        assert_ne!(first, second);
    }
}
