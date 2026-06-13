use std::io::Write;
use std::path::Path;

use json_schema_minify::{MinimizeOptions, minimize_schema};
use serde_json::Value;

use crate::error::CliResult;
use crate::flatten::{self, FlattenOptions};

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

#[tracing::instrument(skip_all, fields(keep_refs = options.keep_refs))]
pub(crate) fn prepare_override_schema(
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
pub(crate) fn apply_output_transforms(
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

    use super::strip_schema_descriptions;

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
}
