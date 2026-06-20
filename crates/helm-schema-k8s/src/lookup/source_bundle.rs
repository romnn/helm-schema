use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct SourceBundleLocation {
    document: String,
    pointer: String,
}

impl SourceBundleLocation {
    pub(crate) fn new(document: impl Into<String>, pointer: impl Into<String>) -> Self {
        Self {
            document: document.into(),
            pointer: pointer.into(),
        }
    }

    pub(crate) fn document(&self) -> &str {
        &self.document
    }

    pub(crate) fn pointer(&self) -> &str {
        &self.pointer
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceBundleNode {
    location: SourceBundleLocation,
    schema: Value,
}

impl SourceBundleNode {
    pub(crate) fn new(
        document: impl Into<String>,
        pointer: impl Into<String>,
        schema: Value,
    ) -> Self {
        Self {
            location: SourceBundleLocation::new(document, pointer),
            schema,
        }
    }

    fn location(&self) -> &SourceBundleLocation {
        &self.location
    }

    fn schema(&self) -> &Value {
        &self.schema
    }
}

pub(crate) fn schema_refs_point_inside(root: &Value, schema: &Value) -> bool {
    schema_refs_point_inside_value(root, schema, SchemaTraversalContext::Schema)
}

pub(crate) fn bundle_source_schema(
    root: SourceBundleNode,
    resolve_external_ref: impl FnMut(&SourceBundleLocation, &str) -> Option<SourceBundleNode>,
) -> Value {
    let mut bundler = SourceSchemaBundler::new(root.schema().clone(), resolve_external_ref);
    bundler.bundle_root(root)
}

struct SourceSchemaBundler<R> {
    root_schema: Value,
    resolve_external_ref: R,
    definition_names_by_location: BTreeMap<SourceBundleLocation, String>,
    definitions_by_name: BTreeMap<String, Value>,
    used_definition_names: BTreeSet<String>,
    root_location: Option<SourceBundleLocation>,
}

impl<R> SourceSchemaBundler<R>
where
    R: FnMut(&SourceBundleLocation, &str) -> Option<SourceBundleNode>,
{
    fn new(root_schema: Value, resolve_external_ref: R) -> Self {
        Self {
            root_schema,
            resolve_external_ref,
            definition_names_by_location: BTreeMap::new(),
            definitions_by_name: BTreeMap::new(),
            used_definition_names: BTreeSet::new(),
            root_location: None,
        }
    }

    fn bundle_root(&mut self, root: SourceBundleNode) -> Value {
        self.seed_existing_definition_names(root.schema());
        self.root_location = Some(root.location().clone());
        let mut schema = self.bundle_root_node(root, 0);
        if !self.definitions_by_name.is_empty() {
            let definitions = schema.as_object_mut().and_then(|object| {
                object
                    .entry("$defs".to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
            });
            if let Some(definitions) = definitions {
                for (name, definition) in std::mem::take(&mut self.definitions_by_name) {
                    definitions.insert(name, definition);
                }
            }
        }
        schema
    }

    fn seed_existing_definition_names(&mut self, schema: &Value) {
        for key in ["$defs", "definitions"] {
            let Some(definitions) = schema.get(key).and_then(Value::as_object) else {
                continue;
            };
            self.used_definition_names
                .extend(definitions.keys().cloned());
        }
    }

    fn bundle_root_node(&mut self, node: SourceBundleNode, depth: usize) -> Value {
        if depth > 64 {
            return node.schema;
        }

        if let Some(reference) = node.schema().get("$ref").and_then(Value::as_str) {
            if ref_points_inside(&self.root_schema, reference) {
                return self.bundle_schema_value(
                    node.schema(),
                    SchemaTraversalContext::Schema,
                    node.location(),
                    depth,
                );
            }
            if let Some(target) = (self.resolve_external_ref)(node.location(), reference) {
                if self.root_location.as_ref() == Some(target.location()) {
                    return Value::Object(
                        [("$ref".to_string(), Value::String("#".to_string()))]
                            .into_iter()
                            .collect(),
                    );
                }
                return self.bundle_root_node(target, depth + 1);
            }
            return strip_ref(node.schema());
        }

        self.bundle_schema_value(
            node.schema(),
            SchemaTraversalContext::Schema,
            node.location(),
            depth,
        )
    }

    fn bundle_schema_value(
        &mut self,
        value: &Value,
        context: SchemaTraversalContext,
        current_location: &SourceBundleLocation,
        depth: usize,
    ) -> Value {
        match value {
            Value::Array(values) => match context {
                SchemaTraversalContext::Data => Value::Array(values.clone()),
                SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                    Value::Array(
                        values
                            .iter()
                            .map(|value| {
                                self.bundle_schema_value(
                                    value,
                                    SchemaTraversalContext::Schema,
                                    current_location,
                                    depth + 1,
                                )
                            })
                            .collect(),
                    )
                }
                SchemaTraversalContext::SchemaMapValues => Value::Array(
                    values
                        .iter()
                        .map(|value| {
                            self.bundle_schema_value(
                                value,
                                SchemaTraversalContext::SchemaMapValues,
                                current_location,
                                depth + 1,
                            )
                        })
                        .collect(),
                ),
            },
            Value::Object(object) => match context {
                SchemaTraversalContext::Data => Value::Object(object.clone()),
                SchemaTraversalContext::SchemaMapValues => Value::Object(
                    object
                        .iter()
                        .map(|(key, value)| {
                            (
                                key.clone(),
                                self.bundle_schema_value(
                                    value,
                                    SchemaTraversalContext::Schema,
                                    current_location,
                                    depth + 1,
                                ),
                            )
                        })
                        .collect(),
                ),
                SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                        return self.bundle_schema_ref(value, reference, current_location, depth);
                    }

                    Value::Object(
                        object
                            .iter()
                            .map(|(key, value)| {
                                (
                                    key.clone(),
                                    self.bundle_schema_value(
                                        value,
                                        schema_child_context_for_keyword(key),
                                        current_location,
                                        depth + 1,
                                    ),
                                )
                            })
                            .collect(),
                    )
                }
            },
            _ => value.clone(),
        }
    }

