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
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSchemaFragment {
    schema: Value,
    source_fragment: Option<ProviderSourceFragment>,
}

/// Provider-owned source leaf.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderSourceFragment {
    source: ProviderSchemaSource,
    source_schema: Value,
    definition_schema: Value,
}

impl ProviderSourceFragment {
    #[must_use]
    pub fn new(
        source: ProviderSchemaSource,
        source_schema: Value,
        definition_schema: Value,
    ) -> Self {
        Self {
            source,
            source_schema,
            definition_schema,
        }
    }

    #[must_use]
    pub fn source(&self) -> &ProviderSchemaSource {
        &self.source
    }

    #[must_use]
    pub fn source_schema(&self) -> &Value {
        &self.source_schema
    }

    #[must_use]
    pub fn definition_schema(&self) -> &Value {
        &self.definition_schema
    }
}

impl ProviderSchemaFragment {
    #[must_use]
    pub fn new(schema: Value) -> Self {
        Self {
            schema,
            source_fragment: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: ProviderSchemaSource) -> Self {
        self.source_fragment = Some(ProviderSourceFragment::new(
            source,
            self.schema.clone(),
            self.schema.clone(),
        ));
        self
    }

    #[must_use]
    pub fn with_source_schema(
        mut self,
        source: ProviderSchemaSource,
        source_schema: Value,
    ) -> Self {
        self.source_fragment = Some(ProviderSourceFragment::new(
            source,
            source_schema.clone(),
            source_schema,
        ));
        self
    }

    #[must_use]
    pub fn with_source_definition_schema(
        mut self,
        source: ProviderSchemaSource,
        source_schema: Value,
        definition_schema: Value,
    ) -> Self {
        self.source_fragment = Some(ProviderSourceFragment::new(
            source,
            source_schema,
            definition_schema,
        ));
        self
    }

    #[must_use]
    pub fn with_optional_source_schema(
        self,
        source: ProviderSchemaSource,
        source_schema: Option<Value>,
    ) -> Self {
        match source_schema {
            Some(source_schema) => self.with_source_schema(source, source_schema),
            None => self.with_source(source),
        }
    }

    #[must_use]
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    #[must_use]
    pub fn source(&self) -> Option<&ProviderSchemaSource> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::source)
    }

    #[must_use]
    pub fn source_schema(&self) -> Option<&Value> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::source_schema)
    }

    #[must_use]
    pub fn definition_schema(&self) -> Option<&Value> {
        self.source_fragment
            .as_ref()
            .map(ProviderSourceFragment::definition_schema)
    }

    #[must_use]
    pub fn source_fragment(&self) -> Option<&ProviderSourceFragment> {
        self.source_fragment.as_ref()
    }

    #[must_use]
    pub fn into_schema(self) -> Value {
        self.schema
    }

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
        } = self;
        let mapped = map_schema(&schema)?;
        let source_fragment = (mapped == schema).then_some(source_fragment).flatten();
        Some(Self {
            schema: mapped,
            source_fragment,
        })
    }
}

impl From<Value> for ProviderSchemaFragment {
    fn from(schema: Value) -> Self {
        Self::new(schema)
    }
}
