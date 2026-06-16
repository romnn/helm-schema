use helm_schema_k8s::{ProviderSchemaFragment, ProviderSchemaSource, ProviderSourceFragment};
use serde_json::Value;

use crate::schema_model::schema_type;

/// Provider-owned schema leaf carried through path resolution.
///
/// The candidate stays tied to the provider source that produced it until
/// generator policy changes the schema shape. Later output stages can use that
/// source identity to emit a stable internal `$ref`; policy stages can still
/// materialize the JSON Schema when they need to compare or merge evidence.
#[derive(Debug, Clone)]
pub(crate) struct ProviderSchemaCandidate {
    key: String,
    schema: Value,
    source_fragment: Option<ProviderSourceFragment>,
}

impl ProviderSchemaCandidate {
    #[cfg(test)]
    pub(crate) fn new(schema: Value) -> Self {
        let key = canonical_schema_key(&schema);
        Self {
            key,
            schema,
            source_fragment: None,
        }
    }

    pub(crate) fn from_provider_fragment(fragment: ProviderSchemaFragment) -> Self {
        let (schema, source_fragment) = fragment.into_source_parts();
        let key = canonical_schema_key(&schema);
        Self {
            key,
            schema,
            source_fragment,
        }
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn schema(&self) -> &Value {
        &self.schema
    }

    pub(crate) fn source(&self) -> Option<&ProviderSchemaSource> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::source)
    }

    pub(crate) fn source_definition_schema(&self) -> Option<&Value> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::definition_schema)
            .filter(|schema| schema_refs_point_inside(schema))
    }

    pub(crate) fn survives_as(&self, schema: &Value) -> bool {
        &self.schema == schema
    }

    pub(crate) fn is_definition_candidate(&self) -> bool {
        is_provider_subtree_schema(&self.schema)
    }
}

fn is_provider_subtree_schema(schema: &Value) -> bool {
    match schema_type(schema) {
        Some("object" | "array") => return true,
        Some(_) => return false,
        None => {}
    }

    let Some(object) = schema.as_object() else {
        return false;
    };
    if object.contains_key("properties")
        || object.contains_key("additionalProperties")
        || object.contains_key("patternProperties")
        || object.contains_key("required")
        || object.contains_key("items")
    {
        return true;
    }

    ["anyOf", "oneOf", "allOf"].into_iter().any(|key| {
        object
            .get(key)
            .and_then(Value::as_array)
            .is_some_and(|variants| variants.iter().any(is_provider_subtree_schema))
    })
}

pub(crate) fn canonical_schema_key(schema: &Value) -> String {
    canonicalize_json_value(schema).to_string()
}

fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json_value).collect()),
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json_value(value)))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        other => other.clone(),
    }
}

pub(crate) fn rewrite_internal_refs_for_root_definition(
    schema: &Value,
    definition_name: &str,
) -> Option<Value> {
    rewrite_internal_refs_in(schema, schema, definition_name)
}

