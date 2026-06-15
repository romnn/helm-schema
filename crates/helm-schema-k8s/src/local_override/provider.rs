use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::inference::cache_scan::scan_crd_source_dir;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{
    K8sSchemaProvider, ProviderLookupResult, ProviderOrigin, ProviderSchemaFragment,
    ProviderSchemaSource,
};
use crate::metadata_enrichment::{enrich_root_metadata_schema, enriched_metadata_schema};
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

    #[must_use]
    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let root = self.load_schema_doc(resource)?;
        let mut stack = std::collections::HashSet::new();
        Some(enrich_root_metadata_schema(expand_local_refs(
            root.root(),
            root.root(),
            0,
            &mut stack,
        )))
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
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        let root = self.load_schema_doc(resource)?;
        self.schema_leaf_for_resource_path_from_doc(&root, path)
            .map(LocalSchemaLeaf::into_schema)
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::LocalOverride
    }

    #[tracing::instrument(skip_all, fields(kind = resource.kind.as_str(), api_version = resource.api_version.as_str(), path_len = path.0.len()))]
    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        match self.load_schema_doc_result(resource) {
            LocalSchemaDocLoad::Loaded(root) => {
                match self.schema_leaf_for_resource_path_from_doc(&root, path) {
                    Some(leaf) => {
                        let source = self.source_for_leaf(resource, &leaf);
                        let mut fragment = ProviderSchemaFragment::new(leaf.into_schema());
                        if let Some(source) = source {
                            fragment = fragment.with_source(source);
                        }
                        ProviderLookupResult::Found {
                            schema: fragment,
                            resolved_k8s_version: None,
                        }
                    }
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

#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub fn descend_schema_path(schema: &Value, path: &[String]) -> Option<Value> {
    let mut current = schema;
    for seg in path {
        current = descend_one(current, seg)?;
    }
    Some(current.clone())
}

fn descend_one<'a>(schema: &'a Value, seg: &str) -> Option<&'a Value> {
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = schema.get(keyword).and_then(|v| v.as_array()) {
            for branch in arr {
                if let Some(v) = descend_one(branch, seg) {
                    return Some(v);
                }
            }
        }
    }

    let (key, is_array_item) = if let Some(k) = seg.strip_suffix("[*]") {
        (k, true)
    } else {
        (seg, false)
    };

    let mut next = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .and_then(|p| p.get(key))
        .or_else(|| {
            schema
                .get("additionalProperties")
                .and_then(|ap| if ap.is_boolean() { None } else { Some(ap) })
        })?;

    if is_array_item {
        next = next.get("items").or_else(|| {
            next.get("prefixItems")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
        })?;
    }

    Some(next)
}

/// Descends a schema path while resolving local `$ref`s only along that path,
/// then expands references inside the returned leaf. The result matches
/// expanding the full document before path descent without materialising the
/// full expanded resource schema for every lookup.
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub fn descend_schema_path_expanding_leaf(root: &Value, path: &[String]) -> Option<Value> {
    descend_schema_path_expanding_leaf_with_source(root, path).map(LocalSchemaLeaf::into_schema)
}

/// Source-aware result of a local schema document path descent.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalSchemaLeaf {
    schema: Value,
    pointer: Option<String>,
}

impl LocalSchemaLeaf {
    fn new(schema: Value, pointer: Option<String>) -> Self {
        Self { schema, pointer }
    }

    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    #[must_use]
    pub fn pointer(&self) -> Option<&str> {
        self.pointer.as_deref()
    }

    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }
}

/// Source-aware form of [`descend_schema_path_expanding_leaf`].
///
/// `pointer` identifies the source JSON Pointer of the resolved leaf before
/// leaf-local `$ref` expansion. It is absent when the result is synthetic or
/// no longer corresponds to one stable provider document location.
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub fn descend_schema_path_expanding_leaf_with_source(
    root: &Value,
    path: &[String],
) -> Option<LocalSchemaLeaf> {
    let mut stack = std::collections::HashSet::new();
    let leaf = descend_schema_path_node(root, root, Some(String::new()), path, 0, &mut stack)?;
    let mut expand_stack = std::collections::HashSet::new();
    Some(LocalSchemaLeaf::new(
        expand_local_refs(root, leaf.schema(), 0, &mut expand_stack),
        leaf.pointer,
    ))
}

