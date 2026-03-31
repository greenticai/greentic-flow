use anyhow::{Context, Result, anyhow};
use greentic_distributor_client::{CachePolicy, DistClient, ResolvePolicy};
use greentic_types::ComponentId;
use greentic_types::flow_resolve::{ComponentSourceRefV1, FlowResolveV1};
use greentic_types::flow_resolve_summary::{
    FLOW_RESOLVE_SUMMARY_SCHEMA_VERSION, FlowResolveSummaryManifestV1,
    FlowResolveSummarySourceRefV1, FlowResolveSummaryV1, NodeResolveSummaryV1,
    read_flow_resolve_summary, resolve_summary_path_for_flow, write_flow_resolve_summary,
};
use semver::Version;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

pub fn write_flow_resolve_summary_for_node(
    flow_path: &Path,
    node_id: &str,
    sidecar: &FlowResolveV1,
) -> Result<PathBuf> {
    let summary_path = resolve_summary_path_for_flow(flow_path);
    if !summary_path.exists() {
        return write_flow_resolve_summary_for_flow(flow_path, sidecar);
    }
    let mut summary =
        read_flow_resolve_summary(&summary_path).map_err(|e| anyhow!(e.to_string()))?;
    summary.flow = flow_name_from_path(flow_path);
    let entry = sidecar.nodes.get(node_id).ok_or_else(|| {
        anyhow!(
            "resolve sidecar missing node '{}' while updating resolve summary",
            node_id
        )
    })?;
    let expected_source = summary_source_ref(&entry.source);
    if let Some(existing) = summary.nodes.get(node_id)
        && existing.source == expected_source
    {
        write_flow_resolve_summary(&summary_path, &summary).map_err(|e| anyhow!(e.to_string()))?;
        return Ok(summary_path);
    }
    let node_summary = summarize_node(flow_path, node_id, &entry.source)?;
    summary.nodes.insert(node_id.to_string(), node_summary);
    write_flow_resolve_summary(&summary_path, &summary).map_err(|e| anyhow!(e.to_string()))?;
    Ok(summary_path)
}

pub fn write_flow_resolve_summary_for_flow(
    flow_path: &Path,
    sidecar: &FlowResolveV1,
) -> Result<PathBuf> {
    let summary_path = resolve_summary_path_for_flow(flow_path);
    let summary = build_flow_resolve_summary(flow_path, sidecar)?;
    write_flow_resolve_summary(&summary_path, &summary).map_err(|e| anyhow!(e.to_string()))?;
    Ok(summary_path)
}

pub fn remove_flow_resolve_summary_node(
    flow_path: &Path,
    node_id: &str,
) -> Result<Option<PathBuf>> {
    let summary_path = resolve_summary_path_for_flow(flow_path);
    if !summary_path.exists() {
        return Ok(None);
    }
    let mut summary =
        read_flow_resolve_summary(&summary_path).map_err(|e| anyhow!(e.to_string()))?;
    summary.flow = flow_name_from_path(flow_path);
    summary.nodes.remove(node_id);
    write_flow_resolve_summary(&summary_path, &summary).map_err(|e| anyhow!(e.to_string()))?;
    Ok(Some(summary_path))
}

pub fn build_flow_resolve_summary(
    flow_path: &Path,
    sidecar: &FlowResolveV1,
) -> Result<FlowResolveSummaryV1> {
    let mut nodes = BTreeMap::new();
    for (node_id, entry) in &sidecar.nodes {
        let node_summary = summarize_node(flow_path, node_id, &entry.source)?;
        nodes.insert(node_id.clone(), node_summary);
    }
    Ok(FlowResolveSummaryV1 {
        schema_version: FLOW_RESOLVE_SUMMARY_SCHEMA_VERSION,
        flow: flow_name_from_path(flow_path),
        nodes,
    })
}

