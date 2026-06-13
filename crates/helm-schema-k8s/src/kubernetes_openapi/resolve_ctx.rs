use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::schema_doc::SchemaDoc;

/// `$ref` resolution context. Holds previously-loaded documents and a
/// stack of (filename, json-pointer) pairs to break cycles.
///
/// The context is short-lived: one per top-level `schema_for_resource_path`
/// call. The provider supplies a loader that knows how to fetch a
/// neighboring schema file by relative filename (typically by mapping
/// the filename into the K8s cache layout and calling the same fetch
/// path the resource doc came from).
pub struct ResolveCtx<F: FnMut(&str) -> Option<PathBuf>> {
    loader: F,
    docs: HashMap<String, SchemaDoc>,
    stack: HashSet<(String, String)>,
}

impl<F: FnMut(&str) -> Option<PathBuf>> ResolveCtx<F> {
    pub fn new(loader: F, root_filename: String, root_doc: SchemaDoc) -> Self {
        let mut docs = HashMap::new();
        docs.insert(root_filename, root_doc);
        Self {
            loader,
            docs,
            stack: HashSet::new(),
        }
    }

    fn normalize_ref_filename(current_filename: &str, file: &str) -> String {
        if file.is_empty() {
            return current_filename.to_string();
        }
        let trimmed = file.trim().trim_start_matches("./");
        trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
    }

    pub fn doc(&self, filename: &str) -> Option<&Value> {
        self.docs.get(filename).map(SchemaDoc::root)
    }

    fn load_doc(&mut self, filename: &str) -> Option<&Value> {
        if self.docs.contains_key(filename) {
            return self.doc(filename);
        }
        let local = (self.loader)(filename)?;
        let bytes = fs::read(&local).ok()?;
        let doc: Value = serde_json::from_slice(&bytes).ok()?;
        self.docs.insert(filename.to_string(), SchemaDoc::new(doc));
        self.doc(filename)
    }

    fn resolve_ref(&mut self, current_filename: &str, r: &str) -> Option<(String, Value)> {
        if let Some(ptr) = r.strip_prefix('#') {
            let doc = self.doc(current_filename)?;
            return doc
                .pointer(ptr)
                .cloned()
                .map(|v| (current_filename.to_string(), v));
        }
        let (file, ptr) = r.split_once('#').unwrap_or((r, ""));
        let filename = Self::normalize_ref_filename(current_filename, file);
        let doc = self.load_doc(&filename)?.clone();
        if ptr.is_empty() {
            Some((filename, doc))
        } else {
            doc.pointer(ptr).cloned().map(|v| (filename, v))
        }
    }
}

fn strip_ref(schema: &Value) -> Value {
    let Some(obj) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = obj.clone();
    out.remove("$ref");
    Value::Object(out)
}

pub fn expand_schema_node<F: FnMut(&str) -> Option<PathBuf>>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    depth: usize,
) -> (String, Value) {
    if depth > 64 {
        return (current_filename.to_string(), schema.clone());
    }

    if let Some(r) = schema.get("$ref").and_then(|v| v.as_str()) {
        let key = if let Some(ptr) = r.strip_prefix('#') {
            (current_filename.to_string(), format!("#{ptr}"))
        } else {
            let (file, ptr) = r.split_once('#').unwrap_or((r, ""));
            let filename = ResolveCtx::<F>::normalize_ref_filename(current_filename, file);
            (filename, format!("#{ptr}"))
        };

        if ctx.stack.contains(&key) {
            return (current_filename.to_string(), strip_ref(schema));
        }
        ctx.stack.insert(key.clone());

        let out = if let Some((nf, target)) = ctx.resolve_ref(current_filename, r) {
            expand_schema_node(ctx, &nf, &target, depth + 1)
        } else {
            (current_filename.to_string(), strip_ref(schema))
        };

        ctx.stack.remove(&key);
        return out;
    }

    for keyword in &["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = schema.get(*keyword).and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for s in arr {
                out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
            }
            let mut obj = schema.as_object().cloned().unwrap_or_default();
            obj.insert((*keyword).to_string(), Value::Array(out));
            return (current_filename.to_string(), Value::Object(obj));
        }
    }

    let mut obj = match schema.as_object() {
        Some(o) => o.clone(),
        None => return (current_filename.to_string(), schema.clone()),
    };

    for prop_key in &["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(props) = obj.get(*prop_key).and_then(|v| v.as_object()) {
            let mut new_props = Map::new();
            for (k, v) in props {
                new_props.insert(
                    k.clone(),
                    expand_schema_node(ctx, current_filename, v, depth + 1).1,
                );
            }
            obj.insert((*prop_key).to_string(), Value::Object(new_props));
        }
    }

    for single_key in &["items", "contains", "not", "if", "then", "else"] {
        if let Some(sub) = obj.get(*single_key) {
            let sub = sub.clone();
            obj.insert(
                (*single_key).to_string(),
                expand_schema_node(ctx, current_filename, &sub, depth + 1).1,
            );
        }
    }

    for array_key in &["prefixItems"] {
        if let Some(arr) = obj.get(*array_key).and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for s in arr {
                out.push(expand_schema_node(ctx, current_filename, s, depth + 1).1);
            }
            obj.insert((*array_key).to_string(), Value::Array(out));
        }
    }

    if let Some(ds) = obj.get("dependentSchemas").and_then(|v| v.as_object()) {
        let mut out = Map::new();
        for (k, v) in ds {
            out.insert(
                k.clone(),
                expand_schema_node(ctx, current_filename, v, depth + 1).1,
            );
        }
        obj.insert("dependentSchemas".to_string(), Value::Object(out));
    }

    if let Some(ap) = obj.get("additionalProperties")
        && !ap.is_boolean()
    {
        let ap = ap.clone();
        obj.insert(
            "additionalProperties".to_string(),
            expand_schema_node(ctx, current_filename, &ap, depth + 1).1,
        );
    }

    (current_filename.to_string(), Value::Object(obj))
}
