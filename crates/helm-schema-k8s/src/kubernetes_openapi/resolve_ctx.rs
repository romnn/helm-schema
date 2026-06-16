use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::schema_doc::SchemaDoc;

/// `$ref` resolution context. Holds previously-loaded documents and a
/// stack of (filename, json-pointer) pairs to break cycles.
///
/// The context is short-lived: one per top-level provider fragment lookup.
/// The provider supplies a loader that knows how to fetch a
/// neighboring schema file by relative filename (typically by mapping
/// the filename through the same provider fetch/cache path the
/// resource doc came from).
pub struct ResolveCtx<F: FnMut(&str) -> Option<SchemaDoc>> {
    loader: F,
    docs: HashMap<String, SchemaDoc>,
    stack: HashSet<(String, String)>,
}

/// Source location of a schema node inside the provider document graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaNodeLocation {
    filename: String,
    pointer: String,
}

impl SchemaNodeLocation {
    fn root(filename: impl Into<String>) -> Self {
        Self {
            filename: filename.into(),
            pointer: String::new(),
        }
    }

    fn child(&self, segment: impl AsRef<str>) -> Self {
        Self {
            filename: self.filename.clone(),
            pointer: append_json_pointer_segment(&self.pointer, segment.as_ref()),
        }
    }

    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    #[must_use]
    pub fn pointer(&self) -> &str {
        &self.pointer
    }
}

/// Schema node plus the provider-document location it was read from.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSchemaNode {
    location: SchemaNodeLocation,
    schema: Value,
}

impl ResolvedSchemaNode {
    fn root(filename: impl Into<String>, schema: Value) -> Self {
        Self {
            location: SchemaNodeLocation::root(filename),
            schema,
        }
    }

    fn child(&self, segment: impl AsRef<str>, schema: Value) -> Self {
        Self {
            location: self.location.child(segment),
            schema,
        }
    }

    fn nested_child(
        &self,
        first_segment: impl AsRef<str>,
        second_segment: impl AsRef<str>,
        schema: Value,
    ) -> Self {
        Self {
            location: self.location.child(first_segment).child(second_segment),
            schema,
        }
    }

    fn at(location: SchemaNodeLocation, schema: Value) -> Self {
        Self { location, schema }
    }

    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }

    #[must_use]
    pub fn location(&self) -> &SchemaNodeLocation {
        &self.location
    }
}

/// Path lookup result with both the materialized leaf and original source leaf.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedSchemaLeaf {
    location: SchemaNodeLocation,
    source_schema: Value,
    schema: Value,
}

impl ResolvedSchemaLeaf {
    fn new(location: SchemaNodeLocation, source_schema: Value, schema: Value) -> Self {
        Self {
            location,
            source_schema,
            schema,
        }
    }

    #[must_use]
    pub fn location(&self) -> &SchemaNodeLocation {
        &self.location
    }

    #[must_use]
    pub fn source_schema(&self) -> &Value {
        &self.source_schema
    }

    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }
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

    fn resolve_ref(
        &mut self,
        current_filename: &str,
        reference: &str,
    ) -> Option<ResolvedSchemaNode> {
        if let Some(pointer) = reference.strip_prefix('#') {
            let doc = self.doc(current_filename)?;
            return doc.pointer(pointer).cloned().map(|schema| {
                ResolvedSchemaNode::at(
                    SchemaNodeLocation {
                        filename: current_filename.to_string(),
                        pointer: pointer.to_string(),
                    },
                    schema,
                )
            });
        }
        let (file, pointer) = reference.split_once('#').unwrap_or((reference, ""));
        let filename = Self::normalize_ref_filename(current_filename, file);
        let doc = self.load_doc(&filename)?.clone();
        if pointer.is_empty() {
            Some(ResolvedSchemaNode::root(filename, doc))
        } else {
            doc.pointer(pointer).cloned().map(|schema| {
                ResolvedSchemaNode::at(
                    SchemaNodeLocation {
                        filename,
                        pointer: pointer.to_string(),
                    },
                    schema,
                )
            })
        }
    }

    pub(crate) fn resolve_schema_reference(
        &mut self,
        current_filename: &str,
        reference: &str,
    ) -> Option<ResolvedSchemaNode> {
        self.resolve_ref(current_filename, reference)
    }
}