/// Descends a schema path while applying Kubernetes metadata enrichment lazily.
///
/// Full-root enrichment is only needed for root-schema materialization. Leaf
/// lookups under `metadata` can clone and enrich just that subtree; all other
/// paths descend the raw document and expand only the resolved leaf.
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub fn descend_schema_path_expanding_leaf_with_root_metadata(
    root: &Value,
    path: &[String],
) -> Option<Value> {
    descend_schema_path_expanding_leaf_with_root_metadata_source(root, path)
        .map(LocalSchemaLeaf::into_schema)
}

/// Source-aware form of
/// [`descend_schema_path_expanding_leaf_with_root_metadata`].
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub fn descend_schema_path_expanding_leaf_with_root_metadata_source(
    root: &Value,
    path: &[String],
) -> Option<LocalSchemaLeaf> {
    let Some(first_segment) = path.first() else {
        let enriched_root = enrich_root_metadata_schema(root.clone());
        let mut stack = std::collections::HashSet::new();
        return Some(LocalSchemaLeaf::new(
            expand_local_refs(&enriched_root, &enriched_root, 0, &mut stack),
            None,
        ));
    };

    if first_segment != "metadata" {
        return descend_schema_path_expanding_leaf_with_source(root, path);
    }

    let metadata = enriched_metadata_schema(root);
    let mut stack = std::collections::HashSet::new();
    let leaf = descend_schema_path_node(root, &metadata, None, &path[1..], 0, &mut stack)?;
    let mut expand_stack = std::collections::HashSet::new();
    Some(LocalSchemaLeaf::new(
        expand_local_refs(root, leaf.schema(), 0, &mut expand_stack),
        leaf.pointer,
    ))
}

fn descend_schema_path_node(
    root: &Value,
    schema: &Value,
    pointer: Option<String>,
    path: &[String],
    depth: usize,
    stack: &mut std::collections::HashSet<String>,
) -> Option<LocalSchemaLeaf> {
    if depth > 64 {
        return Some(LocalSchemaLeaf::new(schema.clone(), pointer));
    }

    let Some((segment, remaining_path)) = path.split_first() else {
        return Some(LocalSchemaLeaf::new(schema.clone(), pointer));
    };

    let LocalSchemaLeaf {
        schema: next_schema,
        pointer: next_pointer,
    } = descend_one_expanding_refs(root, schema, pointer, segment, depth, stack)?;
    descend_schema_path_node(
        root,
        &next_schema,
        next_pointer,
        remaining_path,
        depth + 1,
        stack,
    )
}

fn descend_one_expanding_refs(
    root: &Value,
    schema: &Value,
    pointer: Option<String>,
    segment: &str,
    depth: usize,
    stack: &mut std::collections::HashSet<String>,
) -> Option<LocalSchemaLeaf> {
    let resolved = resolve_local_ref_node(root, schema, pointer, depth, stack);
    let schema = resolved.schema();

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            for (index, branch) in branches.iter().enumerate() {
                let branch_pointer =
                    pointer_with_segments(resolved.pointer(), &[keyword, &index.to_string()]);
                if let Some(next) = descend_one_expanding_refs(
                    root,
                    branch,
                    branch_pointer,
                    segment,
                    depth + 1,
                    stack,
                ) {
                    return Some(next);
                }
            }
        }
    }

    let (key, is_array_item) = segment
        .strip_suffix("[*]")
        .map_or((segment, false), |key| (key, true));

    let mut next = if let Some(property) = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(key))
    {
        LocalSchemaLeaf::new(
            property.clone(),
            pointer_with_segments(resolved.pointer(), &["properties", key]),
        )
    } else {
        let additional_properties =
            schema
                .get("additionalProperties")
                .and_then(|additional_properties| {
                    if additional_properties.is_boolean() {
                        None
                    } else {
                        Some(additional_properties)
                    }
                })?;
        LocalSchemaLeaf::new(
            additional_properties.clone(),
            pointer_with_segments(resolved.pointer(), &["additionalProperties"]),
        )
    };

    if is_array_item {
        let LocalSchemaLeaf {
            schema: next_schema,
            pointer: next_pointer,
        } = next;
        next = resolve_local_ref_node(root, &next_schema, next_pointer, depth + 1, stack);
        if let Some(items) = next.schema().get("items") {
            next = LocalSchemaLeaf::new(
                items.clone(),
                pointer_with_segments(next.pointer(), &["items"]),
            );
        } else {
            let item = next
                .schema()
                .get("prefixItems")
                .and_then(Value::as_array)
                .and_then(|items| items.first())?;
            next = LocalSchemaLeaf::new(
                item.clone(),
                pointer_with_segments(next.pointer(), &["prefixItems", "0"]),
            );
        }
    }

    Some(next)
}

