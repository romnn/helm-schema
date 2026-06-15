use serde_json::Value;

/// Schema fragment returned by a provider for one resource/path lookup.
///
/// This is the provider-owned boundary object. It currently stores the
/// materialized JSON Schema fragment, but callers should pass this type through
/// provider/chain layers instead of collapsing it to a plain [`Value`]. That
/// keeps room for source-document and reference-shape metadata at the same
/// boundary.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSchemaFragment {
    schema: Value,
}

impl ProviderSchemaFragment {
    #[must_use]
    pub fn new(schema: Value) -> Self {
        Self { schema }
    }

    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }

    /// Transform the materialized schema while preserving provider ownership.
    ///
    /// Returning `None` lets callers drop fragments that do not survive a
    /// domain-specific projection.
    pub fn try_map_schema(self, map_schema: impl FnOnce(Value) -> Option<Value>) -> Option<Self> {
        map_schema(self.schema).map(Self::new)
    }
}

impl From<Value> for ProviderSchemaFragment {
    fn from(schema: Value) -> Self {
        Self::new(schema)
    }
}
