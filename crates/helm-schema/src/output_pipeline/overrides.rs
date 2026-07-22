use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::EngineResult;
use crate::flatten;
use crate::load_budget::read_to_end_capped;
use crate::output_pipeline::{PolicyInputOptions, ReferenceMode};
use crate::schema_override;

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
    prepared_override_schemas: Vec<Value>,
}

impl PolicyInputs {
    pub(super) fn override_count(&self) -> usize {
        self.prepared_override_schemas.len()
    }

    pub(super) fn into_prepared_override_schemas(self) -> Vec<Value> {
        self.prepared_override_schemas
    }
}

/// Loads and prepares override schemas according to reference and fetch policy.
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
fn load_prepared_override_schema(path: &Path, options: &PolicyInputOptions) -> EngineResult<Value> {
    let mut override_schema = load_json_file(path, options.load_budget.max_schema_document_bytes)?;

    // Tag every subtree that carries `$ref` with an internal "replace on
    // merge" marker. The marker survives reference preparation and tells
    // override merge to swap the prepared content into the base instead of
    // deep-merging it with inferred constraints for the same path.
    schema_override::mark_refs_for_replacement(&mut override_schema);

    prepare_override_schema(override_schema, path, options)
}

#[tracing::instrument(skip_all, fields(reference_mode = ?options.reference_mode))]
fn prepare_override_schema(
    schema: Value,
    override_path: &Path,
    options: &PolicyInputOptions,
) -> EngineResult<Value> {
    let override_base = override_path.parent().unwrap_or_else(|| Path::new("."));

    match options.reference_mode {
        ReferenceMode::SelfContained => flatten::bundle_refs(
            schema,
            override_base,
            options.fetch_policy,
            options.load_budget,
        ),
        ReferenceMode::FullyInlinedExport => flatten::flatten_refs(
            &schema,
            override_base,
            options.fetch_policy,
            options.load_budget,
        ),
        ReferenceMode::PreserveRefs => Ok(schema),
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
