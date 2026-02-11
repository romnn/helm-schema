use std::collections::HashMap;
use std::path::PathBuf;

use helm_schema_ir::{ResourceRef, YamlPath};
use serde::Deserialize;
use serde_json::Value;

use crate::K8sSchemaProvider;

#[derive(Debug, Clone)]
pub struct CrdCatalogSchemaProvider {
    root_dir: PathBuf,
    index: HashMap<String, HashMap<String, String>>, // apiVersion -> kind -> filename
}

impl CrdCatalogSchemaProvider {
    pub fn new(root_dir: impl Into<PathBuf>) -> Option<Self> {
        let root_dir = root_dir.into();
        let index_path = root_dir.join("index.yaml");
        let bytes = std::fs::read(&index_path).ok()?;
        let index_yaml: IndexYaml = serde_yaml::from_slice(&bytes).ok()?;

        let mut index: HashMap<String, HashMap<String, String>> = HashMap::new();
        for (_group, entries) in index_yaml.0 {
            for e in entries {
                index
                    .entry(e.api_version.clone())
                    .or_default()
                    .insert(e.kind.clone(), e.filename.clone());
            }
        }

        Some(Self { root_dir, index })
    }

    fn load_schema_doc(&self, resource: &ResourceRef) -> Option<Value> {
        let api_version = resource.api_version.trim();
        let kind = resource.kind.trim();
        if api_version.is_empty() || kind.is_empty() {
            return None;
        }

        let filename = self
            .index
            .get(api_version)
            .and_then(|m| m.get(kind))
            .cloned()?;

        let local = self.root_dir.join(filename);
        let bytes = std::fs::read(local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;
        Some(doc)
    }

    pub fn materialize_schema_for_resource(&self, resource: &ResourceRef) -> Option<Value> {
        let root = self.load_schema_doc(resource)?;
        let mut stack = std::collections::HashSet::new();
        Some(expand_local_refs(&root, &root, 0, &mut stack))
    }
}

impl K8sSchemaProvider for CrdCatalogSchemaProvider {
    fn schema_for_resource_path(&self, resource: &ResourceRef, path: &YamlPath) -> Option<Value> {
        let root = self.materialize_schema_for_resource(resource)?;
        descend_schema_path(&root, &path.0)
    }
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    #[serde(rename = "apiVersion")]
    api_version: String,
    filename: String,
    kind: String,
}

#[derive(Debug, Deserialize)]
struct IndexYaml(HashMap<String, Vec<IndexEntry>>);

fn descend_schema_path(schema: &Value, path: &[String]) -> Option<Value> {
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

fn expand_local_refs(
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
            root.pointer(ptr)
                .map(|target| expand_local_refs(root, target, depth + 1, stack))
                .unwrap_or_else(|| strip_ref(schema))
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
        if let Some(v) = out.get(single_key).cloned() {
            if !v.is_boolean() {
                out.insert(
                    single_key.to_string(),
                    expand_local_refs(root, &v, depth + 1, stack),
                );
            }
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
