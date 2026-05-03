use greentic_distributor_client::{
    DistClient, DistOptions, ReleaseChannel, ReleaseIndex, ReleaseResolutionContext,
};

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

#[derive(Debug, serde::Deserialize)]
struct FrequentComponents {
    components: Vec<FrequentComponent>,
}

#[derive(Debug, serde::Deserialize)]
struct FrequentComponent {
    id: String,
    component_ref: String,
}

fn stable_oci_component_refs() -> Vec<FrequentComponent> {
    let catalog: FrequentComponents =
        serde_json::from_str(include_str!("../frequent-components.json"))
            .expect("frequent-components.json should parse");

    catalog
        .components
        .into_iter()
        .filter(|component| {
            component.component_ref.starts_with("oci://")
                && component.component_ref.ends_with(":stable")
        })
        .collect()
}

fn stable_release_indexes(cache_dir: &Path) -> Vec<(ReleaseResolutionContext, PathBuf)> {
    let release_index_dir = cache_dir.join("release-index").join("v1").join("stable");
    let Ok(entries) = std::fs::read_dir(&release_index_dir) else {
        return Vec::new();
    };

    let mut indexes = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|path| {
            let bytes = std::fs::read(&path).ok()?;
            let index = serde_json::from_slice::<ReleaseIndex>(&bytes).ok()?;
            if index.channel != ReleaseChannel::Stable {
                return None;
            }
            Some((
                ReleaseResolutionContext {
                    release: index.release,
                    channel: ReleaseChannel::Stable,
                },
                path,
            ))
        })
        .collect::<Vec<_>>();

    indexes
        .sort_by(|(left, _), (right, _)| compare_release_versions(&right.release, &left.release));
    indexes
}

fn compare_release_versions(left: &str, right: &str) -> Ordering {
    let left_segments = numeric_version_segments(left);
    let right_segments = numeric_version_segments(right);
    left_segments
        .cmp(&right_segments)
        .then_with(|| left.cmp(right))
}

fn numeric_version_segments(version: &str) -> Vec<u64> {
    version
        .split('.')
        .map(|segment| segment.parse::<u64>().unwrap_or(0))
        .collect()
}

fn stable_oci_cache_test_is_required() -> bool {
    std::env::var("GREENTIC_REQUIRE_STABLE_OCI_CACHE").is_ok_and(|value| value == "1")
}

#[tokio::test]
async fn stable_frequent_component_oci_refs_are_available_locally() {
    let components = stable_oci_component_refs();
    assert!(
        !components.is_empty(),
        "frequent-components.json should contain stable OCI component refs"
    );

    let options = DistOptions {
        offline: true,
        ..Default::default()
    };
    let release_indexes = stable_release_indexes(&options.cache_dir);
    if release_indexes.is_empty() {
        let release_index_dir = options
            .cache_dir
            .join("release-index")
            .join("v1")
            .join("stable");
        if stable_oci_cache_test_is_required() {
            panic!(
                "no stable release index is present in the local distribution cache at {}",
                release_index_dir.display()
            );
        }
        eprintln!(
            "skipping stable OCI cache validation: no stable release index is present at {}",
            release_index_dir.display()
        );
        return;
    }

    let client = DistClient::new(options);

    let mut failures = Vec::new();
    for component in components {
        let mut resolved = None;
        let mut resolve_errors = Vec::new();
        for (context, path) in &release_indexes {
            match client
                .resolve_oci_ref_with_context(&component.component_ref, context)
                .await
            {
                Ok(descriptor) => {
                    resolved = Some(descriptor);
                    break;
                }
                Err(err) => resolve_errors.push(format!(
                    "{} ({}) via {}: {err}",
                    component.id,
                    component.component_ref,
                    path.display()
                )),
            }
        }

        let Some(descriptor) = resolved else {
            failures.push(format!(
                "{} ({}) could not be resolved from the local stable release index:\n{}",
                component.id,
                component.component_ref,
                resolve_errors.join("\n")
            ));
            continue;
        };

        if !descriptor.canonical_ref.contains("@sha256:") {
            failures.push(format!(
                "{} ({}) resolved to non-digest-pinned ref {}",
                component.id, component.component_ref, descriptor.canonical_ref
            ));
            continue;
        }

        match client.open_cached(&descriptor.digest) {
            Ok(artifact) => {
                let path = artifact.cache_path.as_ref().unwrap_or(&artifact.local_path);
                match std::fs::metadata(path) {
                    Ok(metadata) if metadata.len() > 0 => {}
                    Ok(_) => failures.push(format!(
                        "{} ({}) cached artifact is empty at {}",
                        component.id,
                        component.component_ref,
                        path.display()
                    )),
                    Err(err) => failures.push(format!(
                        "{} ({}) cached artifact is missing at {}: {err}",
                        component.id,
                        component.component_ref,
                        path.display()
                    )),
                }
            }
            Err(err) => failures.push(format!(
                "{} ({}) resolved to {}, but the artifact could not be opened from the local cache: {err}",
                component.id, component.component_ref, descriptor.canonical_ref
            )),
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
