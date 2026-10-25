use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CacheConfig {
    pub root: PathBuf,
    pub disk_enabled: bool,
    pub memory_enabled: bool,
    pub disk_max_bytes: Option<u64>,
    pub memory_max_bytes: u64,
    pub lfu_protect_hits: u64,
}

impl CacheConfig {
    pub fn disk_root(&self, engine_profile_id: &str) -> PathBuf {
        self.root.join("v1").join(engine_profile_id)
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        let root = std::env::var_os("GREENTIC_CACHE_DIR")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(default_cache_root);
        let cache_disabled =
            env_flag_set("GREENTIC_NO_CACHE") || env_flag_set("GREENTIC_DISABLE_CACHE");
        // Without a trustworthy absolute location the disk cache is disabled:
        // cached artifacts are loaded as native code and must never come from a
        // CWD-relative directory an untrusted repository could pre-populate.
        let disk_enabled = !cache_disabled && root.is_some();
        Self {
            root: root.unwrap_or_default(),
            disk_enabled,
            memory_enabled: !cache_disabled,
            disk_max_bytes: Some(5 * 1024 * 1024 * 1024),
            memory_max_bytes: 512 * 1024 * 1024,
            lfu_protect_hits: 3,
        }
    }
}

/// Per-user absolute cache directory (XDG on Linux, `Library/Caches` on macOS,
/// `LOCALAPPDATA` on Windows).
fn default_cache_root() -> Option<PathBuf> {
    let abs = |value: std::ffi::OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").and_then(abs)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .and_then(abs)
            .map(|home| home.join("Library").join("Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .and_then(abs)
            .or_else(|| {
                std::env::var_os("HOME")
                    .and_then(abs)
                    .map(|home| home.join(".cache"))
            })
    };
    base.map(|dir| dir.join("greentic").join("components"))
}

fn env_flag_set(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}
