//! Trace payloads for independently checked representation observations.

use std::collections::BTreeSet;
use std::rc::Rc;

use serde_json::json;

use super::{HeapNode, HeapVersion, Reach, namespace::Namespace};

pub(super) fn heap(version: &HeapVersion) {
    if !tracing::enabled!(target: "helm_schema_ir::stage0", tracing::Level::DEBUG) {
        return;
    }
    let previous = version.node.parents().next();
    let serde_json::Value::Object(mut row) = (match &version.node {
        HeapNode::Empty => json!({"kind": "empty"}),
        HeapNode::Registration { parent: _, paths } => {
            json!({"kind": "registration", "registration_matches": Rc::ptr_eq(paths, &version.input_paths)})
        }
        HeapNode::Install {
            parent: _,
            object,
            storage: _,
            kind,
        } => json!({"kind": "install", "object": object.ordinal, "install": format!("{kind:?}")}),
        HeapNode::Write {
            parent: _,
            object,
            key,
            field: _,
        } => json!({"kind": "write", "object": object.ordinal, "key": key}),
        HeapNode::WriteSlot {
            parent: _,
            object,
            index,
            value: _,
        } => json!({"kind": "write_slot", "object": object.ordinal, "key": index.to_string()}),
        HeapNode::UnknownWrite {
            parent: _,
            objects,
            inputs: _,
        } => {
            let objects = match objects {
                Reach::Bounded(objects) => {
                    Some(objects.iter().map(|id| id.ordinal).collect::<Vec<_>>())
                }
                Reach::Top => None,
            };
            json!({"kind": "unknown_write", "objects": objects})
        }
        HeapNode::Join {
            decision: _,
            yes: _,
            no: _,
        } => json!({"kind": "join"}),
    }) else {
        return;
    };
    row.insert(
        "parents".into(),
        json!(
            version
                .node
                .parents()
                .map(|parent| parent.id.ordinal)
                .collect::<Vec<_>>()
        ),
    );
    let additions = version.input_paths.added_paths(
        &previous.map_or_else(super::input_paths::InputPaths::default, |parent| {
            parent.input_paths.as_ref().clone()
        }),
    );
    row.insert(
        "paths".into(),
        json!(
            additions
                .iter()
                .map(|(id, path)| (id.ordinal, path))
                .collect::<Vec<_>>()
        ),
    );
    row.insert("event".into(), json!("heap"));
    row.insert(
        "arena".into(),
        json!(format!("{:p}", version.arena.as_ptr())),
    );
    row.insert("version".into(), json!(version.id.ordinal));
    row.insert(
        "changes_inputs".into(),
        json!(version.index.changes_inputs()),
    );
    row.insert(
        "writes".into(),
        json!(version.index.input_writes.paths().collect::<Vec<_>>()),
    );
    tracing::debug!(target: "helm_schema_ir::stage0", payload = %serde_json::Value::Object(row));
}

pub(super) fn certificate(source: &str, namespace: &Namespace, targets: Option<&BTreeSet<String>>) {
    if tracing::enabled!(target: "helm_schema_ir::stage0", tracing::Level::DEBUG) {
        let row = json!({"event": "certificate", "source": source,
            "local_templates": namespace.same_source_names(source), "required": targets.is_none()});
        tracing::debug!(target: "helm_schema_ir::stage0", payload = %row);
    }
}