fn summarize_node(
    flow_path: &Path,
    node_id: &str,
    source: &ComponentSourceRefV1,
) -> Result<NodeResolveSummaryV1> {
    let (source_ref, wasm_path, digest) = resolve_source(flow_path, source)?;
    match find_manifest_for_wasm(&wasm_path) {
        Ok(manifest_path) => {
            let (component_id, manifest) =
                read_manifest_metadata(&manifest_path).with_context(|| {
                    format!(
                        "failed to read component.manifest.json for node '{}' ({})",
                        node_id,
                        manifest_path.display()
                    )
                })?;
            Ok(NodeResolveSummaryV1 {
                component_id,
                source: source_ref,
                digest,
                manifest,
            })
        }
        Err(_) if !matches!(source, ComponentSourceRefV1::Local { .. }) => {
            let component_id = component_id_from_source(source)
                .or_else(|| ComponentId::from_str(node_id).ok())
                .unwrap_or_else(|| ComponentId::from_str("unknown").expect("valid component id"));
            eprintln!(
                "warning: component manifest metadata missing for node '{}'; summary will omit manifest",
                node_id
            );
            Ok(NodeResolveSummaryV1 {
                component_id,
                source: source_ref,
                digest,
                manifest: None,
            })
        }
        Err(e) => Err(e).with_context(|| {
            format!(
                "component.manifest.json not found for node '{}' ({})",
                node_id,
                wasm_path.display()
            )
        }),
    }
}