fn append_json_pointer_segment(pointer: &str, segment: &str) -> String {
    let escaped = segment.replace('~', "~0").replace('/', "~1");
    if pointer.is_empty() {
        format!("/{escaped}")
    } else {
        format!("{pointer}/{escaped}")
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
    let node = ResolvedSchemaNode::root(current_filename.to_string(), schema.clone());
    let expanded = expand_schema_node_at(ctx, node, depth);
    (
        expanded.location.filename().to_string(),
        expanded.into_schema(),
    )
}

fn expand_schema_node_at<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    node: ResolvedSchemaNode,
    depth: usize,
) -> ResolvedSchemaNode {
    if depth > 64 {
        return node;
    }

    if let Some(reference) = node.schema.get("$ref").and_then(|v| v.as_str()) {
        let key = if let Some(pointer) = reference.strip_prefix('#') {
            (node.location.filename().to_string(), format!("#{pointer}"))
        } else {
            let (file, pointer) = reference.split_once('#').unwrap_or((reference, ""));
            let filename = ResolveCtx::<F>::normalize_ref_filename(node.location.filename(), file);
            (filename, format!("#{pointer}"))
        };

        if ctx.stack.contains(&key) {
            return ResolvedSchemaNode::at(node.location, strip_ref(&node.schema));
        }
        ctx.stack.insert(key.clone());

        let out = if let Some(target) = ctx.resolve_ref(node.location.filename(), reference) {
            expand_schema_node_at(ctx, target, depth + 1)
        } else {
            ResolvedSchemaNode::at(node.location, strip_ref(&node.schema))
        };

        ctx.stack.remove(&key);
        return out;
    }

    for keyword in &["allOf", "anyOf", "oneOf"] {
        if let Some(arr) = node.schema.get(*keyword).and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for (index, schema) in arr.iter().enumerate() {
                out.push(
                    expand_schema_node_at(
                        ctx,
                        node.nested_child(*keyword, index.to_string(), schema.clone()),
                        depth + 1,
                    )
                    .into_schema(),
                );
            }
            let mut obj = node.schema.as_object().cloned().unwrap_or_default();
            obj.insert((*keyword).to_string(), Value::Array(out));
            return ResolvedSchemaNode::at(node.location, Value::Object(obj));
        }
    }

    let mut obj = match node.schema.as_object() {
        Some(o) => o.clone(),
        None => return node,
    };

    for prop_key in &["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(props) = obj.get(*prop_key).and_then(|v| v.as_object()) {
            let mut new_props = Map::new();
            for (k, v) in props {
                new_props.insert(
                    k.clone(),
                    expand_schema_node_at(
                        ctx,
                        node.nested_child(*prop_key, k, v.clone()),
                        depth + 1,
                    )
                    .into_schema(),
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
                expand_schema_node_at(ctx, node.child(*single_key, sub), depth + 1).into_schema(),
            );
        }
    }

    for array_key in &["prefixItems"] {
        if let Some(arr) = obj.get(*array_key).and_then(|v| v.as_array()) {
            let mut out = Vec::new();
            for (index, schema) in arr.iter().enumerate() {
                out.push(
                    expand_schema_node_at(
                        ctx,
                        node.nested_child(*array_key, index.to_string(), schema.clone()),
                        depth + 1,
                    )
                    .into_schema(),
                );
            }
            obj.insert((*array_key).to_string(), Value::Array(out));
        }
    }

    if let Some(ds) = obj.get("dependentSchemas").and_then(|v| v.as_object()) {
        let mut out = Map::new();
        for (k, v) in ds {
            out.insert(
                k.clone(),
                expand_schema_node_at(
                    ctx,
                    node.nested_child("dependentSchemas", k, v.clone()),
                    depth + 1,
                )
                .into_schema(),
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
            expand_schema_node_at(ctx, node.child("additionalProperties", ap), depth + 1)
                .into_schema(),
        );
    }

    ResolvedSchemaNode::at(node.location, Value::Object(obj))
}

pub fn descend_schema_path_expanding_leaf_with_location<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    path: &[String],
) -> Option<ResolvedSchemaLeaf> {
    let root = ResolvedSchemaNode::root(current_filename.to_string(), schema.clone());
    let leaf = descend_schema_path_node(ctx, root, path, 0)?;
    let location = leaf.location.clone();
    let source_schema = leaf.schema.clone();
    let expanded = expand_schema_node_at(ctx, leaf, 0).into_schema();
    Some(ResolvedSchemaLeaf::new(location, source_schema, expanded))
}

fn descend_schema_path_node<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    node: ResolvedSchemaNode,
    path: &[String],
    depth: usize,
) -> Option<ResolvedSchemaNode> {
    if depth > 64 {
        return Some(node);
    }

    let Some((segment, remaining_path)) = path.split_first() else {
        return Some(node);
    };

    let next = descend_one_schema_path_segment(ctx, node, segment, depth)?;
    descend_schema_path_node(ctx, next, remaining_path, depth + 1)
}

