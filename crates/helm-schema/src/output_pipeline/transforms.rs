use std::path::Path;

use helm_schema_json_schema_minify::minimize_schema;
use serde_json::Value;

use crate::error::EngineResult;
use crate::flatten;
use crate::output_pipeline::annotation::{FinalOutputPolicy, annotate_final_schema};
use crate::output_pipeline::descriptions::strip_schema_descriptions;
use crate::output_pipeline::{OutputPipelineOptions, PolicyInputs, ReferenceMode};
use crate::schema_override;

/// Applies overrides, reference policy, and final minimization.
///
/// # Errors
///
/// Returns an error when prepared references cannot be validated, bundled, or
/// fully inlined under the requested output policy.
#[tracing::instrument(
    skip_all,
    fields(
        override_count = policy_inputs.override_count(),
        reference_mode = ?options.reference_mode,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
pub(crate) fn apply_schema_output_pipeline(
    mut schema: Value,
    policy_inputs: PolicyInputs,
    base_dir: &Path,
    policy: FinalOutputPolicy,
    options: OutputPipelineOptions,
) -> EngineResult<Value> {
    let override_identity = policy_inputs.identity();
    for override_schema in policy_inputs.into_prepared_override_schemas() {
        schema = schema_override::apply_prepared_schema_override(schema, override_schema);
    }

    schema = apply_output_transforms(schema, base_dir, options)?;
    annotate_final_schema(schema, policy, &override_identity, options.reference_mode)
}

#[tracing::instrument(
    skip_all,
    fields(
        reference_mode = ?options.reference_mode,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
fn apply_output_transforms(
    mut schema: Value,
    base_dir: &Path,
    options: OutputPipelineOptions,
) -> EngineResult<Value> {
    match options.reference_mode {
        ReferenceMode::SelfContained => schema = flatten::bundle_prepared_refs(schema, base_dir)?,
        ReferenceMode::FullyInlinedExport => {
            schema = flatten::flatten_prepared_refs(&schema, base_dir)?;
        }
        ReferenceMode::PreserveRefs => {}
    }

    if options.strip_descriptions {
        strip_schema_descriptions(&mut schema);
    }

    if options.minimize {
        schema = minimize_schema(schema);
    }

    Ok(schema)
}

#[cfg(test)]
#[path = "tests/transforms.rs"]
mod tests;
