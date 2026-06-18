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
mod tests {
    use super::*;

    #[test]
    fn extracts_sorted_top_level_mapping_keys_only() {
        let paths = top_level_value_paths(Some(
            r#"
z:
  nested: true
a: 1
"quoted": value
"#,
        ));

        assert_eq!(
            paths,
            BTreeSet::from(["a".to_string(), "quoted".to_string(), "z".to_string()])
        );
    }

    #[test]
    fn ignores_non_mapping_documents_and_empty_keys() {
        assert!(top_level_value_paths(Some("- item\n")).is_empty());
        assert!(top_level_value_paths(Some("\"\": value\n")).is_empty());
        assert!(top_level_value_paths(None).is_empty());
    }

    #[test]
    fn extracts_nested_explicit_mapping_paths() {
        let paths = explicit_value_paths(Some(
            r#"
controller:
  kind: Deployment
  admissionWebhooks:
    enabled: true
tcp: {}
items:
  - name: first
"#,
        ));

        assert_eq!(
            paths,
            BTreeSet::from([
                "controller".to_string(),
                "controller.admissionWebhooks".to_string(),
                "controller.admissionWebhooks.enabled".to_string(),
                "controller.kind".to_string(),
                "items".to_string(),
                "tcp".to_string(),
            ])
        );
    }

    #[test]
    fn explicit_paths_ignore_non_mapping_documents_and_empty_keys() {
        assert!(explicit_value_paths(Some("- item\n")).is_empty());
        assert!(explicit_value_paths(Some("\"\": value\n")).is_empty());
        assert!(explicit_value_paths(None).is_empty());
    }
}
