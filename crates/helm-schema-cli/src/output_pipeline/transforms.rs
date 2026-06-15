use std::path::Path;

use json_schema_minify::{MinimizeOptions, minimize_schema};
use serde_json::Value;

use crate::error::CliResult;
use crate::flatten::{self, FlattenOptions};
use crate::output_pipeline::descriptions::strip_schema_descriptions;
use crate::output_pipeline::global_mirror::mirror_global_schema_into_subcharts;
use crate::output_pipeline::{OutputPipelineOptions, PreparedOverrideSchema};
use crate::schema_override;

#[tracing::instrument(
    skip_all,
    fields(
        override_count = override_schemas.len(),
        subchart_count = subchart_value_prefixes.len(),
        reference_mode = ?options.reference_mode,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
pub(crate) fn apply_schema_output_pipeline(
    mut schema: Value,
    override_schemas: Vec<PreparedOverrideSchema>,
    subchart_value_prefixes: &[Vec<String>],
    base_dir: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    for override_schema in override_schemas {
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
    if options.reference_mode.dereference() {
        schema = flatten::flatten_refs(
            schema,
            base_dir,
            &FlattenOptions {
                allow_net: options.allow_net,
            },
        )?;
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
    use std::fs;
    use std::path::PathBuf;

    use serde_json::Value;

    use crate::output_pipeline::{
        OutputPipelineOptions, ReferenceMode, apply_schema_output_pipeline,
    };

    fn test_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "helm-schema-output-pipeline-{name}-{}",
            std::process::id()
        ))
    }

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
            Vec::new(),
            &[],
            std::path::Path::new("/does/not/matter"),
            &OutputPipelineOptions {
                reference_mode: ReferenceMode::PreserveRefs,
                allow_net: false,
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
    fn self_contained_reference_mode_resolves_file_refs() {
        let temp_dir = test_temp_dir("self-contained");
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let shared_schema_path = temp_dir.join("shared.json");
        fs::write(
            &shared_schema_path,
            r#"{
                "definitions": {
                    "stringValue": {
                        "type": "string"
                    }
                }
            }"#,
        )
        .expect("write shared schema");

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
            Vec::new(),
            &[],
            &temp_dir,
            &OutputPipelineOptions {
                reference_mode: ReferenceMode::SelfContained,
                allow_net: false,
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
            "self-contained mode should inline file refs"
        );
        assert!(output.pointer("/properties/fromRef/$ref").is_none());

        fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    }
}
