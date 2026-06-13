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
    pub(crate) keep_refs: bool,
    pub(crate) allow_net: bool,
    pub(crate) strip_descriptions: bool,
    pub(crate) minimize: bool,
}

#[tracing::instrument(
    skip_all,
    fields(
        override_count = override_paths.len(),
        subchart_count = subchart_value_prefixes.len(),
        keep_refs = options.keep_refs,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
pub(crate) fn apply_schema_output_pipeline(
    mut schema: Value,
    override_paths: &[PathBuf],
    subchart_value_prefixes: &[Vec<String>],
    base_dir: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    for path in override_paths {
        let override_schema = load_prepared_override_schema(path, options)?;
        schema = schema_override::apply_schema_override(schema, override_schema);
    }

    mirror_global_schema_into_subcharts(&mut schema, subchart_value_prefixes);

    apply_output_transforms(schema, base_dir, options)
}

#[tracing::instrument(skip_all)]
fn load_prepared_override_schema(path: &Path, options: &OutputPipelineOptions) -> CliResult<Value> {
    let mut override_schema = load_json_file(path)?;

    // Tag every subtree that carries `$ref` with an internal "replace on
    // merge" marker. The marker survives pre-flatten dereferencing and tells
    // override merge to swap the resolved content into the base instead of
    // deep-merging it with inferred constraints for the same path.
    schema_override::mark_refs_for_replacement(&mut override_schema);

    prepare_override_schema(override_schema, path, options)
}

#[tracing::instrument(skip_all, fields(keep_refs = options.keep_refs))]
fn prepare_override_schema(
    schema: Value,
    override_path: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    if options.keep_refs {
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
        keep_refs = options.keep_refs,
        strip_descriptions = options.strip_descriptions,
        minimize = options.minimize,
    )
)]
fn apply_output_transforms(
    mut schema: Value,
    base_dir: &Path,
    options: &OutputPipelineOptions,
) -> CliResult<Value> {
    if !options.keep_refs {
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

#[tracing::instrument(skip_all, fields(compact = compact))]
pub(crate) fn write_schema_json(
    out: &mut impl Write,
    schema: &Value,
    compact: bool,
) -> CliResult<()> {
    if compact {
        serde_json::to_writer(&mut *out, schema)?;
    } else {
        serde_json::to_writer_pretty(&mut *out, schema)?;
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
    use serde_json::Value;

    use super::{mirror_global_schema_into_subcharts, strip_schema_descriptions};

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
