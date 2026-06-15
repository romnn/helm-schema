use serde_json::Value;

use crate::shared_defs::ShareableSchema;

/// Provider-owned schema evidence after value-use domain projection.
#[derive(Debug)]
pub(crate) struct ProviderSchemaEvidence {
    shareable_schema: ShareableSchema,
}

impl ProviderSchemaEvidence {
    pub(crate) fn new(schema: Value) -> Self {
        Self {
            shareable_schema: ShareableSchema::new(schema),
        }
    }

    pub(crate) fn schema(&self) -> &Value {
        self.shareable_schema.schema()
    }

    pub(crate) fn shareable_schema(&self) -> &ShareableSchema {
        &self.shareable_schema
    }
}
