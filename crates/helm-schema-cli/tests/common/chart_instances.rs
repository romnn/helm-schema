use color_eyre::eyre::{self, WrapErr};
use serde_json::Value;

/// Compose a sparse values override over the chart defaults using Helm's
/// null-deletion behavior.
pub fn with_override(chart_relative_path: &str, override_value: Value) -> eyre::Result<Value> {
    let values_yaml = crate::values_yaml::read_values_yaml_for_path(chart_relative_path)
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

/// Delete null-valued map keys along MAP chains only. Helm's coalescing
/// treats lists atomically — a list value replaces wholesale and its
/// members (including nulls and null-valued keys inside them) reach the
/// template verbatim — so recursion must stop at arrays.
fn drop_nulls(value: &mut Value) {
    if let Value::Object(entries) = value {
        entries.retain(|_, value| !value.is_null());
        for value in entries.values_mut() {
            drop_nulls(value);
        }
    }
}
