use std::collections::{BTreeMap, BTreeSet};
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
/// prefixes with each parent's effective `global` values propagated into
/// its direct children, MINUS every path the root chart's own values.yaml
/// declares. A key present here fills at the SUBCHART's coalesce stage even
/// when the parent-level document misses it, so schema generation reads
/// absence at such paths as the subchart default instead of nil.
/// Root-declared keys are excluded because their absence can only mean Helm
/// null-deletion, which poisons the key through every later merge stage —
/// the subchart default does NOT resurrect a deleted key.
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

/// The dependency charts' declared defaults, composed under their value
/// prefixes, WITHOUT the parent-declared subtraction: what helm refills a
/// missing or null dependency values root with. `coalesceDeps` recreates
/// the root table and coalesces the subchart's own values into it, and the
/// parent's defaults for that root went with the deletion — so a key the
/// subchart declares comes back while a parent-only key stays gone.
#[instrument(skip_all)]
pub fn build_dependency_refill_values_yaml(
    charts: &[ChartContext],
) -> EngineResult<Option<String>> {
    let mut doc = YamlValue::Mapping(serde_yaml::Mapping::default());
    compose_subchart_values(charts, &mut doc)?;
    let serialized = serde_yaml::to_string(&doc)?;
    if serialized.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(serialized))
    }
}

pub(crate) struct DependencyGlobalOwnership {
    pub(crate) shadowed_input_paths: BTreeSet<String>,
}

pub(crate) fn build_dependency_global_ownership(
    charts: &[ChartContext],
) -> EngineResult<DependencyGlobalOwnership> {
    let mut declarations = Vec::new();
    for chart in charts {
        let values_path = chart.chart_dir.join("values.yaml")?;
        if !values_path.is_file()? {
            continue;
        }
        let defaults = serde_yaml::from_str::<YamlValue>(&values_path.read_to_string()?)?;
        let Some(global) = defaults
            .as_mapping()
            .and_then(|mapping| mapping.get(YamlValue::String("global".to_string())))
            .and_then(YamlValue::as_mapping)
        else {
            continue;
        };
        let mut relative_paths = Vec::new();
        collect_declared_default_paths(global, &mut Vec::new(), &mut relative_paths);
        declarations.push((chart.values_prefix.clone(), relative_paths));
    }

    let mut shadowed_input_paths = BTreeSet::new();
    for (owner_prefix, relative_paths) in declarations {
        for chart in charts.iter().filter(|chart| {
            chart.values_prefix.len() > owner_prefix.len()
                && chart.values_prefix.starts_with(&owner_prefix)
        }) {
            for relative_path in &relative_paths {
                shadowed_input_paths
                    .insert(scoped_global_path(&chart.values_prefix, relative_path));
            }
        }
    }

    Ok(DependencyGlobalOwnership {
        shadowed_input_paths,
    })
}

fn collect_declared_default_paths(
    mapping: &serde_yaml::Mapping,
    prefix: &mut Vec<String>,
    paths: &mut Vec<Vec<String>>,
) {
    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        prefix.push(key.to_string());
        if let YamlValue::Mapping(mapping) = value {
            collect_declared_default_paths(mapping, prefix, paths);
        } else {
            paths.push(prefix.clone());
        }
        prefix.pop();
    }
}

fn scoped_global_path(chart_prefix: &[String], relative_path: &[String]) -> String {
    helm_schema_core::join_value_path(
        chart_prefix
            .iter()
            .cloned()
            .chain(std::iter::once("global".to_string()))
            .chain(relative_path.iter().cloned()),
    )
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
    let mut subcharts = charts
        .iter()
        .filter(|chart| !chart.values_prefix.is_empty())
        .collect::<Vec<_>>();
    subcharts.sort_by(|left, right| {
        left.values_prefix
            .len()
            .cmp(&right.values_prefix.len())
            .then_with(|| left.values_prefix.cmp(&right.values_prefix))
    });

    for chart in subcharts {
        let parent_prefix = chart
            .values_prefix
            .get(..chart.values_prefix.len().saturating_sub(1))
            .unwrap_or_default();
        let parent_global = value_at_path(doc, parent_prefix)
            .and_then(YamlValue::as_mapping)
            .and_then(|mapping| mapping.get(YamlValue::String("global".to_string())))
            .cloned();
        let target = ensure_mapping_path(doc, &chart.values_prefix);
        coalesce_global_values(target, parent_global.as_ref());

        let path = chart.chart_dir.join("values.yaml")?;
        if !path.is_file()? {
            continue;
        }
        let defaults: YamlValue = serde_yaml::from_str(&path.read_to_string()?)?;
        let dependency_keys = direct_dependency_keys(charts, &chart.values_prefix);
        coalesce_chart_values(target, defaults, &dependency_keys);
    }

    Ok(())
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

fn value_at_path<'a>(root: &'a YamlValue, path: &[String]) -> Option<&'a YamlValue> {
    let mut current = root;
    for segment in path {
        current = current
            .as_mapping()?
            .get(YamlValue::String(segment.clone()))?;
    }
    Some(current)
}