fn schema_refs_point_inside(schema: &Value) -> bool {
    schema_refs_point_inside_value(schema, schema, SchemaTraversalContext::Schema)
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
                    && !local_ref_points_inside(root, reference)
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

fn rewrite_internal_refs_in(root: &Value, value: &Value, definition_name: &str) -> Option<Value> {
    rewrite_internal_refs_in_context(root, value, definition_name, SchemaTraversalContext::Schema)
}

fn rewrite_internal_refs_in_context(
    root: &Value,
    value: &Value,
    definition_name: &str,
    context: SchemaTraversalContext,
) -> Option<Value> {
    match value {
        Value::Array(values) => match context {
            SchemaTraversalContext::Data => Some(value.clone()),
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => values
                .iter()
                .map(|value| {
                    rewrite_internal_refs_in_context(
                        root,
                        value,
                        definition_name,
                        SchemaTraversalContext::Schema,
                    )
                })
                .collect(),
            SchemaTraversalContext::SchemaMapValues => values
                .iter()
                .map(|value| {
                    rewrite_internal_refs_in_context(
                        root,
                        value,
                        definition_name,
                        SchemaTraversalContext::SchemaMapValues,
                    )
                })
                .collect(),
        },
        Value::Object(object) => match context {
            SchemaTraversalContext::Data => Some(value.clone()),
            SchemaTraversalContext::SchemaMapValues => {
                let mut rewritten = serde_json::Map::new();
                for (key, value) in object {
                    rewritten.insert(
                        key.clone(),
                        rewrite_internal_refs_in_context(
                            root,
                            value,
                            definition_name,
                            SchemaTraversalContext::Schema,
                        )?,
                    );
                }
                Some(Value::Object(rewritten))
            }
            SchemaTraversalContext::Schema | SchemaTraversalContext::SchemaArray => {
                let mut rewritten = serde_json::Map::new();
                for (key, value) in object {
                    let value = if key == "$ref" {
                        let reference = value.as_str()?;
                        rewrite_local_ref_for_root_definition(root, reference, definition_name)
                            .map(Value::String)?
                    } else {
                        rewrite_internal_refs_in_context(
                            root,
                            value,
                            definition_name,
                            schema_child_context_for_keyword(key),
                        )?
                    };
                    rewritten.insert(key.clone(), value);
                }
                Some(Value::Object(rewritten))
            }
        },
        _ => Some(value.clone()),
    }
}

fn rewrite_local_ref_for_root_definition(
    root: &Value,
    reference: &str,
    definition_name: &str,
) -> Option<String> {
    let pointer = reference.strip_prefix('#')?;
    if !local_ref_points_inside(root, reference) {
        return None;
    }
    Some(format!(
        "#/$defs/{}{}",
        escape_json_pointer_segment(definition_name),
        pointer
    ))
}

fn local_ref_points_inside(root: &Value, reference: &str) -> bool {
    let Some(pointer) = reference.strip_prefix('#') else {
        return false;
    };
    pointer.is_empty() || root.pointer(pointer).is_some()
}

#[derive(Debug, Clone, Copy)]
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

fn escape_json_pointer_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use helm_schema_k8s::ProviderSchemaSource;
    use serde_json::json;

    use super::*;

    #[test]
    fn candidate_preserves_provider_source_leaf_schema() {
        let source_schema = json!({ "$ref": "#/definitions/StringMap" });
        let fragment = ProviderSchemaFragment::new(json!({
            "type": "object",
            "additionalProperties": { "type": "string" }
        }))
        .with_source_schema(
            ProviderSchemaSource::kubernetes_openapi(
                "default",
                "v1.35.0",
                "source.json",
                "/definitions/Container/properties/env",
            ),
            source_schema.clone(),
        );

        let candidate = ProviderSchemaCandidate::from_provider_fragment(fragment);

        assert_eq!(
            candidate.source().map(ProviderSchemaSource::pointer),
            Some("/definitions/Container/properties/env")
        );
        assert_eq!(
            candidate.source_definition_schema(),
            None,
            "source leaf refs to provider-document siblings are not self-contained at output root"
        );
    }

    #[test]
    fn candidate_exposes_provider_source_leaf_with_only_internal_refs() {
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
        let fragment = ProviderSchemaFragment::new(json!({
            "type": "object",
            "properties": {
                "labels": {
                    "type": "object",
                    "additionalProperties": { "type": "string" }
                }
            }
        }))
        .with_source_schema(
            ProviderSchemaSource::kubernetes_openapi(
                "default",
                "v1.35.0",
                "source.json",
                "/definitions/Metadata",
            ),
            source_schema.clone(),
        );

        let candidate = ProviderSchemaCandidate::from_provider_fragment(fragment);

        assert_eq!(candidate.source_definition_schema(), Some(&source_schema));
    }

    #[test]
    fn rewrites_internal_source_refs_for_root_definition_location() {
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

        let rewritten =
            rewrite_internal_refs_for_root_definition(&source_schema, "provider/source~name")
                .expect("internal refs can be relocated under a root definition");

        assert_eq!(
            rewritten.pointer("/properties/labels/$ref"),
            Some(&Value::String(
                "#/$defs/provider~1source~0name/$defs/StringMap".to_string()
            ))
        );
    }

    #[test]
    fn source_ref_rewrite_ignores_ref_shaped_enum_data() {
        let source_schema = json!({
            "type": "object",
            "enum": [
                { "$ref": "#/not/a/schema/ref" }
            ],
            "properties": {
                "name": { "type": "string" }
            }
        });

        let rewritten = rewrite_internal_refs_for_root_definition(&source_schema, "providerSource")
            .expect("ref-shaped enum data is not schema structure");

        assert_eq!(
            rewritten.pointer("/enum/0/$ref"),
            Some(&Value::String("#/not/a/schema/ref".to_string()))
        );
    }

    #[test]
    fn source_ref_rewrite_treats_property_names_as_schema_map_keys() {
        let source_schema = json!({
            "type": "object",
            "$defs": {
                "StringValue": { "type": "string" }
            },
            "properties": {
                "enum": { "$ref": "#/$defs/StringValue" }
            }
        });

        let rewritten = rewrite_internal_refs_for_root_definition(&source_schema, "providerSource")
            .expect("property schemas are traversed independent of property name");

        assert_eq!(
            rewritten.pointer("/properties/enum/$ref"),
            Some(&Value::String(
                "#/$defs/providerSource/$defs/StringValue".to_string()
            ))
        );
    }
}