fn descend_one_schema_path_segment<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    node: ResolvedSchemaNode,
    segment: &str,
    depth: usize,
) -> Option<ResolvedSchemaNode> {
    let node = resolve_direct_ref(ctx, node, depth)?;

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = node.schema.get(keyword).and_then(Value::as_array) {
            for (index, branch) in branches.iter().enumerate() {
                let branch = node.nested_child(keyword, index.to_string(), branch.clone());
                if let Some(next) = descend_one_schema_path_segment(ctx, branch, segment, depth + 1)
                {
                    return Some(next);
                }
            }
        }
    }

    let (key, is_array_item) = segment
        .strip_suffix("[*]")
        .map_or((segment, false), |key| (key, true));

    let mut next = node
        .schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|properties| {
            properties
                .get(key)
                .map(|schema| node.nested_child("properties", key, schema.clone()))
        })
        .or_else(|| {
            node.schema
                .get("additionalProperties")
                .and_then(|additional_properties| {
                    if additional_properties.is_boolean() {
                        None
                    } else {
                        Some(node.child("additionalProperties", additional_properties.clone()))
                    }
                })
        })?;

    if is_array_item {
        next = resolve_direct_ref(ctx, next, depth + 1)?;
        let array_schema = next;
        next = if let Some(items) = array_schema.schema.get("items") {
            array_schema.child("items", items.clone())
        } else {
            let first_prefix_item = array_schema
                .schema
                .get("prefixItems")
                .and_then(Value::as_array)
                .and_then(|items| items.first())?;
            array_schema.nested_child("prefixItems", "0", first_prefix_item.clone())
        };
    }

    Some(next)
}

fn resolve_direct_ref<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    node: ResolvedSchemaNode,
    depth: usize,
) -> Option<ResolvedSchemaNode> {
    if depth > 64 {
        return Some(node);
    }
    let Some(reference) = node.schema.get("$ref").and_then(Value::as_str) else {
        return Some(node);
    };

    let key = if let Some(pointer) = reference.strip_prefix('#') {
        (node.location.filename().to_string(), format!("#{pointer}"))
    } else {
        let (file, pointer) = reference.split_once('#').unwrap_or((reference, ""));
        let filename = ResolveCtx::<F>::normalize_ref_filename(node.location.filename(), file);
        (filename, format!("#{pointer}"))
    };

    if ctx.stack.contains(&key) {
        return Some(ResolvedSchemaNode::at(
            node.location,
            strip_ref(&node.schema),
        ));
    }
    ctx.stack.insert(key.clone());

    let resolved = ctx
        .resolve_ref(node.location.filename(), reference)
        .and_then(|target| resolve_direct_ref(ctx, target, depth + 1));

    ctx.stack.remove(&key);
    resolved.or_else(|| {
        Some(ResolvedSchemaNode::at(
            node.location,
            strip_ref(&node.schema),
        ))
    })
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
        let actual = descend_schema_path_expanding_leaf_with_location(
            &mut lazy_ctx,
            "root.json",
            &lazy_root,
            &path,
        )
        .expect("lazy descent should contain path")
        .schema()
        .clone();

        assert_eq!(actual, expected);
    }

    #[test]
    fn lazy_path_descent_reports_leaf_source_location_after_cross_file_refs() {
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
                            "$ref": "#/definitions/StringMap"
                        }
                    }
                },
                "StringMap": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "string"
                    }
                }
            }
        }));
        let docs = HashMap::from([("defs.json".to_string(), definitions)]);
        let path = vec![
            "spec".to_string(),
            "containers[*]".to_string(),
            "env".to_string(),
        ];

        let mut ctx = ResolveCtx::new(
            move |filename| docs.get(filename).cloned(),
            "root.json".to_string(),
            root,
        );
        let root_doc = ctx
            .doc("root.json")
            .cloned()
            .expect("root doc should be present");
        let actual = descend_schema_path_expanding_leaf_with_location(
            &mut ctx,
            "root.json",
            &root_doc,
            &path,
        )
        .expect("lazy descent should contain path");

        assert_eq!(actual.location().filename(), "defs.json");
        assert_eq!(
            actual.location().pointer(),
            "/definitions/Container/properties/env"
        );
        assert_eq!(
            actual.source_schema(),
            &json!({ "$ref": "#/definitions/StringMap" })
        );
        assert_eq!(
            actual.schema(),
            &json!({
                "type": "object",
                "additionalProperties": {
                    "type": "string"
                }
            })
        );
    }
}
