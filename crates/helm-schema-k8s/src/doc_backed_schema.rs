use serde_json::Value;

use crate::lookup::source_bundle::{
    SourceBundleNode, bundle_source_schema, schema_refs_point_inside,
};
use crate::lookup::{ProviderSchemaFragment, ProviderSchemaSource};
use crate::metadata_enrichment::{enrich_root_metadata_schema, enriched_metadata_schema};
use crate::schema_doc::SchemaDoc;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalSchemaLeaf {
    schema: Value,
    source_schema: Option<Value>,
    pointer: Option<String>,
}

impl LocalSchemaLeaf {
    fn new(schema: Value, pointer: Option<String>) -> Self {
        Self {
            schema,
            source_schema: None,
            pointer,
        }
    }

    fn from_source_leaf(source_leaf: Self, expanded_schema: Value) -> Self {
        let source_schema = source_leaf.pointer.is_some().then_some(source_leaf.schema);
        Self {
            schema: expanded_schema,
            source_schema,
            pointer: source_leaf.pointer,
        }
    }

    #[must_use]
    pub(crate) fn schema(&self) -> &Value {
        &self.schema
    }

    #[must_use]
    pub(crate) fn source_schema(&self) -> Option<&Value> {
        self.source_schema.as_ref()
    }

    #[must_use]
    pub(crate) fn pointer(&self) -> Option<&str> {
        self.pointer.as_deref()
    }

    #[must_use]
    pub(crate) fn into_schema(self) -> Value {
        self.schema
    }
}

pub(crate) fn fragment_for_source_leaf(
    root: &SchemaDoc,
    source: Option<ProviderSchemaSource>,
    leaf: LocalSchemaLeaf,
) -> ProviderSchemaFragment {
    let source_schema = leaf.source_schema().cloned();
    let mut fragment = ProviderSchemaFragment::new(leaf.into_schema());
    match (source, source_schema) {
        (Some(source), Some(source_schema)) => {
            let definition_schema = bundled_local_definition_schema(
                root.root(),
                source.filename(),
                source.pointer(),
                &source_schema,
            );
            fragment =
                fragment.with_source_definition_schema(source, source_schema, definition_schema);
        }
        (Some(source), None) => {
            fragment = fragment.with_source(source);
        }
        (None, _) => {}
    }
    fragment
}

pub(crate) fn bundled_local_definition_schema(
    root: &Value,
    document: &str,
    pointer: &str,
    source_schema: &Value,
) -> Value {
    if schema_refs_point_inside(source_schema, source_schema) {
        return source_schema.clone();
    }

    bundle_source_schema(
        SourceBundleNode::new(document, pointer, source_schema.clone()),
        |current_location, reference| {
            let pointer = reference.strip_prefix('#')?;
            if source_schema.pointer(pointer).is_some() {
                return None;
            }
            root.pointer(pointer)
                .cloned()
                .map(|schema| SourceBundleNode::new(current_location.document(), pointer, schema))
        },
    )
}

#[must_use]
pub(crate) fn debug_materialize_local_schema(root: &Value) -> Value {
    let mut stack = std::collections::HashSet::new();
    enrich_root_metadata_schema(expand_local_refs(root, root, 0, &mut stack))
}

#[cfg(test)]
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path(schema: &Value, path: &[String]) -> Option<Value> {
    let mut current = schema;
    for segment in path {
        current = descend_one(current, segment)?;
    }
    Some(current.clone())
}

#[cfg(test)]
fn descend_one<'a>(schema: &'a Value, segment: &str) -> Option<&'a Value> {
    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(keyword).and_then(Value::as_array) {
            for branch in branches {
                if let Some(value) = descend_one(branch, segment) {
                    return Some(value);
                }
            }
        }
    }

    let (key, is_array_item) = segment
        .strip_suffix("[*]")
        .map_or((segment, false), |key| (key, true));

    let mut next = schema
        .get("properties")
        .and_then(|properties| properties.as_object())
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
        })?;

    if is_array_item {
        next = next.get("items").or_else(|| {
            next.get("prefixItems")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
        })?;
    }

    Some(next)
}

#[cfg(test)]
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path_expanding_leaf(root: &Value, path: &[String]) -> Option<Value> {
    descend_schema_path_expanding_leaf_with_source(root, path).map(LocalSchemaLeaf::into_schema)
}

#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path_expanding_leaf_with_source(
    root: &Value,
    path: &[String],
) -> Option<LocalSchemaLeaf> {
    let mut stack = std::collections::HashSet::new();
    let leaf = descend_schema_path_node(root, root, Some(String::new()), path, 0, &mut stack)?;
    let mut expand_stack = std::collections::HashSet::new();
    let expanded = expand_local_refs(root, leaf.schema(), 0, &mut expand_stack);
    Some(LocalSchemaLeaf::from_source_leaf(leaf, expanded))
}

#[cfg(test)]
#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path_expanding_leaf_with_root_metadata(
    root: &Value,
    path: &[String],
) -> Option<Value> {
    descend_schema_path_expanding_leaf_with_root_metadata_source(root, path)
        .map(LocalSchemaLeaf::into_schema)
}

#[tracing::instrument(skip_all, fields(path_len = path.len()))]
pub(crate) fn descend_schema_path_expanding_leaf_with_root_metadata_source(
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
    let expanded = expand_local_refs(root, leaf.schema(), 0, &mut expand_stack);
    Some(LocalSchemaLeaf::from_source_leaf(leaf, expanded))
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
        ..
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
            ..
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
    let mut pointer = base?.to_string();
    for segment in segments {
        pointer.push('/');
        pointer.push_str(&escape_json_pointer_segment(segment));
    }
    Some(pointer)
}

fn escape_json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

pub(crate) fn expand_local_refs(
    root: &Value,
    schema: &Value,
    depth: usize,
    stack: &mut std::collections::HashSet<String>,
) -> Value {
    if depth > 64 {
        return schema.clone();
    }

    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        if stack.contains(reference) {
            return strip_ref(schema);
        }
        stack.insert(reference.to_string());

        let out = if let Some(pointer) = reference.strip_prefix('#') {
            root.pointer(pointer).map_or_else(
                || strip_ref(schema),
                |target| expand_local_refs(root, target, depth + 1, stack),
            )
        } else {
            strip_ref(schema)
        };

        stack.remove(reference);
        return out;
    }

    let Some(object) = schema.as_object() else {
        return schema.clone();
    };

    let mut out = object.clone();

    for keyword in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = out.get(keyword).and_then(Value::as_array) {
            let expanded = branches
                .iter()
                .map(|branch| expand_local_refs(root, branch, depth + 1, stack))
                .collect();
            out.insert(keyword.to_string(), Value::Array(expanded));
        }
    }

    for map_key in ["properties", "patternProperties", "definitions", "$defs"] {
        if let Some(map) = out.get(map_key).and_then(Value::as_object) {
            let mut expanded = serde_json::Map::new();
            for (key, value) in map {
                expanded.insert(
                    key.clone(),
                    expand_local_refs(root, value, depth + 1, stack),
                );
            }
            out.insert(map_key.to_string(), Value::Object(expanded));
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
        if let Some(value) = out.get(single_key).cloned()
            && !value.is_boolean()
        {
            out.insert(
                single_key.to_string(),
                expand_local_refs(root, &value, depth + 1, stack),
            );
        }
    }

    Value::Object(out)
}

fn strip_ref(schema: &Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = object.clone();
    out.remove("$ref");
    Value::Object(out)
}