fn component_id_from_source(source: &ComponentSourceRefV1) -> Option<ComponentId> {
    let raw_ref = match source {
        ComponentSourceRefV1::Oci { r#ref, .. } => r#ref,
        ComponentSourceRefV1::Repo { r#ref, .. } => r#ref,
        ComponentSourceRefV1::Store { r#ref, .. } => r#ref,
        ComponentSourceRefV1::Local { .. } => return None,
    };
    // Extract component name from ref like "oci://ghcr.io/greenticai/components/templates:latest"
    let path_part = raw_ref.split("://").last().unwrap_or(raw_ref);
    let without_tag = path_part.split([':', '@']).next().unwrap_or(path_part);
    let name = without_tag.rsplit('/').next().unwrap_or(without_tag);
    ComponentId::from_str(name).ok()
}

fn resolve_source(
    flow_path: &Path,
    source: &ComponentSourceRefV1,
) -> Result<(FlowResolveSummarySourceRefV1, PathBuf, String)> {
    match source {
        ComponentSourceRefV1::Local { path, .. } => {
            let wasm_path = local_path_from_sidecar(path, flow_path);
            let digest = compute_sha256(&wasm_path)?;
            Ok((summary_source_ref(source), wasm_path, digest))
        }
        ComponentSourceRefV1::Oci { r#ref, digest } => {
            resolve_remote(flow_path, r#ref, digest.as_deref(), RemoteKind::Oci)
        }
        ComponentSourceRefV1::Repo { r#ref, digest } => {
            resolve_remote(flow_path, r#ref, digest.as_deref(), RemoteKind::Repo)
        }
        ComponentSourceRefV1::Store { r#ref, digest, .. } => {
            resolve_remote(flow_path, r#ref, digest.as_deref(), RemoteKind::Store)
        }
    }
}

enum RemoteKind {
    Oci,
    Repo,
    Store,
}

fn summary_source_ref(source: &ComponentSourceRefV1) -> FlowResolveSummarySourceRefV1 {
    match source {
        ComponentSourceRefV1::Local { path, .. } => FlowResolveSummarySourceRefV1::Local {
            path: strip_file_prefix(path),
        },
        ComponentSourceRefV1::Oci { r#ref, .. } => FlowResolveSummarySourceRefV1::Oci {
            r#ref: r#ref.to_string(),
        },
        ComponentSourceRefV1::Repo { r#ref, .. } => FlowResolveSummarySourceRefV1::Repo {
            r#ref: r#ref.to_string(),
        },
        ComponentSourceRefV1::Store { r#ref, .. } => FlowResolveSummarySourceRefV1::Store {
            r#ref: r#ref.to_string(),
        },
    }
}

fn block_on_auto<F: std::future::Future>(fut: F) -> F::Output {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        tokio::task::block_in_place(|| handle.block_on(fut))
    } else {
        tokio::runtime::Runtime::new()
            .expect("create tokio runtime")
            .block_on(fut)
    }
}

fn resolve_remote(
    _flow_path: &Path,
    reference: &str,
    digest_hint: Option<&str>,
    kind: RemoteKind,
) -> Result<(FlowResolveSummarySourceRefV1, PathBuf, String)> {
    let client = DistClient::new(Default::default());
    let digest = match digest_hint {
        Some(d) => d.to_string(),
        None => {
            let source = client
                .parse_source(reference)
                .map_err(|e| anyhow!("failed to resolve reference {reference}: {e}"))?;
            block_on_auto(client.resolve(source, ResolvePolicy))
                .map_err(|e| anyhow!("failed to resolve reference {reference}: {e}"))?
                .digest
        }
    };
    let mut wasm_path = if let Ok(artifact) = client.open_cached(&digest) {
        artifact.local_path
    } else {
        let source = client.parse_source(reference).map_err(|e| {
            anyhow!(
                "component reference {} not available locally: {e}",
                reference
            )
        })?;
        let descriptor = block_on_auto(client.resolve(source, ResolvePolicy)).map_err(|e| {
            anyhow!(
                "component reference {} not available locally: {e}",
                reference
            )
        })?;
        let resolved = block_on_auto(client.fetch(&descriptor, CachePolicy)).map_err(|e| {
            anyhow!(
                "component reference {} not available locally: {e}",
                reference
            )
        })?;
        resolved
            .cache_path
            .ok_or_else(|| anyhow!("component reference {} has no cache path", reference))?
    };
    if let Some(cache_dir) = wasm_path.parent()
        && let Some(manifest_wasm) = manifest_wasm_from_dir(cache_dir)?
    {
        wasm_path = manifest_wasm;
    }
    let source_ref = match kind {
        RemoteKind::Oci => FlowResolveSummarySourceRefV1::Oci {
            r#ref: reference.to_string(),
        },
        RemoteKind::Repo => FlowResolveSummarySourceRefV1::Repo {
            r#ref: reference.to_string(),
        },
        RemoteKind::Store => FlowResolveSummarySourceRefV1::Store {
            r#ref: reference.to_string(),
        },
    };
    Ok((source_ref, wasm_path, digest))
}

fn manifest_wasm_from_dir(cache_dir: &Path) -> Result<Option<PathBuf>> {
    let manifest_path = cache_dir.join("component.manifest.json");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).context("parse component.manifest.json")?;
    let rel = json
        .get("artifacts")
        .and_then(|v| v.get("component_wasm"))
        .and_then(|v| v.as_str());
    let Some(rel) = rel else {
        return Ok(None);
    };
    let candidate = cache_dir.join(rel);
    if candidate.exists() {
        Ok(Some(candidate))
    } else {
        Ok(None)
    }
}

fn find_manifest_for_wasm(wasm_path: &Path) -> Result<PathBuf> {
    let wasm_abs = fs::canonicalize(wasm_path)
        .with_context(|| format!("resolve wasm path {}", wasm_path.display()))?;
    let mut current = wasm_abs.parent();
    while let Some(dir) = current {
        let candidate = dir.join("component.manifest.json");
        if candidate.exists() && manifest_matches_wasm(&candidate, &wasm_abs)? {
            return Ok(candidate);
        }
        current = dir.parent();
    }
    anyhow::bail!(
        "component.manifest.json not found for wasm {}",
        wasm_abs.display()
    );
}

fn manifest_matches_wasm(manifest_path: &Path, wasm_abs: &Path) -> Result<bool> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).context("parse component.manifest.json")?;
    let artifacts = json.get("artifacts").and_then(|v| v.as_object());
    let Some(artifacts) = artifacts else {
        return Ok(false);
    };
    let rel = artifacts
        .get("component_wasm")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("manifest missing artifacts.component_wasm"))?;
    let manifest_dir = manifest_path
        .parent()
        .ok_or_else(|| anyhow!("manifest path {} has no parent", manifest_path.display()))?;
    let abs = fs::canonicalize(manifest_dir.join(rel))
        .with_context(|| format!("resolve manifest wasm {}", rel))?;
    Ok(abs == *wasm_abs)
}

fn read_manifest_metadata(
    manifest_path: &Path,
) -> Result<(ComponentId, Option<FlowResolveSummaryManifestV1>)> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&raw).context("parse component.manifest.json")?;
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("manifest missing id"))?;
    let component_id = ComponentId::from_str(id).map_err(|e| anyhow!(e.to_string()))?;
    let world = json.get("world").and_then(|v| v.as_str());
    let version = json.get("version").and_then(|v| v.as_str());
    let manifest = match (world, version) {
        (Some(world), Some(version)) => {
            let parsed = Version::parse(version)
                .with_context(|| format!("invalid semver version {version}"))?;
            Some(FlowResolveSummaryManifestV1 {
                world: world.to_string(),
                version: parsed,
            })
        }
        _ => None,
    };
    Ok((component_id, manifest))
}