fn resolve_local_ref_node(
    root: &Value,
    schema: &Value,
    pointer: Option<String>,
    depth: usize,
    stack: &mut std::collections::HashSet<String>,
) -> LocalSchemaLeaf {
    if depth > 64 {
        return LocalSchemaLeaf::new(schema.clone(), pointer);
    }
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return LocalSchemaLeaf::new(schema.clone(), pointer);
    };
    if stack.contains(reference) {
        return LocalSchemaLeaf::new(strip_ref(schema), None);
    }
    stack.insert(reference.to_string());

    let resolved = if let Some(pointer) = reference.strip_prefix('#') {
        root.pointer(pointer).map_or_else(
            || LocalSchemaLeaf::new(strip_ref(schema), None),
            |target| {
                resolve_local_ref_node(root, target, Some(pointer.to_string()), depth + 1, stack)
            },
        )
    } else {
        LocalSchemaLeaf::new(strip_ref(schema), None)
    };

    stack.remove(reference);
    resolved
}

fn pointer_with_segments(base: Option<&str>, segments: &[&str]) -> Option<String> {
    let mut out = base?.to_string();
    for segment in segments {
        out.push('/');
        out.push_str(&escape_json_pointer_segment(segment));
    }
    Some(out)
}

fn escape_json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub fn expand_local_refs(
    root: &Value,
    schema: &Value,
    depth: usize,
    stack: &mut std::collections::HashSet<String>,
) -> Value {
    if depth > 64 {
        return schema.clone();
    }

    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        if stack.contains(r) {
            return strip_ref(schema);
        }
        stack.insert(r.to_string());

        let out = if let Some(ptr) = r.strip_prefix('#') {
            root.pointer(ptr).map_or_else(
                || strip_ref(schema),
                |target| expand_local_refs(root, target, depth + 1, stack),
            )
        } else {
            strip_ref(schema)
        };

        stack.remove(r);
        return out;
    }

    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };

    let mut out = obj.clone();

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = out.get(keyword).and_then(|v| v.as_array()) {
            let expanded: Vec<Value> = arr
                .iter()
                .map(|v| expand_local_refs(root, v, depth + 1, stack))
                .collect();
            out.insert(keyword.to_string(), Value::Array(expanded));
        }
    }

    for map_key in ["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(m) = out.get(map_key).and_then(|v| v.as_object()) {
            let mut new_m = serde_json::Map::new();
            for (k, v) in m {
                new_m.insert(k.clone(), expand_local_refs(root, v, depth + 1, stack));
            }
            out.insert(map_key.to_string(), Value::Object(new_m));
        }
    }

    for single_key in [
        "items",
        "contains",
        "not",
        "if",
        "then",
        "else",
        "additionalProperties",
    ] {
        if let Some(v) = out.get(single_key).cloned()
            && !v.is_boolean()
        {
            out.insert(
                single_key.to_string(),
                expand_local_refs(root, &v, depth + 1, stack),
            );
        }
    }

    Value::Object(out)
}

fn strip_ref(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = obj.clone();
    out.remove("$ref");
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use helm_schema_ir::{ResourceRef, YamlPath};
    use serde_json::json;

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

        assert_eq!(actual, expected);
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

        assert_eq!(leaf.schema(), &json!({ "type": "integer" }));
        assert_eq!(leaf.pointer(), Some("/definitions/Spec/properties/size"));
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
        assert_eq!(metadata_name, json!({ "type": "string" }));

        let metadata_name_leaf = descend_schema_path_expanding_leaf_with_root_metadata_source(
            &root,
            &["metadata".to_string(), "name".to_string()],
        )
        .expect("metadata.name should be synthesized from object metadata");
        assert_eq!(metadata_name_leaf.pointer(), None);

        let metadata_labels = descend_schema_path_expanding_leaf_with_root_metadata(
            &root,
            &["metadata".to_string(), "labels".to_string()],
        )
        .expect("metadata.labels should resolve local refs");
        assert_eq!(
            metadata_labels,
            json!({
                "type": "object",
                "additionalProperties": { "type": "string" }
            })
        );

        let spec_replicas = descend_schema_path_expanding_leaf_with_root_metadata(
            &root,
            &["spec".to_string(), "replicas".to_string()],
        )
        .expect("non-metadata path should still descend the raw document");
        assert_eq!(spec_replicas, json!({ "type": "integer" }));
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

        assert_eq!(source.origin(), ProviderOrigin::LocalOverride);
        assert_eq!(source.source_id(), root_dir.display().to_string());
        assert_eq!(source.filename(), "example.com/widget_v1.json");
        assert_eq!(source.pointer(), "/definitions/Spec/properties/size");
    }
}
