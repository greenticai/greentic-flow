use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::resolve_summary::read_manifest_value;
/// Minimal metadata needed to validate that a component exists and which config keys
/// are required.
#[derive(Debug, Clone)]
pub struct ComponentMetadata {
    pub id: String,
    pub required_fields: Vec<String>,
}

pub trait ComponentCatalog: Send + Sync {
    fn resolve(&self, component_id: &str) -> Option<ComponentMetadata>;
}

/// Catalog backed by component manifest files on disk (CBOR or JSON).
#[derive(Debug, Default, Clone)]
pub struct ManifestCatalog {
    entries: HashMap<String, ComponentMetadata>,
}

#[derive(Deserialize)]
struct Manifest {
    id: String,
    #[serde(default)]
    config_schema: Option<Schema>,
}

#[derive(Deserialize, Default)]
struct Schema {
    #[serde(default)]
    required: Vec<String>,
}

impl ManifestCatalog {
    pub fn load_from_paths(paths: &[impl AsRef<Path>]) -> Self {
        let mut entries = HashMap::new();
        for path in paths {
            let path = path.as_ref();
            let parsed_value = Self::read_manifest_file(path);
            if let Some(mut value) = parsed_value {
                normalize_manifest_value(&mut value);
                if let Ok(manifest) = serde_json::from_value::<Manifest>(value) {
                    entries.insert(
                        manifest.id.clone(),
                        ComponentMetadata {
                            id: manifest.id,
                            required_fields: manifest
                                .config_schema
                                .unwrap_or_default()
                                .required
                                .clone(),
                        },
                    );
                    entries
                        .entry("component.exec".to_string())
                        .or_insert(ComponentMetadata {
                            id: "component.exec".to_string(),
                            required_fields: Vec::new(),
                        });
                    continue;
                }
            }
            // Continue without crashing on unreadable manifests to keep the catalog usable.
        }
        ManifestCatalog { entries }
    }

    /// Read a manifest file, detecting CBOR vs JSON by file extension.
    fn read_manifest_file(path: &Path) -> Option<Value> {
        read_manifest_value(path).ok()
    }
}

impl ComponentCatalog for ManifestCatalog {
    fn resolve(&self, component_id: &str) -> Option<ComponentMetadata> {
        self.entries.get(component_id).cloned()
    }
}

/// Catalog that can be seeded programmatically for tests.
#[derive(Debug, Default, Clone)]
pub struct MemoryCatalog {
    entries: HashMap<String, ComponentMetadata>,
}

impl MemoryCatalog {
    pub fn insert(&mut self, meta: ComponentMetadata) {
        self.entries.insert(meta.id.clone(), meta);
    }
}

impl ComponentCatalog for MemoryCatalog {
    fn resolve(&self, component_id: &str) -> Option<ComponentMetadata> {
        self.entries.get(component_id).cloned()
    }
}

impl ComponentCatalog for Box<dyn ComponentCatalog> {
    fn resolve(&self, component_id: &str) -> Option<ComponentMetadata> {
        self.as_ref().resolve(component_id)
    }
}

/// Normalize legacy manifest shapes in-place (e.g., operations as an array of strings).
pub fn normalize_manifest_value(value: &mut Value) {
    if let Some(ops) = value.get_mut("operations").and_then(Value::as_array_mut) {
        let mut normalized = Vec::with_capacity(ops.len());
        for entry in ops.drain(..) {
            if let Value::String(s) = entry {
                normalized.push(json!({ "name": s }));
            } else {
                normalized.push(entry);
            }
        }
        *ops = normalized;
    }
}
