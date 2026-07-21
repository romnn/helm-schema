use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use helm_schema_ast::extract_values_yaml_descriptions;
use serde_yaml::Value as YamlValue;
use tracing::instrument;
use vfs::VfsPath;

use super::paths::scope_values_path;
use super::types::ChartContext;
use crate::error::{CliError, EngineResult};

#[instrument(skip_all)]
pub fn build_composed_values_yaml(
    charts: &[ChartContext],
    include_subchart_values: bool,
) -> EngineResult<Option<String>> {
    let root = charts.first().ok_or(CliError::NoChartsDiscovered)?;

    let root_values_path = root.chart_dir.join("values.yaml")?;
    let mut doc = if root_values_path.is_file()? {
        serde_yaml::from_str::<YamlValue>(&root_values_path.read_to_string()?)?
    } else {
        YamlValue::Mapping(serde_yaml::Mapping::default())
    };

    if include_subchart_values {
        compose_subchart_values(charts, &mut doc)?;
    }

    let serialized = serde_yaml::to_string(&doc)?;
    if serialized.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(serialized))
    }
}

/// The dependency charts' declared defaults, composed under their value
/// prefixes (with subchart `global` values hoisted like helm does), MINUS
/// every path the parent chart's own values.yaml declares. A key present
/// here fills at the SUBCHART's coalesce stage even when the parent-level
/// document misses it, so schema generation reads absence at such paths
/// as the subchart default instead of nil. Parent-declared keys are
/// excluded because their absence can only mean helm null-deletion, which
/// poisons the key through every later merge stage — the subchart default
/// does NOT resurrect a deleted key.
#[instrument(skip_all)]
pub fn build_dependency_values_yaml(charts: &[ChartContext]) -> EngineResult<Option<String>> {
    let root = charts.first().ok_or(CliError::NoChartsDiscovered)?;
    let mut doc = YamlValue::Mapping(serde_yaml::Mapping::default());
    compose_subchart_values(charts, &mut doc)?;
    let root_values_path = root.chart_dir.join("values.yaml")?;
    if root_values_path.is_file()? {
        let parent = serde_yaml::from_str::<YamlValue>(&root_values_path.read_to_string()?)?;
        doc = subtract_declared_paths(&doc, &parent);
    }
    let serialized = serde_yaml::to_string(&doc)?;
    if serialized.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(serialized))
    }
}

/// `composed` minus every path `parent` declares: keys both documents
/// carry recurse member-wise (a parent `falco-talon: {}` stub keeps the
/// subchart's keys underneath), keys only `composed` carries survive
/// whole, and parent-declared leaves are removed.
fn subtract_declared_paths(composed: &YamlValue, parent: &YamlValue) -> YamlValue {
    match (composed, parent) {
        (YamlValue::Mapping(composed_map), YamlValue::Mapping(parent_map)) => {
            let mut remaining = serde_yaml::Mapping::new();
            for (key, value) in composed_map {
                match parent_map.get(key) {
                    None => {
                        remaining.insert(key.clone(), value.clone());
                    }
                    Some(parent_value) => {
                        let child = subtract_declared_paths(value, parent_value);
                        if !matches!(child, YamlValue::Null) {
                            remaining.insert(key.clone(), child);
                        }
                    }
                }
            }
            if remaining.is_empty() {
                YamlValue::Null
            } else {
                YamlValue::Mapping(remaining)
            }
        }
        _ => YamlValue::Null,
    }
}

#[instrument(skip_all)]
pub fn build_composed_values_descriptions(
    charts: &[ChartContext],
    include_subchart_values: bool,
    values_files: &[PathBuf],
) -> EngineResult<BTreeMap<String, String>> {
    let root = charts.first().ok_or(CliError::NoChartsDiscovered)?;
    let mut descriptions = BTreeMap::new();

    add_values_file_descriptions(&root.chart_dir, &[], &mut descriptions)?;

    if include_subchart_values {
        for chart in charts {
            if chart.values_prefix.is_empty() {
                continue;
            }
            add_values_file_descriptions(
                &chart.chart_dir,
                &chart.values_prefix,
                &mut descriptions,
            )?;
        }
    }

    for path in values_files {
        add_layered_values_file_descriptions(path, &mut descriptions)?;
    }

    Ok(descriptions)
}

