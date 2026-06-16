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

    #[cfg(test)]
    pub(crate) fn source_schema(&self) -> Option<&Value> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::schema)
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

fn canonical_schema_key(schema: &Value) -> String {
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
        assert_eq!(candidate.source_schema(), Some(&source_schema));
    }
}
