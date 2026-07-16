use std::collections::{BTreeMap, BTreeSet};

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
    /// Literal string defaults keyed by their composed values path.
    ///
    /// These remain chart-authored facts rather than accepted-input facts:
    /// consumers use them only when a template explicitly evaluates the
    /// selected default as a program (for example, `tpl .Values.query .`).
    pub(crate) string_defaults: BTreeMap<String, String>,
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
                let path = helm_schema_core::join_value_path([key]);
                roots.top_level_paths.insert(path.clone());
                if matches!(value, YamlValue::Mapping(_)) {
                    roots.top_level_mapping_paths.insert(path);
                }
            }
        }

        collect_values_facts(
            &doc,
            &mut Vec::new(),
            &mut roots.explicit_paths,
            &mut roots.string_defaults,
        );
        roots
    }

    pub(crate) fn string_defaults_for_prefix(&self, prefix: &[String]) -> BTreeMap<String, String> {
        self.string_defaults
            .iter()
            .filter_map(|(path, value)| {
                let segments = helm_schema_core::split_value_path(path);
                segments
                    .strip_prefix(prefix)
                    .filter(|relative| !relative.is_empty())
                    .map(|relative| {
                        (
                            helm_schema_core::join_value_path(relative.iter().cloned()),
                            value.clone(),
                        )
                    })
            })
            .collect()
    }
}

fn collect_values_facts(
    value: &YamlValue,
    current_path: &mut Vec<String>,
    explicit_paths: &mut BTreeSet<String>,
    string_defaults: &mut BTreeMap<String, String>,
) {
    if !current_path.is_empty() {
        let path = helm_schema_core::join_value_path(&*current_path);
        explicit_paths.insert(path.clone());
        if let YamlValue::String(value) = value {
            string_defaults.insert(path, value.clone());
        }
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
        collect_values_facts(child, current_path, explicit_paths, string_defaults);
        current_path.pop();
    }
}

#[cfg(test)]
#[path = "tests/values_roots.rs"]
mod tests;
