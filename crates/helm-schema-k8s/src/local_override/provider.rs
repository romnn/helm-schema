use std::path::PathBuf;

use helm_schema_ir::{ResourceRef, YamlPath};
use serde_json::Value;

use crate::inference::cache_scan::scan_crd_source_dir;
use crate::inference::{ApiVersionCandidate, InferenceSource};
use crate::lookup::{K8sSchemaProvider, ProviderLookupResult, ProviderOrigin};

#[derive(Debug, Clone)]
pub struct LocalSchemaProvider {
    root_dir: PathBuf,
    allow_api_version_guess: bool,
}

impl LocalSchemaProvider {
    #[must_use]
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_dir: root_dir.into(),
            allow_api_version_guess: false,
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

    fn load_schema_doc(&self, resource: &ResourceRef) -> Option<Value> {
        let local = self.override_file_for(resource)?;
        let bytes = std::fs::read(local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;
        Some(doc)
    }

    #[must_use]
    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let root = self.load_schema_doc(resource)?;
        let mut stack = std::collections::HashSet::new();
        Some(expand_local_refs(&root, &root, 0, &mut stack))
    }
}

impl K8sSchemaProvider for LocalSchemaProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        let root = self.materialize_schema_for_resource(resource)?;
        descend_schema_path(&root, &path.0)
    }

    fn origin(&self) -> ProviderOrigin {
        ProviderOrigin::LocalOverride
    }

    fn lookup(&self, resource: &ResourceRef, path: &YamlPath) -> ProviderLookupResult {
        let Some(file) = self.override_file_for(resource) else {
            return ProviderLookupResult::NotOwned;
        };
        if !file.exists() {
            return ProviderLookupResult::NotOwned;
        }
        // Override is claimed; any read failure now is a hard error.
        let source_path = file.display().to_string();
        match std::fs::read(&file) {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(root) => {
                    let mut stack = std::collections::HashSet::new();
                    let expanded = expand_local_refs(&root, &root, 0, &mut stack);
                    match descend_schema_path(&expanded, &path.0) {
                        Some(schema) => ProviderLookupResult::Found {
                            schema,
                            resolved_k8s_version: None,
                        },
                        None => ProviderLookupResult::PathUnresolved,
                    }
                }
                Err(err) => ProviderLookupResult::ResourceDocMissing {
                    io_error: err.to_string(),
                    source_path,
                },
            },
            Err(err) => ProviderLookupResult::ResourceDocMissing {
                io_error: err.to_string(),
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

pub fn descend_schema_path(schema: &Value, path: &[String]) -> Option<Value> {
    let mut current = schema.clone();
    for seg in path {
        current = descend_one(&current, seg)?;
    }
    Some(current)
}

fn descend_one(schema: &Value, seg: &str) -> Option<Value> {
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
        .cloned()
        .or_else(|| {
            schema.get("additionalProperties").and_then(|ap| {
                if ap.is_boolean() {
                    None
                } else {
                    Some(ap.clone())
                }
            })
        })?;

    if is_array_item {
        next = next.get("items").cloned().or_else(|| {
            next.get("prefixItems")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .cloned()
        })?;
    }

    Some(next)
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
