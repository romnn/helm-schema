use helm_schema_core::{ProviderSchemaFragment, ProviderSchemaSource, ProviderSourceFragment};
use json_schema_walk::{
    SchemaTraversalContext, escape_json_pointer_segment, try_map_schema_context,
};
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
    pub(crate) fn from_provider_fragment(fragment: ProviderSchemaFragment) -> Self {
        let (schema, source_fragment) = fragment.into_source_parts();
        let key = json_schema_walk::canonical_json_string(&schema);
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
            .filter(|schema| json_schema_walk::schema_refs_point_inside(schema))
    }

    pub(crate) fn survives_as(&self, schema: &Value) -> bool {
        &self.schema == schema
            || ["anyOf", "oneOf"].into_iter().any(|keyword| {
                schema
                    .get(keyword)
                    .and_then(Value::as_array)
                    .is_some_and(|variants| variants.iter().any(|variant| variant == &self.schema))
            })
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

pub(crate) fn rewrite_internal_refs_for_root_definition(
    schema: &Value,
    definition_name: &str,
) -> Option<Value> {
    let rewritten: Result<Value, ()> = try_map_schema_context(
        schema,
        SchemaTraversalContext::Schema,
        |value, context, _depth| {
            if context != SchemaTraversalContext::Ref {
                return Ok(None);
            }
            let reference = value.as_str().ok_or(())?;
            let reference =
                rewrite_local_ref_for_root_definition(schema, reference, definition_name)
                    .ok_or(())?;
            Ok(Some(Value::String(reference)))
        },
    );
    rewritten.ok()
}

fn rewrite_local_ref_for_root_definition(
    root: &Value,
    reference: &str,
    definition_name: &str,
) -> Option<String> {
    let pointer = reference.strip_prefix('#')?;
    if !json_schema_walk::ref_points_inside(root, reference) {
        return None;
    }
    Some(format!(
        "#/$defs/{}{}",
        escape_json_pointer_segment(definition_name),
        pointer
    ))
}

#[cfg(test)]
#[path = "tests/provider_schema.rs"]
mod tests;
