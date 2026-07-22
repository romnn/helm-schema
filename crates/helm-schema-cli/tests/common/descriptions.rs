use color_eyre::eyre::{self, WrapErr};
use helm_schema_ast::extract_values_yaml_descriptions;
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

pub fn assert_chart_values_comments_apply_to_existing_schema_paths(
    chart_relative_path: &str,
    schema: &Value,
    min_applied: usize,
) -> eyre::Result<()> {
    let values_yaml = crate::values_yaml::read_values_yaml_for_path(chart_relative_path)
        .wrap_err("read values.yaml")?;
    let descriptions = extract_values_yaml_descriptions(&values_yaml);

    let mut applied = 0;
    for (path, expected_description) in descriptions {
        let Some(schema_node) = schema_node_for_values_path(schema, &path) else {
            continue;
        };
        applied += 1;
        sim_assert_eq!(
            have: schema_node
                .get("description")
                .and_then(serde_json::Value::as_str),
            want: Some(expected_description.as_str()),
            "description mismatch for values path {path}"
        );
    }

    assert!(
        applied >= min_applied,
        "expected at least {min_applied} values comments to apply for {chart_relative_path}, got {applied}"
    );
    Ok(())
}

fn schema_node_for_values_path<'schema>(
    schema: &'schema Value,
    path: &str,
) -> Option<&'schema Value> {
    let mut current = schema;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = resolve_local_ref(schema, current);
        current = if segment == "*" {
            current.get("items")?
        } else {
            current.get("properties")?.get(segment)?
        };
    }
    Some(resolve_local_ref(schema, current))
}

/// Interned subtrees live in root-level `$defs`; follow local refs so
/// values-path lookups see through the output interning.
fn resolve_local_ref<'schema>(root: &'schema Value, node: &'schema Value) -> &'schema Value {
    node.get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| reference.strip_prefix("#/$defs/"))
        .and_then(|name| root.pointer(&format!("/$defs/{name}")))
        .unwrap_or(node)
}