    fn bundle_schema_ref(
        &mut self,
        schema: &Value,
        reference: &str,
        current_location: &SourceBundleLocation,
        depth: usize,
    ) -> Value {
        if ref_points_inside(&self.root_schema, reference) {
            return schema.clone();
        }

        let Some(target) = (self.resolve_external_ref)(current_location, reference) else {
            return strip_ref(schema);
        };

        if self.root_location.as_ref() == Some(target.location()) {
            return Value::Object(
                [("$ref".to_string(), Value::String("#".to_string()))]
                    .into_iter()
                    .collect(),
            );
        }

        let definition_name = self.definition_name_for_target(target, depth + 1);
        Value::Object(
            [(
                "$ref".to_string(),
                Value::String(format!("#/$defs/{definition_name}")),
            )]
            .into_iter()
            .collect(),
        )
    }

    fn definition_name_for_target(&mut self, target: SourceBundleNode, depth: usize) -> String {
        if let Some(name) = self.definition_names_by_location.get(target.location()) {
            return name.clone();
        }

        let name = self.next_definition_name(target.location());
        self.definition_names_by_location
            .insert(target.location().clone(), name.clone());

        let definition_schema = self.bundle_root_node(target, depth);
        self.definitions_by_name
            .insert(name.clone(), definition_schema);
        name
    }

    fn next_definition_name(&mut self, location: &SourceBundleLocation) -> String {
        let base_name = definition_base_name(location.pointer());
        if self.used_definition_names.insert(base_name.clone()) {
            return base_name;
        }

        let mut suffix = 2;
        loop {
            let candidate = format!("{base_name}_{suffix}");
            suffix += 1;
            if self.used_definition_names.insert(candidate.clone()) {
                return candidate;
            }
        }
    }
}

fn schema_refs_point_inside_value(
    root: &Value,
    value: &Value,
    context: SchemaTraversalContext,
) -> bool {
    match value {
        Value::Array(values) => match context {
            SchemaTraversalContext::Data => true,
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                values.iter().all(|value| {
                    schema_refs_point_inside_value(root, value, SchemaTraversalContext::Schema)
                })
            }
            SchemaTraversalContext::SchemaMapValues => values.iter().all(|value| {
                schema_refs_point_inside_value(root, value, SchemaTraversalContext::SchemaMapValues)
            }),
        },
        Value::Object(object) => match context {
            SchemaTraversalContext::Data => true,
            SchemaTraversalContext::SchemaMapValues => object.values().all(|value| {
                schema_refs_point_inside_value(root, value, SchemaTraversalContext::Schema)
            }),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                if let Some(reference) = object.get("$ref").and_then(Value::as_str)
                    && !ref_points_inside(root, reference)
                {
                    return false;
                }
                object.iter().all(|(key, value)| {
                    schema_refs_point_inside_value(
                        root,
                        value,
                        schema_child_context_for_keyword(key),
                    )
                })
            }
        },
        _ => true,
    }
}

