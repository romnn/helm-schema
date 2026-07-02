use std::collections::BTreeSet;

use serde_yaml::Value as YamlValue;

/// Structural evidence extracted from the composed values.yaml in one parse.
#[derive(Debug, Default)]
pub(crate) struct ValuesRoots {
    /// The explicit top-level keys present in the composed values.yaml.
    ///
    /// These keys are structural evidence that the chart accepts the root
    /// value path, but they do not imply that any nested key should exist.
    pub(crate) top_level_paths: BTreeSet<String>,
    /// The subset of top-level keys whose default value is a mapping.
    pub(crate) top_level_mapping_paths: BTreeSet<String>,
    /// Every explicit mapping-backed values path present in the composed
    /// values.yaml.
    ///
    /// This includes nested object keys such as `controller.kind` and
    /// `controller.admissionWebhooks.enabled`, which are structural evidence
    /// that the chart already ships a default at that path and therefore must
    /// not infer it as user-required later.
    pub(crate) explicit_paths: BTreeSet<String>,
}

impl ValuesRoots {
    pub(crate) fn from_values_yaml(values_yaml: Option<&str>) -> Self {
        let mut roots = Self::default();
        let Some(values_yaml) = values_yaml else {
            return roots;
        };
        let Ok(doc) = serde_yaml::from_str::<YamlValue>(values_yaml) else {
            return roots;
        };

        if let YamlValue::Mapping(mapping) = &doc {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else {
                    continue;
                };
                let key = key.trim();
                if key.is_empty() {
                    continue;
                }
                roots.top_level_paths.insert(key.to_string());
                if matches!(value, YamlValue::Mapping(_)) {
                    roots.top_level_mapping_paths.insert(key.to_string());
                }
            }
        }

        collect_explicit_paths(&doc, &mut Vec::new(), &mut roots.explicit_paths);
        roots
    }
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
