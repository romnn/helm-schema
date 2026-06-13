use std::sync::Arc;

use serde_json::Value;

/// Shared parsed JSON Schema document.
///
/// Providers load the same upstream documents repeatedly while descending
/// resource paths and resolving references. Keeping parsed documents behind an
/// `Arc` avoids cloning whole `serde_json::Value` trees at the raw document
/// cache boundary.
#[derive(Debug, Clone)]
pub(crate) struct SchemaDoc {
    root: Arc<Value>,
}

impl SchemaDoc {
    pub(crate) fn new(root: Value) -> Self {
        Self {
            root: Arc::new(root),
        }
    }

    pub(crate) fn root(&self) -> &Value {
        self.root.as_ref()
    }
}
