use color_eyre::eyre::{Report, WrapErr};
use serde_json::Value;

/// Compose a sparse values override over the chart defaults using Helm's
/// null-deletion behavior.
pub fn with_override(
    chart_relative_path: &str,
    override_value: Value,
) -> std::result::Result<Value, Report> {
    let values_yaml = crate::schema_roundtrip::read_values_yaml_for_path(chart_relative_path)
        .wrap_err("read values.yaml")?;
    let mut values: Value = serde_yaml::from_str(&values_yaml).wrap_err("parse values.yaml")?;
    drop_nulls(&mut values);
    merge_override(&mut values, override_value);
    Ok(values)
}

fn merge_override(base: &mut Value, override_value: Value) {
    let overrides = match override_value {
        Value::Object(overrides) => overrides,
        mut value => {
            drop_nulls(&mut value);
            *base = value;
            return;
        }
    };
    if !base.is_object() {
        *base = Value::Object(serde_json::Map::new());
    }
    let Some(base) = base.as_object_mut() else {
        return;
    };
    for (key, mut value) in overrides {
        if value.is_null() {
            base.remove(&key);
        } else if let Some(existing) = base.get_mut(&key) {
            merge_override(existing, value);
        } else {
            drop_nulls(&mut value);
            base.insert(key, value);
        }
    }
}

fn drop_nulls(value: &mut Value) {
    match value {
        Value::Array(items) => {
            items.retain(|item| !item.is_null());
            for item in items {
                drop_nulls(item);
            }
        }
        Value::Object(entries) => {
            entries.retain(|_, value| !value.is_null());
            for value in entries.values_mut() {
                drop_nulls(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}
