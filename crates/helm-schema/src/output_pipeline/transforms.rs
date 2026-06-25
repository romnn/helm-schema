use std::path::Path;

use json_schema_minify::minimize_schema;
use serde_json::Value;

use crate::error::CliResult;
use crate::flatten;
use crate::output_pipeline::descriptions::strip_schema_descriptions;
use crate::output_pipeline::global_mirror::mirror_global_schema_into_subcharts;
use crate::output_pipeline::{OutputPipelineOptions, PolicyInputs};
use crate::schema_override;

const GENERATED_SCHEMA_MARKER_KEY: &str = "x-helm-schema-generated";

#[tracing::instrument(
    skip_all,
    fields(
        override_count = policy_inputs.override_count(),
        subchart_count = subchart_value_prefixes.len(),
        reference_mode = ?options.reference_mode,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
pub fn apply_schema_output_pipeline(
    mut schema: Value,
    policy_inputs: PolicyInputs,
    subchart_value_prefixes: &[Vec<String>],
    base_dir: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    for override_schema in policy_inputs.into_prepared_override_schemas() {
        schema = schema_override::apply_schema_override(schema, override_schema.schema);
    }

    mirror_global_schema_into_subcharts(&mut schema, subchart_value_prefixes);

    schema = apply_output_transforms(schema, base_dir, options)?;
    mark_generated_schema(&mut schema);
    Ok(schema)
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
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    if options.reference_mode.bundles_refs() {
        schema = flatten::bundle_prepared_refs(schema, base_dir)?;
    } else if options.reference_mode.fully_inlines_refs() {
        schema = flatten::flatten_prepared_refs(schema, base_dir)?;
    }

    if options.strip_descriptions {
        strip_schema_descriptions(&mut schema);
    }

    if options.minimize {
        schema = minimize_schema(schema);
    }

    Ok(schema)
}

fn mark_generated_schema(schema: &mut Value) {
    if let Value::Object(object) = schema {
        object.insert(GENERATED_SCHEMA_MARKER_KEY.to_string(), Value::Bool(true));
    }
}

#[cfg(test)]
#[path = "tests/transforms.rs"]
mod tests;
