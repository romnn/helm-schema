use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::schema_doc::SchemaDoc;

/// `$ref` resolution context. Holds previously-loaded documents and a
/// stack of (filename, json-pointer) pairs to break cycles.
///
/// The context is short-lived: one per top-level `schema_for_resource_path`
/// call. The provider supplies a loader that knows how to fetch a
/// neighboring schema file by relative filename (typically by mapping
/// the filename through the same provider fetch/cache path the
/// resource doc came from).
pub struct ResolveCtx<F: FnMut(&str) -> Option<SchemaDoc>> {
    loader: F,
    docs: HashMap<String, SchemaDoc>,
    stack: HashSet<(String, String)>,
}

impl<F: FnMut(&str) -> Option<SchemaDoc>> ResolveCtx<F> {
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
        let doc = (self.loader)(filename)?;
        self.docs.insert(filename.to_string(), doc);
        self.doc(filename)
    }

    fn resolve_ref(&mut self, current_filename: &str, reference: &str) -> Option<(String, Value)> {
        if let Some(ptr) = reference.strip_prefix('#') {
            let doc = self.doc(current_filename)?;
            return doc
                .pointer(ptr)
                .cloned()
                .map(|v| (current_filename.to_string(), v));
        }
        let (file, ptr) = reference.split_once('#').unwrap_or((reference, ""));
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

pub fn expand_schema_node<F: FnMut(&str) -> Option<SchemaDoc>>(
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

pub fn descend_schema_path_expanding_leaf<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    path: &[String],
) -> Option<Value> {
    let (leaf_filename, leaf_schema) =
        descend_schema_path_node(ctx, current_filename, schema, path, 0)?;
    Some(expand_schema_node(ctx, &leaf_filename, &leaf_schema, 0).1)
}

fn descend_schema_path_node<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    path: &[String],
    depth: usize,
) -> Option<(String, Value)> {
    if depth > 64 {
        return Some((current_filename.to_string(), schema.clone()));
    }

    let Some((segment, remaining_path)) = path.split_first() else {
        return Some((current_filename.to_string(), schema.clone()));
    };

    let (next_filename, next_schema) =
        descend_one_schema_path_segment(ctx, current_filename, schema, segment, depth)?;
    descend_schema_path_node(ctx, &next_filename, &next_schema, remaining_path, depth + 1)
}

fn descend_one_schema_path_segment<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    segment: &str,
    depth: usize,
) -> Option<(String, Value)> {
    let (schema_filename, schema) = resolve_direct_ref(ctx, current_filename, schema, depth)?;

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            for branch in branches {
                if let Some(next) = descend_one_schema_path_segment(
                    ctx,
                    &schema_filename,
                    branch,
                    segment,
                    depth + 1,
                ) {
                    return Some(next);
                }
            }
        }
    }

    let (key, is_array_item) = segment
        .strip_suffix("[*]")
        .map_or((segment, false), |key| (key, true));

    let mut next = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| properties.get(key))
        .or_else(|| {
            schema
                .get("additionalProperties")
                .and_then(|additional_properties| {
                    if additional_properties.is_boolean() {
                        None
                    } else {
                        Some(additional_properties)
                    }
                })
        })?
        .clone();
    let mut next_filename = schema_filename;

    if is_array_item {
        (next_filename, next) = resolve_direct_ref(ctx, &next_filename, &next, depth + 1)?;
        next = next
            .get("items")
            .or_else(|| {
                next.get("prefixItems")
                    .and_then(Value::as_array)
                    .and_then(|items| items.first())
            })?
            .clone();
    }

    Some((next_filename, next))
}

fn resolve_direct_ref<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    depth: usize,
) -> Option<(String, Value)> {
    if depth > 64 {
        return Some((current_filename.to_string(), schema.clone()));
    }
    let Some(reference) = schema.get("$ref").and_then(Value::as_str) else {
        return Some((current_filename.to_string(), schema.clone()));
    };

    let key = if let Some(pointer) = reference.strip_prefix('#') {
        (current_filename.to_string(), format!("#{pointer}"))
    } else {
        let (file, pointer) = reference.split_once('#').unwrap_or((reference, ""));
        let filename = ResolveCtx::<F>::normalize_ref_filename(current_filename, file);
        (filename, format!("#{pointer}"))
    };

    if ctx.stack.contains(&key) {
        return Some((current_filename.to_string(), strip_ref(schema)));
    }
    ctx.stack.insert(key.clone());

    let resolved = ctx
        .resolve_ref(current_filename, reference)
        .and_then(|(filename, target)| resolve_direct_ref(ctx, &filename, &target, depth + 1));

    ctx.stack.remove(&key);
    resolved.or_else(|| Some((current_filename.to_string(), strip_ref(schema))))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use super::*;

    #[test]
    fn lazy_path_descent_matches_full_expansion_for_cross_file_array_ref() {
        let root = SchemaDoc::new(json!({
            "type": "object",
            "properties": {
                "spec": {
                    "$ref": "defs.json#/definitions/Spec"
                }
            }
        }));
        let definitions = SchemaDoc::new(json!({
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
        }));
        let docs = HashMap::from([("defs.json".to_string(), definitions.clone())]);
        let path = vec![
            "spec".to_string(),
            "containers[*]".to_string(),
            "env".to_string(),
        ];

        let mut full_ctx = ResolveCtx::new(
            {
                let docs = docs.clone();
                move |filename| docs.get(filename).cloned()
            },
            "root.json".to_string(),
            root.clone(),
        );
        let expanded_root = expand_schema_node(&mut full_ctx, "root.json", root.root(), 0).1;
        let expected = crate::local_override::descend_schema_path(&expanded_root, &path)
            .expect("expanded root should contain path");

        let mut lazy_ctx = ResolveCtx::new(
            move |filename| docs.get(filename).cloned(),
            "root.json".to_string(),
            root,
        );
        let lazy_root = lazy_ctx
            .doc("root.json")
            .cloned()
            .expect("root doc should be present");
        let actual =
            descend_schema_path_expanding_leaf(&mut lazy_ctx, "root.json", &lazy_root, &path)
                .expect("lazy descent should contain path");

        assert_eq!(actual, expected);
    }
}
