use serde_json::Value;

use super::provider_origin::ProviderOrigin;

/// Provider document location that produced a schema fragment.
///
/// This is structured metadata so later bundled-emission code can decide how
/// to retain provider `$ref` shape without parsing a formatted string key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSchemaSource {
    origin: ProviderOrigin,
    source_id: String,
    version: Option<String>,
    filename: String,
    pointer: String,
}

impl ProviderSchemaSource {
    #[must_use]
    pub fn new(
        origin: ProviderOrigin,
        source_id: impl Into<String>,
        version: Option<String>,
        filename: impl Into<String>,
        pointer: impl Into<String>,
    ) -> Self {
        Self {
            origin,
            source_id: source_id.into(),
            version,
            filename: filename.into(),
            pointer: pointer.into(),
        }
    }

    #[must_use]
    pub fn kubernetes_openapi(
        source_id: impl Into<String>,
        version: impl Into<String>,
        filename: impl Into<String>,
        pointer: impl Into<String>,
    ) -> Self {
        Self::new(
            ProviderOrigin::KubernetesOpenApi,
            source_id,
            Some(version.into()),
            filename,
            pointer,
        )
    }

    #[must_use]
    pub fn origin(&self) -> ProviderOrigin {
        self.origin
    }

    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
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
    source: Option<ProviderSchemaSource>,
}

impl ProviderSchemaFragment {
    #[must_use]
    pub fn new(schema: Value) -> Self {
        Self {
            schema,
            source: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: ProviderSchemaSource) -> Self {
        self.source = Some(source);
        self
    }

    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    #[must_use]
    pub fn source(&self) -> Option<&ProviderSchemaSource> {
        self.source.as_ref()
    }

    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }

    #[must_use]
    pub fn into_parts(self) -> (Value, Option<ProviderSchemaSource>) {
        (self.schema, self.source)
    }

    /// Transform the materialized schema while preserving provider ownership.
    ///
    /// Returning `None` lets callers drop fragments that do not survive a
    /// domain-specific projection. Source identity survives only when the
    /// projection is structurally exact; changed schemas are no longer the
    /// same provider document fragment.
    #[must_use]
    pub fn try_map_schema(self, map_schema: impl FnOnce(&Value) -> Option<Value>) -> Option<Self> {
        let Self { schema, source } = self;
        let mapped = map_schema(&schema)?;
        let source = (mapped == schema).then_some(source).flatten();
        Some(Self {
            schema: mapped,
            source,
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
    fn unchanged_projection_preserves_source() {
        let fragment = ProviderSchemaFragment::new(json!({"type": "string"})).with_source(
            ProviderSchemaSource::kubernetes_openapi(
                "default",
                "v1.35.0",
                "source.json",
                "/definitions/name",
            ),
        );

        let projected = fragment
            .try_map_schema(|schema| Some(schema.clone()))
            .expect("unchanged projection should survive");

        let source = projected.source().expect("source should be preserved");
        assert_eq!(source.origin(), ProviderOrigin::KubernetesOpenApi);
        assert_eq!(source.source_id(), "default");
        assert_eq!(source.version(), Some("v1.35.0"));
        assert_eq!(source.filename(), "source.json");
        assert_eq!(source.pointer(), "/definitions/name");
    }

    #[test]
    fn structurally_equal_rebuilt_projection_preserves_source() {
        let fragment = ProviderSchemaFragment::new(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        }))
        .with_source(ProviderSchemaSource::kubernetes_openapi(
            "default",
            "v1.35.0",
            "source.json",
            "/definitions/metadata",
        ));

        let projected = fragment
            .try_map_schema(|_| {
                Some(json!({
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "type": "object"
                }))
            })
            .expect("structurally equal projection should survive");

        let source = projected.source().expect("source should be preserved");
        assert_eq!(source.pointer(), "/definitions/metadata");
    }

    #[test]
    fn changed_projection_drops_source() {
        let fragment = ProviderSchemaFragment::new(json!({"type": "string"})).with_source(
            ProviderSchemaSource::kubernetes_openapi(
                "default",
                "v1.35.0",
                "source.json",
                "/definitions/name",
            ),
        );

        let projected = fragment
            .try_map_schema(|_| Some(json!({"type": ["string", "null"]})))
            .expect("changed projection should survive");

        assert_eq!(projected.source(), None);
    }
}
