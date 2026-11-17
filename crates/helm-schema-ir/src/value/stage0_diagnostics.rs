//! Trace payloads for independently checked helper-effect certificates.

use std::collections::BTreeSet;

use serde_json::json;

use super::namespace::Namespace;

pub(super) fn certificate(source: &str, namespace: &Namespace, targets: Option<&BTreeSet<String>>) {
    if tracing::enabled!(target: "helm_schema_ir::stage0", tracing::Level::DEBUG) {
        let row = json!({"event": "certificate", "source": source,
            "local_templates": namespace.same_source_names(source), "required": targets.is_none()});
        tracing::debug!(target: "helm_schema_ir::stage0", payload = %row);
    }
}