fn direct_dependency_keys(
    charts: &[ChartContext],
    parent_prefix: &[String],
) -> std::collections::BTreeSet<String> {
    charts
        .iter()
        .filter_map(|chart| {
            (chart.values_prefix.len() == parent_prefix.len() + 1
                && chart.values_prefix.starts_with(parent_prefix))
            .then(|| chart.values_prefix.last().cloned())
            .flatten()
        })
        .collect()
}

fn coalesce_global_values(destination_chart: &mut YamlValue, source_global: Option<&YamlValue>) {
    let YamlValue::Mapping(destination) = destination_chart else {
        return;
    };
    let global_key = YamlValue::String("global".to_string());
    let mut destination_global = match destination.get(&global_key) {
        None => serde_yaml::Mapping::new(),
        Some(YamlValue::Mapping(mapping)) => mapping.clone(),
        Some(_) => return,
    };
    let source_global = match source_global {
        None => serde_yaml::Mapping::new(),
        Some(YamlValue::Mapping(mapping)) => mapping.clone(),
        Some(_) => return,
    };

    for (key, source_value) in source_global {
        if let YamlValue::Mapping(source_mapping) = source_value {
            match destination_global.get(&key) {
                None => {
                    destination_global.insert(key, YamlValue::Mapping(source_mapping));
                }
                Some(YamlValue::Mapping(destination_mapping)) => {
                    let mut merged = source_mapping;
                    merge_mapping_existing_prefers_left(
                        &mut merged,
                        destination_mapping.clone(),
                        true,
                    );
                    destination_global.insert(key, YamlValue::Mapping(merged));
                }
                Some(_) => {}
            }
        } else if !destination_global
            .get(&key)
            .is_some_and(YamlValue::is_mapping)
        {
            destination_global.insert(key, source_value);
        }
    }

    destination.insert(global_key, YamlValue::Mapping(destination_global));
}

fn coalesce_chart_values(
    destination: &mut YamlValue,
    defaults: YamlValue,
    dependency_keys: &std::collections::BTreeSet<String>,
) {
    let (YamlValue::Mapping(destination), YamlValue::Mapping(defaults)) = (destination, defaults)
    else {
        return;
    };

    for (key, default) in defaults {
        let preserve_nulls = key
            .as_str()
            .is_some_and(|key| dependency_keys.contains(key));
        merge_value_existing_prefers_left(destination, key, default, preserve_nulls);
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

fn merge_mapping_existing_prefers_left(
    target: &mut serde_yaml::Mapping,
    incoming: serde_yaml::Mapping,
    preserve_nulls: bool,
) {
    for (key, value) in incoming {
        merge_value_existing_prefers_left(target, key, value, preserve_nulls);
    }
}

fn merge_value_existing_prefers_left(
    target: &mut serde_yaml::Mapping,
    key: YamlValue,
    incoming: YamlValue,
    preserve_nulls: bool,
) {
    let Some(existing) = target.get(&key).cloned() else {
        target.insert(key, incoming);
        return;
    };
    if existing.is_null() && !preserve_nulls && !incoming.is_null() {
        target.remove(&key);
        return;
    }
    if let (YamlValue::Mapping(mut existing), YamlValue::Mapping(incoming)) = (existing, incoming) {
        merge_mapping_existing_prefers_left(&mut existing, incoming, preserve_nulls);
        target.insert(key, YamlValue::Mapping(existing));
    }
}

#[cfg(test)]
#[path = "tests/values.rs"]
mod tests;
