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
            .unwrap_or_else(|| PathBuf::from(".greentic/cache/components"));
        let cache_disabled =
            env_flag_set("GREENTIC_NO_CACHE") || env_flag_set("GREENTIC_DISABLE_CACHE");
        Self {
            root,
            disk_enabled: !cache_disabled,
            memory_enabled: !cache_disabled,
            disk_max_bytes: Some(5 * 1024 * 1024 * 1024),
            memory_max_bytes: 512 * 1024 * 1024,
            lfu_protect_hits: 3,
        }
    }
}

fn env_flag_set(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}
