use serde_json::Value;

use crate::ProviderOrigin;

/// Provider document location that produced a schema fragment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderSchemaSource {
    origin: ProviderOrigin,
    source_id: String,
    version: Option<String>,
    filename: String,
    pointer: String,
}

impl ProviderSchemaSource {
    /// Describes one provider document and JSON pointer that supplied a fragment.
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

    /// Describes a fragment sourced from a versioned Kubernetes `OpenAPI` document.
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

    /// Returns the provider family that owns the source.
    #[must_use]
    pub fn origin(&self) -> ProviderOrigin {
        self.origin
    }

    /// Returns the stable provider-source namespace.
    #[must_use]
    pub fn source_id(&self) -> &str {
        &self.source_id
    }

    /// Returns the Kubernetes release associated with the document, if any.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Returns the source document's filename.
    #[must_use]
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Returns the JSON pointer of the fragment within the source document.
    #[must_use]
    pub fn pointer(&self) -> &str {
        &self.pointer
    }
}

/// Schema fragment returned by a provider for one resource/path lookup.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSchemaFragment {
    schema: Value,
    source_fragment: Option<ProviderSourceFragment>,
    /// Whether the resolved path's final segment is listed in its parent
    /// object's `required` array: the provider rejects a resource that
    /// omits the field (or, for typed fields, supplies null).
    required_in_parent: bool,
}

/// Provider-owned source leaf.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSourceFragment {
    source: ProviderSchemaSource,
    source_schema: Value,
    definition_schema: Value,
}

impl ProviderSourceFragment {
    fn new(source: ProviderSchemaSource, source_schema: Value, definition_schema: Value) -> Self {
        Self {
            source,
            source_schema,
            definition_schema,
        }
    }

    /// Returns the provider document location that owns this fragment.
    #[must_use]
    pub fn source(&self) -> &ProviderSchemaSource {
        &self.source
    }

    /// Returns the normalized schema document loaded from the provider.
    #[must_use]
    pub fn source_schema(&self) -> &Value {
        &self.source_schema
    }

    /// Returns the definition root used to resolve references in the fragment.
    #[must_use]
    pub fn definition_schema(&self) -> &Value {
        &self.definition_schema
    }
}

impl ProviderSchemaFragment {
    /// Wraps and normalizes a materialized provider schema.
    #[must_use]
    pub fn new(mut schema: Value) -> Self {
        // Ingestion is the one boundary where foreign regex dialects enter:
        // provider documents spell Go/RE2 patterns (a leading `(?i)`), and
        // everything downstream — resolution, arms, emitted fixtures — must
        // only ever see the portable ECMA-262 form.
        crate::normalize_schema_pattern_dialects(&mut schema);
        Self {
            schema,
            source_fragment: None,
            required_in_parent: false,
        }
    }

    /// Attaches source ownership using the materialized schema as its definition root.
    #[must_use]
    pub fn with_source(mut self, source: ProviderSchemaSource) -> Self {
        self.source_fragment = Some(ProviderSourceFragment::new(
            source,
            self.schema.clone(),
            self.schema.clone(),
        ));
        self
    }

    /// Attaches explicit source-document and definition-root schemas.
    #[must_use]
    pub fn with_source_definition_schema(
        mut self,
        source: ProviderSchemaSource,
        mut source_schema: Value,
        mut definition_schema: Value,
    ) -> Self {
        crate::normalize_schema_pattern_dialects(&mut source_schema);
        crate::normalize_schema_pattern_dialects(&mut definition_schema);
        self.source_fragment = Some(ProviderSourceFragment::new(
            source,
            source_schema,
            definition_schema,
        ));
        self
    }

    /// Records whether the fragment's field is required by its parent schema.
    #[must_use]
    pub fn with_required_in_parent(mut self, required_in_parent: bool) -> Self {
        self.required_in_parent = required_in_parent;
        self
    }

    /// Reports whether the fragment's field is required by its parent schema.
    #[must_use]
    pub fn required_in_parent(&self) -> bool {
        self.required_in_parent
    }

    /// Returns the materialized schema at the requested resource path.
    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// Returns source ownership when the fragment still maps to a provider leaf.
    #[must_use]
    pub fn source(&self) -> Option<&ProviderSchemaSource> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::source)
    }

    /// Consumes the fragment and returns its materialized schema.
    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }

    /// Consumes the fragment into its schema and optional provider-owned source leaf.
    #[must_use]
    pub fn into_source_parts(self) -> (Value, Option<ProviderSourceFragment>) {
        (self.schema, self.source_fragment)
    }

    /// Transform the materialized schema while preserving provider ownership.
    #[must_use]
    pub fn try_map_schema(self, map_schema: impl FnOnce(&Value) -> Option<Value>) -> Option<Self> {
        let Self {
            schema,
            source_fragment,
            required_in_parent,
        } = self;
        let mapped = map_schema(&schema)?;
        let source_fragment = (mapped == schema).then_some(source_fragment).flatten();
        Some(Self {
            schema: mapped,
            source_fragment,
            required_in_parent,
        })
    }
}

impl From<Value> for ProviderSchemaFragment {
    fn from(schema: Value) -> Self {
        Self::new(schema)
    }
}
