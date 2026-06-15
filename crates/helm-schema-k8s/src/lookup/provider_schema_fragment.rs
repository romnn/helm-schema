use serde_json::Value;

/// Schema fragment returned by a provider for one resource/path lookup.
///
/// This is the provider-owned boundary object. It currently stores the
/// materialized JSON Schema fragment plus optional provider-source identity.
/// Callers should pass this type through provider/chain layers instead of
/// collapsing it to a plain [`Value`]. That keeps source-document and
/// reference-shape metadata attached to the provider boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSchemaFragment {
    schema: Value,
    source_key: Option<String>,
}

impl ProviderSchemaFragment {
    #[must_use]
    pub fn new(schema: Value) -> Self {
        Self {
            schema,
            source_key: None,
        }
    }

    #[must_use]
    pub fn with_source_key(mut self, source_key: impl Into<String>) -> Self {
        self.source_key = Some(source_key.into());
        self
    }

    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    #[must_use]
    pub fn source_key(&self) -> Option<&str> {
        self.source_key.as_deref()
    }

    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }

    /// Transform the materialized schema while preserving provider ownership.
    ///
    /// Returning `None` lets callers drop fragments that do not survive a
    /// domain-specific projection. Callers must set `preserve_source_key` only
    /// when their projection is structurally known to leave the provider
    /// schema unchanged.
    pub fn try_map_schema(
        self,
        map_schema: impl FnOnce(Value) -> Option<Value>,
        preserve_source_key: bool,
    ) -> Option<Self> {
        let Self { schema, source_key } = self;
        let mapped = map_schema(schema)?;
        let source_key = preserve_source_key.then_some(source_key).flatten();
        Some(Self {
            schema: mapped,
            source_key,
        })
    }
}

impl From<Value> for ProviderSchemaFragment {
    fn from(schema: Value) -> Self {
        Self::new(schema)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn unchanged_projection_preserves_source_key() {
        let fragment = ProviderSchemaFragment::new(json!({"type": "string"}))
            .with_source_key("source.json#/definitions/name");

        let projected = fragment
            .try_map_schema(Some, true)
            .expect("unchanged projection should survive");

        assert_eq!(
            projected.source_key(),
            Some("source.json#/definitions/name")
        );
    }

    #[test]
    fn changed_projection_drops_source_key() {
        let fragment = ProviderSchemaFragment::new(json!({"type": "string"}))
            .with_source_key("source.json#/definitions/name");

        let projected = fragment
            .try_map_schema(|_| Some(json!({"type": ["string", "null"]})), false)
            .expect("changed projection should survive");

        assert_eq!(projected.source_key(), None);
    }
}
