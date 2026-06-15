use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::CliResult;
use crate::flatten::{self, FlattenOptions};
use crate::output_pipeline::OutputPipelineOptions;
use crate::schema_override;

/// Override schema document after file loading and output-mode preparation.
///
/// The final output pipeline consumes these prepared documents as data, so
/// override file IO and override merge policy stay separate.
#[derive(Debug)]
pub(crate) struct PreparedOverrideSchema {
    pub(super) schema: Value,
}

#[tracing::instrument(skip_all, fields(override_count = paths.len()))]
pub(crate) fn load_prepared_override_schemas(
    paths: &[PathBuf],
    options: &OutputPipelineOptions,
) -> CliResult<Vec<PreparedOverrideSchema>> {
    paths
        .iter()
        .map(|path| load_prepared_override_schema(path, options))
        .collect()
}

#[tracing::instrument(skip_all)]
fn load_prepared_override_schema(
    path: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<PreparedOverrideSchema> {
    let mut override_schema = load_json_file(path)?;

    // Tag every subtree that carries `$ref` with an internal "replace on
    // merge" marker. The marker survives reference preparation and tells
    // override merge to swap the prepared content into the base instead of
    // deep-merging it with inferred constraints for the same path.
    schema_override::mark_refs_for_replacement(&mut override_schema);

    let schema = prepare_override_schema(override_schema, path, options)?;
    Ok(PreparedOverrideSchema { schema })
}

#[tracing::instrument(skip_all, fields(reference_mode = ?options.reference_mode))]
fn prepare_override_schema(
    schema: Value,
    override_path: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    if options.reference_mode.bundles_refs() {
        let override_base = override_path.parent().unwrap_or_else(|| Path::new("."));
        return flatten::bundle_refs(
            schema,
            override_base,
            &FlattenOptions {
                allow_net: options.allow_net,
            },
        );
    }

    if !options.reference_mode.fully_inlines_refs() {
        return Ok(schema);
    }

    let override_base = override_path.parent().unwrap_or_else(|| Path::new("."));
    flatten::flatten_refs(
        schema,
        override_base,
        &FlattenOptions {
            allow_net: options.allow_net,
        },
    )
}

fn load_json_file(path: &Path) -> CliResult<Value> {
    let bytes = std::fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::Value;

    use crate::output_pipeline::{
        OutputPipelineOptions, ReferenceMode, apply_schema_output_pipeline,
        load_prepared_override_schemas,
    };

    fn test_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "helm-schema-output-pipeline-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn prepared_override_schemas_bundle_refs_before_merge() {
        let temp_dir = test_temp_dir("prepared-overrides");
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        fs::write(
            temp_dir.join("shared.json"),
            r#"{
                "definitions": {
                    "cloud": {
                        "enum": [null, "azure", "minikube"]
                    }
                }
            }"#,
        )
        .expect("write shared schema");
        let override_path = temp_dir.join("override.json");
        fs::write(
            &override_path,
            r#"{
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json#/definitions/cloud"
                    }
                }
            }"#,
        )
        .expect("write override schema");

        let options = OutputPipelineOptions {
            reference_mode: ReferenceMode::SelfContained,
            allow_net: false,
            strip_descriptions: false,
            minimize: false,
        };
        let overrides =
            load_prepared_override_schemas(&[override_path], &options).expect("load overrides");
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "cloud": {
                    "type": ["boolean", "string"]
                }
            },
            "type": "object"
        });

        let output = apply_schema_output_pipeline(schema, overrides, &[], &temp_dir, &options)
            .expect("apply output pipeline");

        let cloud = output.pointer("/properties/cloud").expect("cloud schema");
        assert_eq!(
            cloud,
            &serde_json::json!({
                "$ref": "#/$defs/schema1"
            }),
            "prepared override refs should replace inferred constraints with bundled refs"
        );
        assert_eq!(
            output.pointer("/$defs/schema1"),
            Some(&serde_json::json!({
                "enum": [null, "azure", "minikube"]
            })),
            "prepared override refs should carry resolved content under $defs"
        );

        fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    }

    #[test]
    fn fully_inlined_export_override_refs_resolve_before_merge() {
        let temp_dir = test_temp_dir("prepared-overrides-inline");
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        fs::write(
            temp_dir.join("shared.json"),
            r#"{
                "definitions": {
                    "cloud": {
                        "enum": [null, "azure", "minikube"]
                    }
                }
            }"#,
        )
        .expect("write shared schema");
        let override_path = temp_dir.join("override.json");
        fs::write(
            &override_path,
            r#"{
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json#/definitions/cloud"
                    }
                }
            }"#,
        )
        .expect("write override schema");

        let options = OutputPipelineOptions {
            reference_mode: ReferenceMode::FullyInlinedExport,
            allow_net: false,
            strip_descriptions: false,
            minimize: false,
        };
        let overrides =
            load_prepared_override_schemas(&[override_path], &options).expect("load overrides");
        let schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "properties": {
                "cloud": {
                    "type": ["boolean", "string"]
                }
            },
            "type": "object"
        });

        let output = apply_schema_output_pipeline(schema, overrides, &[], &temp_dir, &options)
            .expect("apply output pipeline");

        let cloud = output.pointer("/properties/cloud").expect("cloud schema");
        assert_eq!(
            cloud,
            &serde_json::json!({
                "enum": [null, "azure", "minikube"]
            }),
            "fully inlined export refs should replace inferred constraints after dereferencing"
        );

        fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    }

    #[test]
    fn override_refs_are_preserved_when_reference_mode_preserves_refs() {
        let temp_dir = test_temp_dir("prepared-overrides-keep-refs");
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let override_path = temp_dir.join("override.json");
        fs::write(
            &override_path,
            r#"{
                "properties": {
                    "cloud": {
                        "$ref": "./shared.json#/definitions/cloud"
                    }
                }
            }"#,
        )
        .expect("write override schema");

        let options = OutputPipelineOptions {
            reference_mode: ReferenceMode::PreserveRefs,
            allow_net: false,
            strip_descriptions: false,
            minimize: false,
        };
        let overrides =
            load_prepared_override_schemas(&[override_path], &options).expect("load overrides");
        let schema = serde_json::json!({
            "properties": {
                "cloud": {
                    "type": "string"
                }
            },
            "type": "object"
        });

        let output = apply_schema_output_pipeline(schema, overrides, &[], &temp_dir, &options)
            .expect("apply output pipeline");

        assert_eq!(
            output
                .pointer("/properties/cloud/$ref")
                .and_then(Value::as_str),
            Some("./shared.json#/definitions/cloud"),
        );

        fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    }
}
