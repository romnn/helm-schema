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

/// Clone `schema` without its `$ref` key, keeping sibling keys.
pub(crate) fn strip_ref(schema: &Value) -> Value {
    let Some(object) = schema.as_object() else {
        return schema.clone();
    };
    let mut out = object.clone();
    out.remove("$ref");
    Value::Object(out)
}
