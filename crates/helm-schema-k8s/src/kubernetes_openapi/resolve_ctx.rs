use std::collections::{HashMap, HashSet};

use json_schema_walk::{SchemaTraversalContext, schema_child_context_for_keyword};
use serde_json::{Map, Value};

use crate::schema_doc::{SchemaDoc, strip_ref};

/// `$ref` resolution context. Holds previously-loaded documents and a
/// stack of (filename, json-pointer) pairs to break cycles.
///
/// The context is short-lived: one per top-level provider fragment lookup.
/// The provider supplies a loader that knows how to fetch a
/// neighboring schema file by relative filename (typically by mapping
/// the filename through the same provider fetch/cache path the
/// resource doc came from).
pub(crate) struct ResolveCtx<F: FnMut(&str) -> Option<SchemaDoc>> {
    loader: F,
    docs: HashMap<String, SchemaDoc>,
    stack: HashSet<(String, String)>,
}

/// Source location of a schema node inside the provider document graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SchemaNodeLocation {
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
pub(crate) struct ResolvedSchemaNode {
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
pub(crate) struct ResolvedSchemaLeaf {
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

    pub(crate) fn resolve_ref(
        &mut self,
        current_filename: &str,
        reference: &str,
    ) -> Option<ResolvedSchemaNode> {
        let (filename, pointer) = split_reference(current_filename, reference);
        let doc = self.load_doc(&filename)?;
        let schema = if pointer.is_empty() {
            doc.clone()
        } else {
            doc.pointer(&pointer)?.clone()
        };
        Some(ResolvedSchemaNode::at(
            SchemaNodeLocation { filename, pointer },
            schema,
        ))
    }
}

/// Split a `$ref` into the `(filename, json-pointer)` pair it targets,
/// resolving same-document references against `current_filename`. The pair is
/// also the cycle-stack key for that reference.
fn split_reference(current_filename: &str, reference: &str) -> (String, String) {
    if let Some(pointer) = reference.strip_prefix('#') {
        return (current_filename.to_string(), pointer.to_string());
    }
    let (file, pointer) = reference.split_once('#').unwrap_or((reference, ""));
    (
        normalize_ref_filename(current_filename, file),
        pointer.to_string(),
    )
}

fn normalize_ref_filename(current_filename: &str, file: &str) -> String {
    if file.is_empty() {
        return current_filename.to_string();
    }
    let trimmed = file.trim().trim_start_matches("./");
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

fn append_json_pointer_segment(pointer: &str, segment: &str) -> String {
    let escaped = json_schema_walk::escape_json_pointer_segment(segment);
    if pointer.is_empty() {
        format!("/{escaped}")
    } else {
        format!("{pointer}/{escaped}")
    }
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
        let key = split_reference(node.location.filename(), reference);

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

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = node.schema.get(keyword).and_then(Value::as_array) {
            let expanded = expand_schema_array_at(ctx, &node, keyword, branches, depth + 1);
            let mut obj = node.schema.as_object().cloned().unwrap_or_default();
            obj.insert(keyword.to_string(), expanded);
            return ResolvedSchemaNode::at(node.location, Value::Object(obj));
        }
    }

    let mut obj = match node.schema.as_object() {
        Some(o) => o.clone(),
        None => return node,
    };

    for (key, value) in node.schema.as_object().into_iter().flat_map(Map::iter) {
        let expanded = match schema_child_context_for_keyword(key) {
            SchemaTraversalContext::Schema if value.is_boolean() => continue,
            SchemaTraversalContext::Schema => match value {
                Value::Array(values) => expand_schema_array_at(ctx, &node, key, values, depth + 1),
                _ => expand_schema_node_at(ctx, node.child(key, value.clone()), depth + 1)
                    .into_schema(),
            },
            SchemaTraversalContext::SchemaArray => {
                let Some(values) = value.as_array() else {
                    continue;
                };
                expand_schema_array_at(ctx, &node, key, values, depth + 1)
            }
            SchemaTraversalContext::SchemaMapValues => {
                let Some(values) = value.as_object() else {
                    continue;
                };
                Value::Object(
                    values
                        .iter()
                        .map(|(entry_key, schema)| {
                            (
                                entry_key.clone(),
                                expand_schema_node_at(
                                    ctx,
                                    node.nested_child(key, entry_key, schema.clone()),
                                    depth + 1,
                                )
                                .into_schema(),
                            )
                        })
                        .collect(),
                )
            }
            SchemaTraversalContext::Data | SchemaTraversalContext::Ref => continue,
        };
        obj.insert(key.clone(), expanded);
    }

    ResolvedSchemaNode::at(node.location, Value::Object(obj))
}

fn expand_schema_array_at<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    node: &ResolvedSchemaNode,
    key: &str,
    values: &[Value],
    depth: usize,
) -> Value {
    Value::Array(
        values
            .iter()
            .enumerate()
            .map(|(index, schema)| {
                expand_schema_node_at(
                    ctx,
                    node.nested_child(key, index.to_string(), schema.clone()),
                    depth,
                )
                .into_schema()
            })
            .collect(),
    )
}

pub(crate) fn descend_schema_path_expanding_leaf_with_location<
    F: FnMut(&str) -> Option<SchemaDoc>,
>(
    ctx: &mut ResolveCtx<F>,
    current_filename: &str,
    schema: &Value,
    path: &[String],
) -> Option<ResolvedSchemaLeaf> {
    let root = ResolvedSchemaNode::root(current_filename.to_string(), schema.clone());
    let leaf = descend_schema_path_node(ctx, root, path)?;
    let location = leaf.location.clone();
    let source_schema = leaf.schema.clone();
    let expanded = expand_schema_node_at(ctx, leaf, 0).into_schema();
    Some(ResolvedSchemaLeaf::new(location, source_schema, expanded))
}

fn descend_schema_path_node<F: FnMut(&str) -> Option<SchemaDoc>>(
    ctx: &mut ResolveCtx<F>,
    mut node: ResolvedSchemaNode,
    path: &[String],
) -> Option<ResolvedSchemaNode> {
    for (depth, segment) in path.iter().enumerate() {
        if depth > 64 {
            return Some(node);
        }
        node = descend_one_schema_path_segment(ctx, node, segment, depth)?;
    }
    Some(node)
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

    let key = split_reference(node.location.filename(), reference);
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
#[path = "tests/resolve_ctx.rs"]
mod tests;
