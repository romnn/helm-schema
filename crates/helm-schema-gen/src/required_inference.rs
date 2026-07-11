//! Heuristic required-inference for generated values schemas.
//!
//! Lives in its own module so the entire feature can be removed
//! cleanly. The output is a schema mutation that adds `required: [...]`
//! arrays at the parent objects of paths the chart references
//! unconditionally and never accesses via a `default` fallback.
//!
//! Why this is heuristic:
//!   - Helm truthiness in a positive header is not by itself proof that a
//!     value is user-required.
//!
//! The schemadiff tool already strips `required` arrays from both
//! sides before diffing — the only place this feature's output is
//! user-visible is the CLI's `--infer-required` flag. If the heuristic
//! ever proves more trouble than it's worth, deleting this file plus
//! the matching CLI module is the entire rip surface.

use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::ContractPathSchemaEvidence;
use serde_json::Value;

/// Mutate `schema` in place to add `required: [...]` arrays at the
/// parent objects of paths the chart references unconditionally and
/// never accesses via a `default` fallback.
///
/// `explicit_default_value_paths` should contain any values paths explicitly
/// present in the composed chart defaults. Those paths are already satisfied
/// by the chart and must not be inferred as user-required, even if they also
/// appear in positive guard headers.
///
/// Required paths are those checked in positive header positions, lacking
/// explicit chart defaults, and also consumed by at least one non-self-guarded
/// render use. This remains heuristic because Helm truthiness does not by
/// itself imply user-requiredness. The extra render-use eligibility check
/// filters out common feature-toggle and helper-override patterns like:
/// `if .Values.fullnameOverride }}{{ .Values.fullnameOverride }}{{ else }}...`.
pub fn apply_required_inference(
    schema: &mut Value,
    schema_evidence_by_value_path: &BTreeMap<String, ContractPathSchemaEvidence>,
    explicit_default_value_paths: &BTreeSet<String>,
) {
    for (path, evidence) in schema_evidence_by_value_path {
        if !evidence.is_required_inference_candidate()
            || explicit_default_value_paths.contains(path)
        {
            continue;
        }
        add_path_to_required(schema, path);
    }
}

/// Locate `path`'s parent object schema and add the leaf segment to its
/// `required` list (sorted, de-duplicated). Silently no-ops if the
/// schema doesn't have a property tree at that path — the schema's
/// inferred shape may not include every path that drives required-
/// inference (e.g. when the path is referenced only via a guard).
fn add_path_to_required(schema: &mut Value, vp: &str) {
    let parts = crate::split_value_path(vp);
    let Some((leaf, parents)) = parts.split_last() else {
        return;
    };
    let Some(parent) = navigate_to_object_property(schema, parents) else {
        return;
    };
    add_to_required_list(parent, leaf);
}

/// Walk `segments` through `.properties.<seg>` accessors. Returns
/// `None` if any intermediate level is missing or isn't an object.
fn navigate_to_object_property<'a>(
    schema: &'a mut Value,
    segments: &[String],
) -> Option<&'a mut Value> {
    let mut node = schema;
    for seg in segments {
        node = node
            .as_object_mut()?
            .get_mut("properties")?
            .as_object_mut()?
            .get_mut(seg.as_str())?;
    }
    Some(node)
}

/// Add `key` to `node`'s `required` array (creating it if missing).
/// Keeps the array sorted and de-duplicated.
fn add_to_required_list(node: &mut Value, key: &str) {
    let Some(obj) = node.as_object_mut() else {
        return;
    };
    let req = obj
        .entry("required".to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    let Some(arr) = req.as_array_mut() else {
        // Pre-existing non-array `required` — leave it alone rather
        // than overwrite a hand-authored shape we don't understand.
        return;
    };
    if !arr.iter().any(|v| v.as_str() == Some(key)) {
        arr.push(Value::String(key.to_string()));
    }
    arr.sort_by(|a, b| a.as_str().unwrap_or("").cmp(b.as_str().unwrap_or("")));
    arr.dedup();
}

#[cfg(test)]
#[path = "tests/required_inference.rs"]
mod tests;
