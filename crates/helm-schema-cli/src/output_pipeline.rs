use std::io::Write;
use std::path::{Path, PathBuf};

use json_schema_minify::{MinimizeOptions, minimize_schema};
use serde_json::{Map, Value};

use crate::error::CliResult;
use crate::flatten::{self, FlattenOptions};
use crate::schema_override;

/// Output-only schema transforms selected by CLI flags.
///
/// These transforms run after inference and override merging. They must not
/// feed information back into template analysis.
#[derive(Debug, Clone, Copy)]
pub(crate) struct OutputPipelineOptions {
    pub(crate) reference_handling: ReferenceHandling,
    pub(crate) allow_net: bool,
    pub(crate) strip_descriptions: bool,
    pub(crate) minimize: bool,
}

/// Override schema document after file loading and output-mode preparation.
///
/// The final output pipeline consumes these prepared documents as data, so
/// override file IO and override merge policy stay separate.
#[derive(Debug)]
pub(crate) struct PreparedOverrideSchema {
    schema: Value,
}

/// How final output should handle JSON Schema `$ref` nodes.
///
/// This is an output concern only. It controls whether the final schema stays
/// as a bundled document with references or is exported as a fully flattened
/// document for consumers that cannot resolve references themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReferenceHandling {
    PreserveRefs,
    FlattenedExport,
}

impl ReferenceHandling {
    pub(crate) fn from_keep_refs(keep_refs: bool) -> Self {
        if keep_refs {
            Self::PreserveRefs
        } else {
            Self::FlattenedExport
        }
    }

    fn flatten(self) -> bool {
        matches!(self, Self::FlattenedExport)
    }
}

/// JSON serialization format for the final schema document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JsonOutputFormat {
    Pretty,
    Compact,
}

impl JsonOutputFormat {
    pub(crate) fn from_compact(compact: bool) -> Self {
        if compact { Self::Compact } else { Self::Pretty }
    }
}

#[tracing::instrument(
    skip_all,
    fields(
        override_count = override_schemas.len(),
        subchart_count = subchart_value_prefixes.len(),
        reference_handling = ?options.reference_handling,
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
    // merge" marker. The marker survives pre-flatten dereferencing and tells
    // override merge to swap the resolved content into the base instead of
    // deep-merging it with inferred constraints for the same path.
    schema_override::mark_refs_for_replacement(&mut override_schema);

    let schema = prepare_override_schema(override_schema, path, options)?;
    Ok(PreparedOverrideSchema { schema })
}