fn ref_points_inside(root: &Value, reference: &str) -> bool {
    let Some(pointer) = reference.strip_prefix('#') else {
        return false;
    };
    pointer.is_empty() || root.pointer(pointer).is_some()
}

fn strip_ref(schema: &Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = object.clone();
    out.remove("$ref");
    Value::Object(out)
}

#[derive(Clone, Copy)]
enum SchemaTraversalContext {
    Schema,
    SchemaArray,
    SchemaMapValues,
    Data,
}

fn schema_child_context_for_keyword(key: &str) -> SchemaTraversalContext {
    match key {
        "properties" | "patternProperties" | "$defs" | "definitions" | "dependentSchemas" => {
            SchemaTraversalContext::SchemaMapValues
        }
        "allOf" | "anyOf" | "oneOf" | "prefixItems" => SchemaTraversalContext::SchemaArray,
        "additionalItems"
        | "additionalProperties"
        | "contains"
        | "else"
        | "if"
        | "items"
        | "not"
        | "propertyNames"
        | "then"
        | "unevaluatedItems"
        | "unevaluatedProperties" => SchemaTraversalContext::Schema,
        _ => SchemaTraversalContext::Data,
    }
}

fn definition_base_name(pointer: &str) -> String {
    let base = pointer
        .rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("definition");
    let sanitized: String = base
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "definition".to_string()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use test_util::prelude::sim_assert_eq;

    use super::*;

    #[test]
    fn bundles_provider_document_refs_into_local_definitions() {
        let root_leaf = json!({
            "type": "object",
            "additionalProperties": { "$ref": "#/definitions/StringMap" }
        });
        let bundled = bundle_source_schema(
            SourceBundleNode::new(
                "source.json",
                "/definitions/Container/properties/env",
                root_leaf,
            ),
            |current_location, reference| {
                sim_assert_eq!(have: current_location.document(), want: "source.json");
                (reference == "#/definitions/StringMap").then(|| {
                    SourceBundleNode::new(
                        "source.json",
                        "/definitions/StringMap",
                        json!({
                            "type": "object",
                            "additionalProperties": { "type": "string" }
                        }),
                    )
                })
            },
        );

        sim_assert_eq!(
            have: bundled,
            want: json!({
                "type": "object",
                "additionalProperties": { "$ref": "#/$defs/StringMap" },
                "$defs": {
                    "StringMap": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }
                }
            })
        );
    }

    #[test]
    fn keeps_leaf_local_definitions_intact() {
        let source_schema = json!({
            "type": "object",
            "properties": {
                "labels": { "$ref": "#/$defs/StringMap" }
            },
            "$defs": {
                "StringMap": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        });

        assert!(schema_refs_point_inside(&source_schema, &source_schema));
    }

    #[test]
    fn bundles_cross_file_refs_into_local_definitions() {
        let root_leaf = json!({
            "type": "object",
            "properties": {
                "selector": { "$ref": "common.json#/definitions/Selector" }
            }
        });
        let bundled = bundle_source_schema(
            SourceBundleNode::new("pod.json", "/definitions/Spec", root_leaf),
            |_, reference| {
                (reference == "common.json#/definitions/Selector")
                    .then(|| {
                        SourceBundleNode::new(
                            "common.json",
                            "/definitions/Selector",
                            json!({
                                "type": "object",
                                "properties": {
                                    "matchLabels": {
                                        "$ref": "#/definitions/StringMap"
                                    }
                                }
                            }),
                        )
                    })
                    .or_else(|| {
                        (reference == "#/definitions/StringMap").then(|| {
                            SourceBundleNode::new(
                                "common.json",
                                "/definitions/StringMap",
                                json!({
                                    "type": "object",
                                    "additionalProperties": { "type": "string" }
                                }),
                            )
                        })
                    })
            },
        );

        sim_assert_eq!(
            have: bundled,
            want: json!({
                "type": "object",
                "properties": {
                    "selector": { "$ref": "#/$defs/Selector" }
                },
                "$defs": {
                    "Selector": {
                        "type": "object",
                        "properties": {
                            "matchLabels": { "$ref": "#/$defs/StringMap" }
                        }
                    },
                    "StringMap": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }
                }
            })
        );
    }
}
