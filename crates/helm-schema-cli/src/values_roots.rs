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
}