#[tracing::instrument(skip_all, fields(reference_handling = ?options.reference_handling))]
fn prepare_override_schema(
    schema: Value,
    override_path: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    if !options.reference_handling.flatten() {
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

#[tracing::instrument(
    skip_all,
    fields(
        reference_handling = ?options.reference_handling,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
fn apply_output_transforms(
    mut schema: Value,
    base_dir: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    if options.reference_handling.flatten() {
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

#[tracing::instrument(skip_all, fields(format = ?format))]
pub(crate) fn write_schema_json(
    out: &mut impl Write,
    schema: &Value,
    format: JsonOutputFormat,
) -> CliResult<()> {
    match format {
        JsonOutputFormat::Compact => serde_json::to_writer(&mut *out, schema)?,
        JsonOutputFormat::Pretty => serde_json::to_writer_pretty(&mut *out, schema)?,
    }
    out.write_all(b"\n")?;
    Ok(())
}

fn mirror_global_schema_into_subcharts(schema: &mut Value, subchart_prefixes: &[Vec<String>]) {
    let Some(root_global_schema) = schema.pointer("/properties/global").cloned() else {
        return;
    };

    for prefix in subchart_prefixes {
        let subchart_schema = schema_object_at_values_prefix(schema, prefix);
        let subchart_global_schema = schema_property_mut(subchart_schema, "global");
        let existing = std::mem::take(subchart_global_schema);
        *subchart_global_schema =
            schema_override::apply_schema_override(existing, root_global_schema.clone());
    }
}

fn schema_object_at_values_prefix<'a>(schema: &'a mut Value, prefix: &[String]) -> &'a mut Value {
    let mut current = schema;
    for segment in prefix {
        current = schema_property_mut(current, segment);
    }
    current
}

fn schema_property_mut<'a>(schema: &'a mut Value, property: &str) -> &'a mut Value {
    let object = ensure_json_object(schema);
    let properties = object
        .entry("properties".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let properties = ensure_json_object(properties);
    properties
        .entry(property.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
}

fn ensure_json_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    match value {
        Value::Object(object) => object,
        _ => unreachable!("json value was just replaced with an object"),
    }
}

fn load_json_file(path: &Path) -> CliResult<Value> {
    let bytes = std::fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

fn strip_schema_descriptions(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };

    object.remove("description");

    for key in [
        "additionalItems",
        "additionalProperties",
        "contains",
        "else",
        "if",
        "not",
        "propertyNames",
        "then",
        "unevaluatedItems",
        "unevaluatedProperties",
    ] {
        if let Some(child) = object.get_mut(key) {
            strip_schema_descriptions(child);
        }
    }

    if let Some(items) = object.get_mut("items") {
        strip_schema_or_schema_array_descriptions(items);
    }

    for key in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "dependencies",
        "patternProperties",
        "properties",
    ] {
        if let Some(Value::Object(children)) = object.get_mut(key) {
            for child in children.values_mut() {
                strip_schema_descriptions(child);
            }
        }
    }

    for key in ["allOf", "anyOf", "oneOf", "prefixItems"] {
        if let Some(Value::Array(children)) = object.get_mut(key) {
            for child in children {
                strip_schema_descriptions(child);
            }
        }
    }
}

fn strip_schema_or_schema_array_descriptions(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                strip_schema_descriptions(item);
            }
        }
        value => strip_schema_descriptions(value),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::Value;

    use super::{
        JsonOutputFormat, OutputPipelineOptions, ReferenceHandling, apply_schema_output_pipeline,
        load_prepared_override_schemas, mirror_global_schema_into_subcharts,
        strip_schema_descriptions, write_schema_json,
    };

    fn test_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "helm-schema-output-pipeline-{name}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn reference_handling_preserves_refs_when_requested() {
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
                reference_handling: ReferenceHandling::PreserveRefs,
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
    fn reference_handling_flattened_export_resolves_file_refs() {
        let temp_dir = test_temp_dir("flattened-export");
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
                reference_handling: ReferenceHandling::FlattenedExport,
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
            "flattened export mode should inline file refs"
        );
        assert!(output.pointer("/properties/fromRef/$ref").is_none());

        fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    }

    #[test]
    fn prepared_override_schemas_resolve_refs_before_merge() {
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
            reference_handling: ReferenceHandling::FlattenedExport,
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
            "prepared override refs should replace inferred constraints after pre-flattening"
        );

        fs::remove_dir_all(&temp_dir).expect("remove temp dir");
    }

    #[test]
    fn json_output_format_controls_pretty_vs_compact_serialization() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string"
                }
            }
        });

        let mut pretty = Vec::new();
        write_schema_json(&mut pretty, &schema, JsonOutputFormat::Pretty).expect("write pretty");
        let pretty = String::from_utf8(pretty).expect("pretty utf8");
        assert!(
            pretty.contains("\n  "),
            "pretty output should contain indentation: {pretty}"
        );

        let mut compact = Vec::new();
        write_schema_json(&mut compact, &schema, JsonOutputFormat::Compact).expect("write compact");
        let compact = String::from_utf8(compact).expect("compact utf8");
        assert_eq!(
            compact,
            r#"{"properties":{"name":{"type":"string"}},"type":"object"}"#.to_string() + "\n"
        );
    }

    #[test]
    fn strip_schema_descriptions_preserves_description_value_property() {
        let mut schema = serde_json::json!({
            "description": "root annotation",
            "type": "object",
            "properties": {
                "description": {
                    "description": "value property annotation",
                    "type": "string"
                },
                "nested": {
                    "description": "nested annotation",
                    "type": "object",
                    "properties": {
                        "description": {
                            "description": "nested value property annotation",
                            "type": "string"
                        }
                    }
                }
            },
            "$defs": {
                "shared": {
                    "description": "shared annotation",
                    "type": "string"
                }
            },
            "items": [
                {
                    "description": "tuple item annotation",
                    "type": "string"
                }
            ]
        });

        strip_schema_descriptions(&mut schema);

        assert!(schema.get("description").is_none());
        assert_eq!(
            schema.pointer("/properties/description/type"),
            Some(&Value::String("string".to_string())),
        );
        assert!(
            schema
                .pointer("/properties/description/description")
                .is_none(),
        );
        assert_eq!(
            schema.pointer("/properties/nested/properties/description/type"),
            Some(&Value::String("string".to_string())),
        );
        assert!(
            schema
                .pointer("/properties/nested/properties/description/description")
                .is_none(),
        );
        assert!(schema.pointer("/$defs/shared/description").is_none());
        assert_eq!(
            schema.pointer("/items/0/type"),
            Some(&Value::String("string".to_string())),
        );
        assert!(schema.pointer("/items/0/description").is_none());
    }

    #[test]
    fn shared_global_override_schema_is_mirrored_into_nested_subcharts() {
        let mut schema = serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "global": {
                    "additionalProperties": true,
                    "properties": {
                        "kube-score/ignore": {
                            "type": "string"
                        }
                    },
                    "type": "object"
                },
                "oauth2-proxy": {
                    "additionalProperties": false,
                    "properties": {
                        "global": {
                            "additionalProperties": false,
                            "properties": {
                                "imageRegistry": {
                                    "type": "string"
                                }
                            },
                            "type": "object"
                        },
                        "redis": {
                            "additionalProperties": false,
                            "properties": {
                                "global": {
                                    "additionalProperties": false,
                                    "properties": {
                                        "storageClass": {
                                            "type": "string"
                                        }
                                    },
                                    "type": "object"
                                }
                            },
                            "type": "object"
                        }
                    },
                    "type": "object"
                }
            },
            "type": "object"
        });

        mirror_global_schema_into_subcharts(
            &mut schema,
            &[
                vec!["oauth2-proxy".to_string()],
                vec!["oauth2-proxy".to_string(), "redis".to_string()],
            ],
        );

        let child_global = schema
            .pointer("/properties/oauth2-proxy/properties/global")
            .expect("child global schema");
        assert_eq!(
            child_global
                .pointer("/properties/kube-score~1ignore/type")
                .and_then(Value::as_str),
            Some("string"),
            "shared global property should be mirrored into child global: {child_global}"
        );
        assert_eq!(
            child_global
                .pointer("/properties/imageRegistry/type")
                .and_then(Value::as_str),
            Some("string"),
            "child global-specific properties should be preserved: {child_global}"
        );
        assert_eq!(
            child_global
                .get("additionalProperties")
                .and_then(Value::as_bool),
            Some(true),
            "shared open-global policy should be mirrored into child global: {child_global}"
        );

        let nested_global = schema
            .pointer("/properties/oauth2-proxy/properties/redis/properties/global")
            .expect("nested global schema");
        assert_eq!(
            nested_global
                .pointer("/properties/kube-score~1ignore/type")
                .and_then(Value::as_str),
            Some("string"),
            "shared global property should be mirrored into nested child global: {nested_global}"
        );
        assert_eq!(
            nested_global
                .pointer("/properties/storageClass/type")
                .and_then(Value::as_str),
            Some("string"),
            "nested child global-specific properties should be preserved: {nested_global}"
        );
        assert_eq!(
            nested_global
                .get("additionalProperties")
                .and_then(Value::as_bool),
            Some(true),
            "shared open-global policy should be mirrored into nested child global: {nested_global}"
        );
    }
}