fn flow_name_from_path(flow_path: &Path) -> String {
    flow_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "flow.ygtc".to_string())
}

fn strip_file_prefix(path: &str) -> String {
    path.strip_prefix("file://").unwrap_or(path).to_string()
}

fn local_path_from_sidecar(path: &str, flow_path: &Path) -> PathBuf {
    let trimmed = path.strip_prefix("file://").unwrap_or(path);
    let raw = PathBuf::from(trimmed);
    if raw.is_absolute() {
        raw
    } else {
        flow_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(raw)
    }
}

fn compute_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("read wasm at {}", path.display()))?;
    let mut sha = Sha256::new();
    sha.update(bytes);
    Ok(format!("sha256:{:x}", sha.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use greentic_types::flow_resolve::ComponentSourceRefV1;
    use semver::Version;
    use tempfile::tempdir;

    #[test]
    fn helper_functions_normalize_local_paths_and_refs() {
        let flow_path = Path::new("/tmp/flows/demo.ygtc");
        assert_eq!(strip_file_prefix("file://component.wasm"), "component.wasm");
        assert_eq!(
            local_path_from_sidecar("relative/component.wasm", flow_path),
            Path::new("/tmp/flows/relative/component.wasm")
        );
        assert_eq!(
            local_path_from_sidecar("/abs/component.wasm", flow_path),
            PathBuf::from("/abs/component.wasm")
        );
        assert_eq!(flow_name_from_path(flow_path), "demo.ygtc");
    }

    #[test]
    fn helper_functions_extract_component_ids_from_remote_refs() {
        let source = ComponentSourceRefV1::Oci {
            r#ref: "oci://ghcr.io/greenticai/components/templates:latest".to_string(),
            digest: None,
        };
        assert_eq!(
            component_id_from_source(&source).unwrap().as_str(),
            "templates"
        );
        match summary_source_ref(&source) {
            FlowResolveSummarySourceRefV1::Oci { r#ref } => {
                assert!(r#ref.contains("ghcr.io"));
            }
            other => panic!("expected oci summary ref, got {other:?}"),
        }
    }

    #[test]
    fn manifest_helpers_require_matching_component_wasm() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("component/dist");
        fs::create_dir_all(&nested).unwrap();
        let wasm_path = nested.join("widget.wasm");
        fs::write(&wasm_path, b"wasm").unwrap();
        let manifest_path = dir.path().join("component/component.manifest.json");
        fs::write(
            &manifest_path,
            serde_json::json!({
                "id": "acme.widget",
                "version": "1.2.3",
                "world": "component",
                "artifacts": { "component_wasm": "dist/widget.wasm" }
            })
            .to_string(),
        )
        .unwrap();

        assert!(manifest_matches_wasm(&manifest_path, &wasm_path.canonicalize().unwrap()).unwrap());
        assert_eq!(find_manifest_for_wasm(&wasm_path).unwrap(), manifest_path);

        let (component_id, manifest) = read_manifest_metadata(&manifest_path).unwrap();
        assert_eq!(component_id.as_str(), "acme.widget");
        assert_eq!(
            manifest,
            Some(FlowResolveSummaryManifestV1 {
                world: "component".to_string(),
                version: Version::parse("1.2.3").unwrap(),
            })
        );
    }

    #[test]
    fn manifest_wasm_from_dir_and_sha_cover_missing_and_present_cases() {
        let dir = tempdir().unwrap();
        let wasm_path = dir.path().join("bundle.wasm");
        fs::write(&wasm_path, b"abc").unwrap();
        fs::write(
            dir.path().join("component.manifest.json"),
            serde_json::json!({
                "artifacts": { "component_wasm": "bundle.wasm" }
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(
            manifest_wasm_from_dir(dir.path()).unwrap(),
            Some(wasm_path.clone())
        );
        assert!(compute_sha256(&wasm_path).unwrap().starts_with("sha256:"));

        fs::write(
            dir.path().join("component.manifest.json"),
            serde_json::json!({
                "artifacts": { "component_wasm": "missing.wasm" }
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(manifest_wasm_from_dir(dir.path()).unwrap(), None);
    }
}