fn compose_subchart_values(charts: &[ChartContext], doc: &mut YamlValue) -> EngineResult<()> {
    // Helm hoists each subchart's `global` values into the root `.Values.global`,
    // then exposes that effective global object back inside every subchart.
    // Mirroring the same shape here keeps the composed values document aligned
    // with what `helm lint --strict` validates after Helm's own value merge.
    let mut sub_docs: Vec<(&ChartContext, YamlValue)> = Vec::new();
    for chart in charts {
        if chart.values_prefix.is_empty() {
            continue;
        }

        let path = chart.chart_dir.join("values.yaml")?;
        if !path.is_file()? {
            continue;
        }

        let mut sub_doc: YamlValue = serde_yaml::from_str(&path.read_to_string()?)?;

        if let Some(global_doc) = take_global_key(&mut sub_doc) {
            merge_global_values(doc, global_doc);
        }

        sub_docs.push((chart, sub_doc));
    }

    let empty_global = YamlValue::Mapping(serde_yaml::Mapping::default());
    let root_global = doc
        .as_mapping()
        .and_then(|mapping| mapping.get(YamlValue::String("global".to_string())))
        .cloned()
        .unwrap_or(empty_global);

    for (chart, mut sub_doc) in sub_docs {
        if !matches!(sub_doc, YamlValue::Mapping(_)) {
            sub_doc = YamlValue::Mapping(serde_yaml::Mapping::default());
        }
        if let YamlValue::Mapping(mapping) = &mut sub_doc {
            mapping.insert(YamlValue::String("global".to_string()), root_global.clone());
        }
        merge_values_at_prefix(doc, &chart.values_prefix, sub_doc);
    }

    Ok(())
}

fn take_global_key(doc: &mut YamlValue) -> Option<YamlValue> {
    let YamlValue::Mapping(mapping) = doc else {
        return None;
    };

    mapping.remove(YamlValue::String("global".to_string()))
}

fn merge_global_values(root: &mut YamlValue, global_doc: YamlValue) {
    let target = ensure_mapping_path(root, &["global".to_string()]);

    if matches!(target, YamlValue::Null) {
        *target = YamlValue::Mapping(serde_yaml::Mapping::default());
    }

    let YamlValue::Mapping(mapping) = target else {
        return;
    };

    let YamlValue::Mapping(sub_mapping) = global_doc else {
        return;
    };

    merge_mapping_existing_prefers_left(mapping, sub_mapping);
}

fn add_values_file_descriptions(
    chart_dir: &VfsPath,
    prefix: &[String],
    out: &mut BTreeMap<String, String>,
) -> EngineResult<()> {
    let values_path = chart_dir.join("values.yaml")?;
    if !values_path.is_file()? {
        return Ok(());
    }

    let descriptions = extract_values_yaml_descriptions(&values_path.read_to_string()?);

    for (path, description) in descriptions {
        let scoped_path = scope_values_path(&path, prefix);
        out.entry(scoped_path).or_insert(description);
    }

    Ok(())
}

fn add_layered_values_file_descriptions(
    values_path: &Path,
    out: &mut BTreeMap<String, String>,
) -> EngineResult<()> {
    let source = std::fs::read_to_string(values_path)?;
    let descriptions = extract_values_yaml_descriptions(&source);

    for (path, description) in descriptions {
        out.insert(path, description);
    }

    Ok(())
}

fn merge_values_at_prefix(root: &mut YamlValue, prefix: &[String], sub: YamlValue) {
    let target = ensure_mapping_path(root, prefix);

    if let YamlValue::Mapping(mapping) = target
        && let YamlValue::Mapping(sub_mapping) = sub
    {
        merge_mapping_existing_prefers_left(mapping, sub_mapping);
    }
}

fn ensure_mapping_path<'a>(root: &'a mut YamlValue, path: &[String]) -> &'a mut YamlValue {
    let mut current = root;

    for segment in path {
        if !matches!(current, YamlValue::Mapping(_)) {
            *current = YamlValue::Mapping(serde_yaml::Mapping::default());
        }

        let YamlValue::Mapping(mapping) = current else {
            return current;
        };

        let key = YamlValue::String(segment.clone());
        current = mapping
            .entry(key)
            .or_insert_with(|| YamlValue::Mapping(serde_yaml::Mapping::default()));
    }

    current
}

fn merge_yaml_existing_prefers_left(left: YamlValue, right: YamlValue) -> YamlValue {
    match (left, right) {
        (YamlValue::Mapping(mut left), YamlValue::Mapping(right)) => {
            merge_mapping_existing_prefers_left(&mut left, right);
            YamlValue::Mapping(left)
        }
        (left, _) => left,
    }
}

fn merge_mapping_existing_prefers_left(
    target: &mut serde_yaml::Mapping,
    incoming: serde_yaml::Mapping,
) {
    for (key, value) in incoming {
        if let Some(existing) = target.get(&key).cloned() {
            target.insert(key, merge_yaml_existing_prefers_left(existing, value));
        } else {
            target.insert(key, value);
        }
    }
}

#[cfg(test)]
#[path = "tests/values.rs"]
mod tests;
