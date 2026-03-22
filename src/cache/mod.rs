#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/config.rs"]
pub mod config;
#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/disk.rs"]
pub mod disk;
#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/engine_profile.rs"]
pub mod engine_profile;
#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/keys.rs"]
pub mod keys;
#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/memory.rs"]
pub mod memory;
#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/metadata.rs"]
pub mod metadata;
#[path = "../../../greentic-runner/crates/greentic-runner-host/src/cache/singleflight.rs"]
pub mod singleflight;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use wasmtime::Engine;
use wasmtime::component::Component;

pub use config::CacheConfig;
pub use engine_profile::{CpuPolicy, EngineProfile};
pub use keys::ArtifactKey;
pub use memory::MemoryStats;
pub use metadata::ArtifactMetadata;

use disk::DiskCache;
use memory::MemoryCache;
use singleflight::Singleflight;

#[derive(Clone, Debug)]
pub struct CacheManager {
    config: CacheConfig,
    profile: EngineProfile,
    memory: MemoryCache,
    disk: DiskCache,
    singleflight: Singleflight,
    metrics: Arc<CacheMetrics>,
}

#[derive(Debug, Default)]
struct CacheMetrics {
    memory_hits: AtomicU64,
    disk_hits: AtomicU64,
    disk_reads: AtomicU64,
    compiles: AtomicU64,
}

#[derive(Clone, Debug, Default)]
pub struct CacheMetricsSnapshot {
    pub memory_hits: u64,
    pub disk_hits: u64,
    pub disk_reads: u64,
    pub compiles: u64,
}

#[derive(Clone, Debug, Default)]
pub struct DiskStats {
    pub artifact_bytes: u64,
    pub artifact_count: u64,
}

impl CacheManager {
    pub fn new(config: CacheConfig, profile: EngineProfile) -> Self {
        let disk_root = config.disk_root(profile.id());
        let memory_max_bytes = config.memory_max_bytes;
        let lfu_protect_hits = config.lfu_protect_hits;
        let disk_max_bytes = config.disk_max_bytes;
        let memory = MemoryCache::new(memory_max_bytes, lfu_protect_hits);
        Self {
            config,
            profile: profile.clone(),
            memory,
            disk: DiskCache::new(disk_root, profile, disk_max_bytes),
            singleflight: Singleflight::new(),
            metrics: Arc::new(CacheMetrics::default()),
        }
    }

    pub fn engine_profile_id(&self) -> &str {
        self.profile.id()
    }

    pub fn metrics(&self) -> CacheMetricsSnapshot {
        CacheMetricsSnapshot {
            memory_hits: self.metrics.memory_hits.load(Ordering::Relaxed),
            disk_hits: self.metrics.disk_hits.load(Ordering::Relaxed),
            disk_reads: self.metrics.disk_reads.load(Ordering::Relaxed),
            compiles: self.metrics.compiles.load(Ordering::Relaxed),
        }
    }

    pub fn memory_stats(&self) -> MemoryStats {
        self.memory.stats()
    }

    pub fn disk_stats(&self) -> Result<DiskStats> {
        if !self.config.disk_enabled {
            return Ok(DiskStats::default());
        }
        Ok(DiskStats {
            artifact_bytes: self.disk.approx_size_bytes()?,
            artifact_count: self.disk.artifact_count()?,
        })
    }

    #[allow(unsafe_code)]
    pub async fn get_component(
        &self,
        engine: &Engine,
        key: &ArtifactKey,
        wasm_bytes: impl FnOnce() -> Result<Vec<u8>>,
    ) -> Result<Arc<Component>> {
        if self.config.memory_enabled
            && let Some(component) = self.memory.get(key)
        {
            self.metrics.memory_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(component);
        }
        if self.config.disk_enabled {
            self.metrics.disk_reads.fetch_add(1, Ordering::Relaxed);
            if let Some(serialized) = self.disk.try_read(key)? {
                match unsafe { Component::deserialize(engine, &serialized) } {
                    Ok(component) => {
                        self.metrics.disk_hits.fetch_add(1, Ordering::Relaxed);
                        let component = Arc::new(component);
                        if self.config.memory_enabled {
                            self.memory.insert(
                                key.clone(),
                                Arc::clone(&component),
                                serialized.len(),
                                false,
                            );
                        }
                        return Ok(component);
                    }
                    Err(_) => {
                        let _ = self.disk.delete(key);
                    }
                }
            }
        }

        let _guard = self.singleflight.acquire(key.clone()).await;
        if self.config.memory_enabled
            && let Some(component) = self.memory.get(key)
        {
            self.metrics.memory_hits.fetch_add(1, Ordering::Relaxed);
            return Ok(component);
        }
        if self.config.disk_enabled {
            self.metrics.disk_reads.fetch_add(1, Ordering::Relaxed);
            if let Some(serialized) = self.disk.try_read(key)? {
                match unsafe { Component::deserialize(engine, &serialized) } {
                    Ok(component) => {
                        self.metrics.disk_hits.fetch_add(1, Ordering::Relaxed);
                        let component = Arc::new(component);
                        if self.config.memory_enabled {
                            self.memory.insert(
                                key.clone(),
                                Arc::clone(&component),
                                serialized.len(),
                                false,
                            );
                        }
                        return Ok(component);
                    }
                    Err(_) => {
                        let _ = self.disk.delete(key);
                    }
                }
            }
        }

        let bytes = wasm_bytes()?;
        self.metrics.compiles.fetch_add(1, Ordering::Relaxed);
        let component = Component::from_binary(engine, &bytes)?;
        let component = Arc::new(component);
        if self.config.disk_enabled
            && let Ok(serialized) = component.serialize()
        {
            let meta = ArtifactMetadata::new(
                &self.profile,
                key.wasm_digest.clone(),
                serialized.len() as u64,
            );
            let _ = self.disk.write_atomic(key, &serialized, &meta);
        }
        if self.config.memory_enabled {
            self.memory
                .insert(key.clone(), Arc::clone(&component), bytes.len(), false);
        }
        Ok(component)
    }

    pub async fn warmup(
        &self,
        _engine: &Engine,
        items: &[WarmupItem],
        _mode: WarmupMode,
    ) -> Result<WarmupReport> {
        Ok(WarmupReport {
            warmed: items.len() as u64,
            skipped: 0,
        })
    }

    pub fn doctor(&self) -> CacheDoctorReport {
        CacheDoctorReport {
            disk_enabled: self.config.disk_enabled,
            memory_enabled: self.config.memory_enabled,
            entries_checked: 0,
        }
    }

    pub async fn prune_disk(&self, dry_run: bool) -> Result<PruneReport> {
        self.disk.prune_to_limit(dry_run)
    }
}

#[derive(Clone, Debug)]
pub struct WarmupItem {
    pub key: ArtifactKey,
}

#[derive(Clone, Copy, Debug)]
pub enum WarmupMode {
    BestEffort,
    Strict,
}

#[derive(Clone, Debug)]
pub struct WarmupReport {
    pub warmed: u64,
    pub skipped: u64,
}

#[derive(Clone, Debug)]
pub struct CacheDoctorReport {
    pub disk_enabled: bool,
    pub memory_enabled: bool,
    pub entries_checked: u64,
}

#[derive(Clone, Debug)]
pub struct PruneReport {
    pub removed_entries: u64,
    pub removed_bytes: u64,
}
