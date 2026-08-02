use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::error::EngineResult;
use crate::flatten;
use crate::load_budget::read_to_end_capped;
use crate::output_pipeline::{PolicyInputOptions, ReferenceMode};
use crate::schema_override::{PreparedOverride, UnpreparedOverride};

/// Output policy inputs loaded from the filesystem before final schema
/// transforms run.
///
/// This keeps override IO and external `$ref` retrieval out of the pure output
/// transform path.
#[derive(Debug, Default)]
pub struct PolicyInputs {
    /// Override schema documents after file loading and output-mode
    /// preparation. The final output pipeline consumes these prepared
    /// documents as data, so override file IO and override merge policy
    /// stay separate.
    prepared_override_schemas: Vec<PreparedOverride>,
}

pub(super) struct PreparedOverridesIdentity {
    pub(super) count: usize,
    pub(super) digest: String,
}

impl PolicyInputs {
    pub(super) fn override_count(&self) -> usize {
        self.prepared_override_schemas.len()
    }

    pub(super) fn identity(&self) -> PreparedOverridesIdentity {
        let identities = Value::Array(
            self.prepared_override_schemas
                .iter()
                .map(PreparedOverride::identity)
                .collect(),
        );
        let canonical = helm_schema_json_schema_walk::canonical_json_string(&identities);
        let digest = Sha256::digest(canonical.as_bytes());
        let mut digest_hex = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(digest_hex, "{byte:02x}");
        }
        PreparedOverridesIdentity {
            count: self.prepared_override_schemas.len(),
            digest: digest_hex,
        }
    }

    pub(super) fn into_prepared_override_schemas(self) -> Vec<PreparedOverride> {
        self.prepared_override_schemas
    }
}

/// Loads and prepares override schemas according to reference and fetch policy.
///
/// Overrides must not reference generator-owned definitions in the inferred
/// document's `$defs`; their names and presence are private implementation
/// details. An override that needs a definition must carry that definition.
///
/// # Errors
///
/// Returns an error when an override exceeds its load budget, cannot be read
/// or decoded, or contains references that policy cannot prepare.
#[tracing::instrument(skip_all, fields(override_count = paths.len()))]
pub fn load_policy_inputs(
    paths: &[PathBuf],
    options: &PolicyInputOptions,
) -> EngineResult<PolicyInputs> {
    let prepared_override_schemas = paths
        .iter()
        .map(|path| load_prepared_override_schema(path, options))
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(PolicyInputs {
        prepared_override_schemas,
    })
}

#[tracing::instrument(skip_all)]
fn load_prepared_override_schema(
    path: &Path,
    options: &PolicyInputOptions,
) -> EngineResult<PreparedOverride> {
    let override_schema = load_json_file(path, options.load_budget.max_schema_document_bytes)?;
    if !override_schema.is_object() && !override_schema.is_boolean() {
        return Err(crate::error::CliError::InvalidOverrideRoot {
            path: path.to_path_buf(),
            kind: json_kind(&override_schema),
        });
    }
    let unprepared = UnpreparedOverride::capture(override_schema);
    let prepared_schema = prepare_override_schema(unprepared.schema(), path, options)?;
    Ok(unprepared.into_prepared(prepared_schema))
}

#[tracing::instrument(skip_all, fields(reference_mode = ?options.reference_mode))]
fn prepare_override_schema(
    schema: &Value,
    override_path: &Path,
    options: &PolicyInputOptions,
) -> EngineResult<Value> {
    let override_base = override_path.parent().unwrap_or_else(|| Path::new("."));

    match options.reference_mode {
        ReferenceMode::SelfContained => flatten::bundle_refs(
            schema.clone(),
            override_base,
            options.fetch_policy,
            options.load_budget,
        ),
        ReferenceMode::FullyInlinedExport => flatten::flatten_refs(
            schema,
            override_base,
            options.fetch_policy,
            options.load_budget,
        ),
        ReferenceMode::PreserveRefs => Ok(schema.clone()),
    }
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

fn load_json_file(path: &Path, max_bytes: usize) -> EngineResult<Value> {
    let mut file = std::fs::File::open(path)?;
    let bytes = read_to_end_capped(&mut file, max_bytes, path.display().to_string())?;
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

#[cfg(test)]
#[path = "tests/overrides.rs"]
mod tests;
