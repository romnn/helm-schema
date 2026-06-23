use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::doc_backed_schema::{
    LocalSchemaLeaf, debug_materialize_local_schema,
    descend_schema_path_expanding_leaf_with_root_metadata_source, fragment_for_source_leaf,
};
use crate::inference::cache_scan::scan_crd_source_dir;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource,
};
use crate::schema_doc::SchemaDoc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResourceDocKey {
    api_version: String,
    kind: String,
}

impl ResourceDocKey {
    fn from_resource(resource: &ResourceRef) -> Self {
        Self {
            api_version: resource.api_version.clone(),
            kind: resource.kind.clone(),
        }
    }
}

#[derive(Debug)]
pub struct LocalSchemaProvider {
    root_dir: PathBuf,
    allow_api_version_guess: bool,
    docs: Mutex<HashMap<ResourceDocKey, SchemaDoc>>,
}

impl LocalSchemaProvider {
    #[must_use]
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
            allow_api_version_guess: false,
            docs: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn with_api_version_guess(mut self, enabled: bool) -> Self {
        self.allow_api_version_guess = enabled;
        self
    }

    fn relative_path_for_resource(resource: &ResourceRef) -> Option<String> {
        let api_version = resource.api_version.trim();
        let kind = resource.kind.trim();
        if api_version.is_empty() || kind.is_empty() {
            return None;
        }
        let (group, version) = api_version.split_once('/')?;
        let group = group.trim();
        let version = version.trim();
        if group.is_empty() || version.is_empty() {
            return None;
        }
        let kind_lc = kind.to_ascii_lowercase();
        Some(format!("{group}/{kind_lc}_{version}.json"))
    }

    fn override_file_for(&self, resource: &ResourceRef) -> Option<PathBuf> {
        Some(
            self.root_dir
                .join(Self::relative_path_for_resource(resource)?),
        )
    }

    fn load_schema_doc(&self, resource: &ResourceRef) -> Option<SchemaDoc> {
        match self.load_schema_doc_result(resource) {
            LocalSchemaDocLoad::Loaded(doc) => Some(doc),
            LocalSchemaDocLoad::NotOwned | LocalSchemaDocLoad::Error { .. } => None,
        }
    }

    fn load_schema_doc_result(&self, resource: &ResourceRef) -> LocalSchemaDocLoad {
        let Some(local) = self.override_file_for(resource) else {
            return LocalSchemaDocLoad::NotOwned;
        };
        if !local.exists() {
            return LocalSchemaDocLoad::NotOwned;
        }

        let cache_key = ResourceDocKey::from_resource(resource);
        if let Ok(guard) = self.docs.lock()
            && let Some(doc) = guard.get(&cache_key)
        {
            return LocalSchemaDocLoad::Loaded(doc.clone());
        }

        let source_path = local.display().to_string();
        let bytes = match std::fs::read(&local) {
            Ok(bytes) => bytes,
            Err(err) => {
                return LocalSchemaDocLoad::Error {
                    source_path,
                    io_error: err.to_string(),
                };
            }
        };
        let doc = match serde_json::from_slice::<Value>(&bytes) {
            Ok(doc) => SchemaDoc::new(doc),
            Err(err) => {
                return LocalSchemaDocLoad::Error {
                    source_path,
                    io_error: err.to_string(),
                };
            }
        };
        if let Ok(mut guard) = self.docs.lock() {
            guard.insert(cache_key, doc.clone());
        }
        LocalSchemaDocLoad::Loaded(doc)
    }

    fn schema_leaf_for_resource_path_from_doc(
        &self,
        root: &SchemaDoc,
        path: &YamlPath,
    ) -> Option<LocalSchemaLeaf> {
        descend_schema_path_expanding_leaf_with_root_metadata_source(root.root(), &path.0)
    }

    fn source_for_leaf(
        &self,
        resource: &ResourceRef,
        leaf: &LocalSchemaLeaf,
    ) -> Option<ProviderSchemaSource> {
        let pointer = leaf.pointer()?;
        Some(ProviderSchemaSource::new(
            ProviderOrigin::LocalOverride,
            self.root_dir.display().to_string(),
            None,
            Self::relative_path_for_resource(resource)?,
            pointer.to_string(),
        ))
    }

    fn fragment_for_leaf(
        &self,
        resource: &ResourceRef,
        root: &SchemaDoc,
        leaf: LocalSchemaLeaf,
    ) -> ProviderSchemaFragment {
        fragment_for_source_leaf(root, self.source_for_leaf(resource, &leaf), leaf)
    }
}

enum LocalSchemaDocLoad {
    Loaded(SchemaDoc),
    NotOwned,
    Error {
        source_path: String,
        io_error: String,
    },
}

impl K8sSchemaProvider for LocalSchemaProvider {
    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::LocalOverride
    }

    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len()))]
    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        match self.load_schema_doc_result(resource) {
            LocalSchemaDocLoad::Loaded(root) => {
                match self.schema_leaf_for_resource_path_from_doc(&root, path) {
                    Some(leaf) => ProviderLookupResult::Found {
                        schema: self.fragment_for_leaf(resource, &root, leaf),
                        resolved_k8s_version: None,
                    },
                    None => ProviderLookupResult::PathUnresolved,
                }
            }
            LocalSchemaDocLoad::NotOwned => ProviderLookupResult::NotOwned,
            LocalSchemaDocLoad::Error {
                source_path,
                io_error,
            } => ProviderLookupResult::ResourceDocMissing {
                io_error,
                source_path,
            },
        }
    }

    fn has_resource(&self, resource: &ResourceRef) -> bool {
        self.override_file_for(resource).is_some_and(|p| p.exists())
    }

    fn infer_api_version_candidates(&self, kind: &str) -> Vec<ApiVersionCandidate> {
        if !self.allow_api_version_guess {
            return Vec::new();
        }
        let kind_lc = kind.to_ascii_lowercase();
        let mut out = scan_crd_source_dir(&self.root_dir, &kind_lc, ProviderOrigin::LocalOverride);
        // Override-as-shortlist: stamp source=Shortlist if found locally.
        for c in &mut out {
            c.source = InferenceSource::Shortlist;
        }
        out
    }
}

/// Expand the full override document for regression tests and debugging.
///
/// Production provider lookup stays on the fragment-first path.
#[must_use]
pub fn debug_materialize_schema_for_resource(
    provider: &LocalSchemaProvider,
    resource: &ResourceRef,
) -> Option<Value> {
    let root = provider.load_schema_doc(resource)?;
    Some(debug_materialize_local_schema(root.root()))
}

#[cfg(test)]
#[path = "tests/provider.rs"]
mod tests;
