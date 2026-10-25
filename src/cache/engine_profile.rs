use sha2::{Digest, Sha256};
use wasmtime::Engine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuPolicy {
    Native,
    Baseline,
}

impl CpuPolicy {
    pub fn as_str(&self) -> &'static str {
        match self {
            CpuPolicy::Native => "native",
            CpuPolicy::Baseline => "baseline",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineProfile {
    pub wasmtime_version: String,
    pub target_triple: String,
    pub cpu_policy: CpuPolicy,
    pub config_fingerprint: String,
    pub engine_profile_id: String,
}

impl EngineProfile {
    pub fn from_engine(
        _engine: &Engine,
        cpu_policy: CpuPolicy,
        config_fingerprint: String,
    ) -> Self {
        let wasmtime_version = wasmtime_environ::VERSION.to_string();
        let target_triple = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);
        let engine_profile_id = compute_engine_profile_id(
            &wasmtime_version,
            &target_triple,
            cpu_policy,
            &config_fingerprint,
        );
        Self {
            wasmtime_version,
            target_triple,
            cpu_policy,
            config_fingerprint,
            engine_profile_id,
        }
    }

    pub fn id(&self) -> &str {
        &self.engine_profile_id
    }
}

fn compute_engine_profile_id(
    wasmtime_version: &str,
    target_triple: &str,
    cpu_policy: CpuPolicy,
    config_fingerprint: &str,
) -> String {
    // Length-prefix every field so ("ab","c") and ("a","bc") cannot collide.
    let mut hasher = Sha256::new();
    for field in [
        wasmtime_version,
        target_triple,
        cpu_policy.as_str(),
        config_fingerprint,
    ] {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    format!(
        "sha256:{}",
        crate::flow_bundle::sha256_hex(hasher.finalize())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_id_does_not_collide_across_field_boundaries() {
        let a = compute_engine_profile_id("ab", "c", CpuPolicy::Native, "x");
        let b = compute_engine_profile_id("a", "bc", CpuPolicy::Native, "x");
        assert_ne!(a, b);
    }

    #[test]
    fn wasmtime_version_matches_linked_wasmtime() {
        let engine = Engine::default();
        let profile = EngineProfile::from_engine(&engine, CpuPolicy::Native, "fp".to_string());
        assert_eq!(profile.wasmtime_version, wasmtime_environ::VERSION);
    }

    /// Independent source of truth: the resolved versions in `Cargo.lock`.
    /// The profile id embeds `wasmtime_environ::VERSION`, so it must be the
    /// same release as the `wasmtime` crate that compiles and deserializes
    /// the cached artifacts.
    #[test]
    fn wasmtime_and_environ_resolve_to_one_matching_version_in_lockfile() {
        let lock_path = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock");
        let lock = std::fs::read_to_string(lock_path).expect("read Cargo.lock");
        let versions_of = |crate_name: &str| -> Vec<String> {
            let mut out = Vec::new();
            for block in lock.split("[[package]]") {
                let mut name = None;
                let mut version = None;
                for line in block.lines() {
                    if let Some(v) = line.strip_prefix("name = ") {
                        name = Some(v.trim_matches('"').to_string());
                    } else if let Some(v) = line.strip_prefix("version = ") {
                        version = Some(v.trim_matches('"').to_string());
                    }
                }
                if let (Some(n), Some(v)) = (name, version)
                    && n == crate_name
                {
                    out.push(v);
                }
            }
            out
        };
        let wasmtime = versions_of("wasmtime");
        let environ = versions_of("wasmtime-environ");
        assert_eq!(
            wasmtime.len(),
            1,
            "multiple wasmtime versions: {wasmtime:?}"
        );
        assert_eq!(environ, wasmtime, "wasmtime-environ differs from wasmtime");
        assert_eq!(wasmtime_environ::VERSION, wasmtime[0]);
    }
}
