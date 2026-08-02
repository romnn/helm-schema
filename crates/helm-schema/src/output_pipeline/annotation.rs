use std::fmt::Write as _;

use helm_schema_gen::{POLICY_VOCABULARY_VERSION, ResolvedEmissionPolicy, SchemaProfile};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{CliError, EngineResult};
use crate::output_pipeline::ReferenceMode;
use crate::output_pipeline::overrides::PreparedOverridesIdentity;

const ANNOTATION_FORMAT_VERSION: u64 = 1;
const GENERATED_SCHEMA_MARKER_KEY: &str = "x-helm-schema-generated";
const POLICY_ANNOTATION_KEY: &str = "x-helm-schema-policy";
const DRAFT_07_URI: &str = "http://json-schema.org/draft-07/schema#";

#[derive(Debug, Clone, Copy)]
pub(crate) struct FinalOutputPolicy {
    requested_profile: Option<SchemaProfile>,
    resolved: ResolvedEmissionPolicy,
    infer_required: bool,
}

impl FinalOutputPolicy {
    pub(crate) fn for_profile(profile: SchemaProfile, infer_required: bool) -> Self {
        Self {
            requested_profile: Some(profile),
            resolved: profile.resolved_policy(),
            infer_required,
        }
    }
}

pub(crate) fn annotate_final_schema(
    schema: Value,
    policy: FinalOutputPolicy,
    overrides: &PreparedOverridesIdentity,
    reference_mode: ReferenceMode,
) -> EngineResult<Value> {
    let resolved = serde_json::to_value(policy.resolved)?;
    let narrowing = if policy.infer_required {
        vec![Value::String("infer-required".to_string())]
    } else {
        Vec::new()
    };
    let modifiers = serde_json::json!({
        "overrides": {
            "count": overrides.count,
            "digest": overrides.digest,
        },
        "reference-mode": reference_mode.annotation_name(),
    });
    let fingerprint_input = serde_json::json!({
        "policy-vocabulary-version": POLICY_VOCABULARY_VERSION,
        "resolved": resolved.clone(),
        "narrowing": narrowing.clone(),
        "modifiers": modifiers.clone(),
    });
    let canonical = helm_schema_json_schema_walk::canonical_json_string(&fingerprint_input);
    let digest = Sha256::digest(canonical.as_bytes());
    let mut fingerprint = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(fingerprint, "{byte:02x}");
    }

    let annotation = serde_json::json!({
        "annotation-format-version": ANNOTATION_FORMAT_VERSION,
        "policy-vocabulary-version": POLICY_VOCABULARY_VERSION,
        "requested-profile": policy.requested_profile.map(SchemaProfile::as_str),
        "resolved": resolved,
        "narrowing": narrowing,
        "modifiers": modifiers,
        "policy-fingerprint": fingerprint,
    });

    let mut object = match schema {
        Value::Object(object) => object,
        Value::Bool(boolean) => Map::from_iter([
            (
                "$schema".to_string(),
                Value::String(DRAFT_07_URI.to_string()),
            ),
            (
                "allOf".to_string(),
                Value::Array(vec![Value::Bool(boolean)]),
            ),
        ]),
        other => {
            return Err(CliError::InvalidFinalSchemaRoot {
                kind: json_kind(&other),
            });
        }
    };
    object.insert(GENERATED_SCHEMA_MARKER_KEY.to_string(), Value::Bool(true));
    object.insert(POLICY_ANNOTATION_KEY.to_string(), annotation);
    Ok(Value::Object(object))
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
#[path = "tests/annotation.rs"]
mod tests;
