use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use helm_schema_core::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::doc_backed_schema::{
    LocalSchemaLeaf, debug_materialize_local_schema,
    descend_schema_path_expanding_leaf_with_root_metadata_source, fragment_for_source_leaf,
};
#[cfg(test)]
use crate::doc_backed_schema::{
    descend_schema_path, descend_schema_path_expanding_leaf,
    descend_schema_path_expanding_leaf_with_root_metadata,
    descend_schema_path_expanding_leaf_with_source, expand_local_refs,
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
    fn schema_fragment_for_resource_path(
        &self,
        resource: &ResourceRef,
        path: &YamlPath,
    ) -> Option<ProviderSchemaFragment> {
        let root = self.load_schema_doc(resource)?;
        self.schema_leaf_for_resource_path_from_doc(&root, path)
            .map(|leaf| self.fragment_for_leaf(resource, &root, leaf))
    }

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

impl helm_schema_core::ResourceSchemaOracle for LocalSchemaProvider {
    fn schema_fragment_for_use(
        &self,
        use_: &helm_schema_core::ProviderSchemaUse,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        <Self as K8sSchemaProvider>::schema_fragment_for_use(self, use_)
    }

    fn schema_fragment_for_resource_path(
        &self,
        resource: &helm_schema_core::ResourceRef,
        path: &helm_schema_core::YamlPath,
    ) -> Option<helm_schema_core::ProviderSchemaFragment> {
        <Self as K8sSchemaProvider>::schema_fragment_for_resource_path(self, resource, path)
    }

    fn origin(&self) -> helm_schema_core::ProviderOrigin {
        <Self as K8sSchemaProvider>::origin(self)
    }

    fn has_resource(&self, resource: &helm_schema_core::ResourceRef) -> bool {
        <Self as K8sSchemaProvider>::has_resource(self, resource)
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
mod tests {
    use helm_schema_core::{ResourceRef, YamlPath};
    use serde_json::json;
    use test_util::prelude::sim_assert_eq;

    use super::*;

    fn widget_resource() -> ResourceRef {
        ResourceRef {
            api_version: "example.com/v1".to_string(),
            kind: "Widget".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }
    }

    #[test]
    fn lazy_local_path_descent_matches_full_expansion_for_array_ref() {
        let root = json!({
            "type": "object",
            "properties": {
                "spec": {
                    "$ref": "#/definitions/Spec"
                }
            },
            "definitions": {
                "Spec": {
                    "type": "object",
                    "properties": {
                        "containers": {
                            "type": "array",
                            "items": {
                                "$ref": "#/definitions/Container"
                            }
                        }
                    }
                },
                "Container": {
                    "type": "object",
                    "properties": {
                        "env": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "string"
                            }
                        }
                    }
                }
            }
        });
        let path = vec![
            "spec".to_string(),
            "containers[*]".to_string(),
            "env".to_string(),
        ];

        let mut stack = std::collections::HashSet::new();
        let expanded = expand_local_refs(&root, &root, 0, &mut stack);
        let expected =
            descend_schema_path(&expanded, &path).expect("expanded root should contain path");
        let actual = descend_schema_path_expanding_leaf(&root, &path)
            .expect("lazy descent should contain path");

        sim_assert_eq!(have: actual, want: expected);
    }

    #[test]
    fn source_aware_local_path_descent_reports_ref_target_pointer() {
        let root = json!({
            "type": "object",
            "properties": {
                "spec": {
                    "$ref": "#/definitions/Spec"
                }
            },
            "definitions": {
                "Spec": {
                    "type": "object",
                    "properties": {
                        "size": { "type": "integer" }
                    }
                }
            }
        });

        let leaf = descend_schema_path_expanding_leaf_with_source(
            &root,
            &["spec".to_string(), "size".to_string()],
        )
        .expect("lazy descent should resolve ref-backed path");

        sim_assert_eq!(have: leaf.schema(), want: &json!({ "type": "integer" }));
        sim_assert_eq!(have: leaf.source_schema(), want: Some(&json!({ "type": "integer" })));
        sim_assert_eq!(have: leaf.pointer(), want: Some("/definitions/Spec/properties/size"));
    }

    #[test]
    fn source_aware_local_path_descent_preserves_raw_leaf_before_expansion() {
        let root = json!({
            "type": "object",
            "properties": {
                "spec": {
                    "$ref": "#/definitions/Spec"
                }
            },
            "definitions": {
                "Spec": {
                    "type": "object",
                    "properties": {
                        "labels": { "$ref": "#/definitions/StringMap" }
                    }
                },
                "StringMap": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        });

        let leaf = descend_schema_path_expanding_leaf_with_source(
            &root,
            &["spec".to_string(), "labels".to_string()],
        )
        .expect("lazy descent should resolve ref-backed leaf");

        sim_assert_eq!(
            have: leaf.source_schema(),
            want: Some(&json!({ "$ref": "#/definitions/StringMap" }))
        );
        sim_assert_eq!(
            have: leaf.schema(),
            want: &json!({
                "type": "object",
                "additionalProperties": { "type": "string" }
            })
        );
        sim_assert_eq!(have: leaf.pointer(), want: Some("/definitions/Spec/properties/labels"));
    }

    #[test]
    fn lazy_root_metadata_descent_enriches_only_metadata_leaf() {
        let root = json!({
            "type": "object",
            "properties": {
                "metadata": {
                    "type": "object",
                    "properties": {
                        "labels": { "$ref": "#/definitions/StringMap" }
                    }
                },
                "spec": {
                    "type": "object",
                    "properties": {
                        "replicas": { "type": "integer" }
                    }
                }
            },
            "definitions": {
                "StringMap": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        });

        let metadata_name = descend_schema_path_expanding_leaf_with_root_metadata(
            &root,
            &["metadata".to_string(), "name".to_string()],
        )
        .expect("metadata.name should be synthesized from object metadata");
        sim_assert_eq!(have: metadata_name, want: json!({ "type": "string" }));

        let metadata_name_leaf = descend_schema_path_expanding_leaf_with_root_metadata_source(
            &root,
            &["metadata".to_string(), "name".to_string()],
        )
        .expect("metadata.name should be synthesized from object metadata");
        sim_assert_eq!(have: metadata_name_leaf.pointer(), want: None);

        let metadata_labels = descend_schema_path_expanding_leaf_with_root_metadata(
            &root,
            &["metadata".to_string(), "labels".to_string()],
        )
        .expect("metadata.labels should resolve local refs");
        sim_assert_eq!(
            have: metadata_labels,
            want: json!({
                "type": "object",
                "additionalProperties": { "type": "string" }
            })
        );

        let spec_replicas = descend_schema_path_expanding_leaf_with_root_metadata(
            &root,
            &["spec".to_string(), "replicas".to_string()],
        )
        .expect("non-metadata path should still descend the raw document");
        sim_assert_eq!(have: spec_replicas, want: json!({ "type": "integer" }));
    }

    #[test]
    fn local_override_lookup_attaches_provider_source() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let root_dir =
            std::env::temp_dir().join(format!("helm-schema-local-override-source-{unique}"));
        let group_dir = root_dir.join("example.com");
        std::fs::create_dir_all(&group_dir).expect("create local override test directory");
        std::fs::write(
            group_dir.join("widget_v1.json"),
            serde_json::to_vec(&json!({
                "type": "object",
                "properties": {
                    "spec": {
                        "$ref": "#/definitions/Spec"
                    }
                },
                "definitions": {
                    "Spec": {
                        "type": "object",
                        "properties": {
                            "size": { "type": "integer" }
                        }
                    }
                }
            }))
            .expect("serialize local override schema"),
        )
        .expect("write local override schema");

        let provider = LocalSchemaProvider::new(&root_dir);
        let result = provider.lookup(
            &widget_resource(),
            &YamlPath(vec!["spec".to_string(), "size".to_string()]),
        );
        let ProviderLookupResult::Found { schema, .. } = result else {
            panic!("local override lookup should resolve spec.size");
        };
        let source = schema
            .source()
            .expect("local override source should attach");

        sim_assert_eq!(have: source.origin(), want: ProviderOrigin::LocalOverride);
        sim_assert_eq!(have: source.source_id(), want: root_dir.display().to_string());
        sim_assert_eq!(have: source.filename(), want: "example.com/widget_v1.json");
        sim_assert_eq!(have: source.pointer(), want: "/definitions/Spec/properties/size");
    }
}
