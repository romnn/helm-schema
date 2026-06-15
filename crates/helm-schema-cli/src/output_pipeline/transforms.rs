use std::path::Path;

use json_schema_minify::{MinimizeOptions, minimize_schema};
use serde_json::Value;

use crate::error::CliResult;
use crate::flatten;
use crate::output_pipeline::descriptions::strip_schema_descriptions;
use crate::output_pipeline::global_mirror::mirror_global_schema_into_subcharts;
use crate::output_pipeline::{OutputPipelineOptions, PolicyInputs};
use crate::schema_override;

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
pub(crate) fn apply_schema_output_pipeline(
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

    apply_output_transforms(schema, base_dir, options)
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
        schema = minimize_schema(schema, &MinimizeOptions::default()).schema;
    }

    Ok(schema)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use crate::output_pipeline::{
        OutputPipelineOptions, PolicyInputs, ReferenceMode, apply_schema_output_pipeline,
    };

    #[test]
    fn reference_mode_preserves_refs_when_requested() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "fromRef": {
                    "$ref": "./shared.json#/definitions/stringValue"
                }
            },
            "type": "object"
        });

        let output = apply_schema_output_pipeline(
            schema,
            PolicyInputs::default(),
            &[],
            std::path::Path::new("/does/not/matter"),
            &OutputPipelineOptions {
                reference_mode: ReferenceMode::PreserveRefs,
                strip_descriptions: false,
                minimize: false,
            },
        )
        .expect("apply output pipeline");

        assert_eq!(
            output
                .pointer("/properties/fromRef/$ref")
                .and_then(Value::as_str),
            Some("./shared.json#/definitions/stringValue"),
            "reference-preserving output mode should not dereference refs"
        );
    }

    #[test]
    fn self_contained_reference_mode_preserves_prepared_internal_refs() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {
                "stringValue": {
                    "type": "string"
                }
            },
            "properties": {
                "fromRef": {
                    "$ref": "#/$defs/stringValue"
                }
            },
            "type": "object"
        });

        let output = apply_schema_output_pipeline(
            schema,
            PolicyInputs::default(),
            &[],
            std::path::Path::new("/does/not/matter"),
            &OutputPipelineOptions {
                reference_mode: ReferenceMode::SelfContained,
                strip_descriptions: false,
                minimize: false,
            },
        )
        .expect("apply output pipeline");

        assert_eq!(
            output
                .pointer("/properties/fromRef/$ref")
                .and_then(Value::as_str),
            Some("#/$defs/stringValue"),
            "self-contained final output should keep prepared internal refs"
        );
        assert_eq!(
            output
                .pointer("/$defs/stringValue/type")
                .and_then(Value::as_str),
            Some("string"),
            "prepared definitions should remain available under $defs"
        );
    }

    #[test]
    fn self_contained_reference_mode_rejects_unprepared_external_refs() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "fromRef": {
                    "$ref": "./shared.json#/definitions/stringValue"
                }
            },
            "type": "object"
        });

        let err = apply_schema_output_pipeline(
            schema,
            PolicyInputs::default(),
            &[],
            std::path::Path::new("/does/not/matter"),
            &OutputPipelineOptions {
                reference_mode: ReferenceMode::SelfContained,
                strip_descriptions: false,
                minimize: false,
            },
        )
        .expect_err("unprepared external ref should fail final output transform");

        assert!(
            err.to_string()
                .contains("external $ref remained after input preparation"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn fully_inlined_export_reference_mode_inlines_prepared_internal_refs() {
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "$defs": {
                "stringValue": {
                    "type": "string"
                }
            },
            "properties": {
                "fromRef": {
                    "$ref": "#/$defs/stringValue"
                }
            },
            "type": "object"
        });

        let output = apply_schema_output_pipeline(
            schema,
            PolicyInputs::default(),
            &[],
            std::path::Path::new("/does/not/matter"),
            &OutputPipelineOptions {
                reference_mode: ReferenceMode::FullyInlinedExport,
                strip_descriptions: false,
                minimize: false,
            },
        )
        .expect("apply output pipeline");

        assert_eq!(
            output
                .pointer("/properties/fromRef/type")
                .and_then(Value::as_str),
            Some("string"),
            "fully inlined export mode should inline prepared internal refs"
        );
        assert!(output.pointer("/properties/fromRef/$ref").is_none());
    }
}
