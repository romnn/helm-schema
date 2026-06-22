use std::collections::BTreeSet;

use serde_yaml::Value as YamlValue;

/// Return the explicit top-level keys present in the composed values.yaml.
///
/// These keys are structural evidence that the chart accepts the root value
/// path, but they do not imply that any nested key should exist.
pub(crate) fn top_level_value_paths(values_yaml: Option<&str>) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let Some(values_yaml) = values_yaml else {
        return paths;
    };
    let Ok(doc) = serde_yaml::from_str::<YamlValue>(values_yaml) else {
        return paths;
    };
    let YamlValue::Mapping(mapping) = doc else {
        return paths;
    };

    for (key, _) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        let key = key.trim();
        if !key.is_empty() {
            paths.insert(key.to_string());
        }
    }

    paths
}

/// Return every explicit mapping-backed values path present in the composed
/// values.yaml.
///
/// This includes nested object keys such as `controller.kind` and
/// `controller.admissionWebhooks.enabled`, which are structural evidence that
/// the chart already ships a default at that path and therefore must not infer
/// it as user-required later.
pub(crate) fn explicit_value_paths(values_yaml: Option<&str>) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    let Some(values_yaml) = values_yaml else {
        return paths;
    };
    let Ok(doc) = serde_yaml::from_str::<YamlValue>(values_yaml) else {
        return paths;
    };
    collect_explicit_paths(&doc, &mut Vec::new(), &mut paths);
    paths
}

fn collect_explicit_paths(
    value: &YamlValue,
    current_path: &mut Vec<String>,
    out: &mut BTreeSet<String>,
) {
    if !current_path.is_empty() {
        out.insert(current_path.join("."));
    }

    let YamlValue::Mapping(mapping) = value else {
        return;
    };

    for (key, child) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }

        current_path.push(key.to_string());
        collect_explicit_paths(child, current_path, out);
        current_path.pop();
    }
}

#[cfg(test)]
#[path = "tests/values_roots.rs"]
mod tests;
