//! Helm's `ProcessDependencies` (`pkg/chart/v2/util/dependencies.go`):
//! alias instances, tags and conditions, then `import-values`.

use std::collections::BTreeSet;
use std::rc::Rc;

use serde_json::Value;

use super::chart::{Chart, ImportValue, Requirement};
use super::coalesce::{coalesce_tables, coalesce_values, merge_values};
use super::go_value::{Node, Table, deep_copy, get, new_table, set};
use super::semver::is_compatible_range;
use super::{ValuesError, Warning};

/// `ProcessDependencies`: drops disabled dependencies, then imports child
/// values into their parents bottom-up.
pub(super) fn process_dependencies(
    chart: &mut Chart,
    values: &Table,
    warnings: &mut BTreeSet<Warning>,
) -> Result<(), ValuesError> {
    process_dependency_enabled(chart, values, "", warnings)?;
    process_dependency_import_values(chart, warnings)
}

/// `processDependencyEnabled`.
fn process_dependency_enabled(
    chart: &mut Chart,
    values: &Table,
    path: &str,
    warnings: &mut BTreeSet<Warning>,
) -> Result<(), ValuesError> {
    let Some(mut requirements) = chart.requirements.take() else {
        return Ok(());
    };

    // Loaded charts no requirement claims come first, then one instance per
    // requirement that finds a compatible chart.
    let mut instances = Vec::new();
    'loaded: for existing in &chart.dependencies {
        for requirement in &requirements {
            if existing.name == requirement.name
                && is_compatible_range(&requirement.version, &existing.version)?
            {
                continue 'loaded;
            }
        }
        instances.push(existing.clone());
    }
    let undeclared = instances.len();
    let mut instance_counts = vec![0_usize; chart.dependencies.len()];
    for requirement in &mut requirements {
        let mut found = None;
        for (index, existing) in chart.dependencies.iter().enumerate() {
            if existing.name == requirement.name
                && is_compatible_range(&requirement.version, &existing.version)?
            {
                found = Some(index);
                break;
            }
        }
        if let Some(index) = found
            && let (Some(existing), Some(count)) = (
                chart.dependencies.get(index),
                instance_counts.get_mut(index),
            )
        {
            *count += 1;
            let mut instance = existing.clone();
            if !requirement.alias.is_empty() {
                instance.name = requirement.alias.clone();
            }
            instances.push(instance);
        }
        if !requirement.alias.is_empty() {
            requirement.name = requirement.alias.clone();
        }
    }
    // Helm's instances of one chart share their nested charts' metadata,
    // so processing one instance edits the others.
    for (existing, count) in chart.dependencies.iter().zip(&instance_counts) {
        if *count > 1 && declares_nested_dependencies(existing) {
            return Err(ValuesError::Unmodelled(format!(
                "chart {} is instantiated {count} times and declares dependencies below it",
                existing.name
            )));
        }
    }
    chart.dependencies = instances;
    chart.unordered = undeclared;

    for requirement in &mut requirements {
        requirement.enabled = true;
    }
    let coalesced = coalesce_values(chart, values, warnings)?;
    process_dependency_tags(&mut requirements, &coalesced, warnings);
    process_dependency_conditions(&mut requirements, &coalesced, path, warnings);

    let disabled: BTreeSet<String> = requirements
        .iter()
        .filter(|requirement| !requirement.enabled)
        .map(|requirement| requirement.name.clone())
        .collect();
    chart.unordered = chart
        .dependencies
        .iter()
        .take(undeclared)
        .filter(|dependency| !disabled.contains(&dependency.name))
        .count();
    chart
        .dependencies
        .retain(|dependency| !disabled.contains(&dependency.name));
    requirements.retain(|requirement| !disabled.contains(&requirement.name));

    for dependency in &mut chart.dependencies {
        let path = format!("{path}{}.", dependency.name);
        process_dependency_enabled(dependency, &coalesced, &path, warnings)?;
    }
    chart.requirements = (!requirements.is_empty()).then_some(requirements);
    Ok(())
}

fn declares_nested_dependencies(chart: &Chart) -> bool {
    chart.requirements.is_some() || chart.dependencies.iter().any(declares_nested_dependencies)
}

/// `processDependencyTags`: a dependency is disabled when one of its tags
/// is false and none is true.
fn process_dependency_tags(
    requirements: &mut [Requirement],
    coalesced: &Table,
    warnings: &mut BTreeSet<Warning>,
) {
    let Some(tags) = table_at(coalesced, "tags") else {
        return;
    };
    for requirement in requirements {
        let mut has_true = false;
        let mut has_false = false;
        for tag in &requirement.tags {
            match get(&tags, tag) {
                Some(Node::Leaf(Value::Bool(true))) => has_true = true,
                Some(Node::Leaf(Value::Bool(false))) => has_false = true,
                Some(_) => {
                    warnings.insert(Warning::NonBoolTag {
                        chart: requirement.name.clone(),
                        tag: tag.clone(),
                    });
                }
                None => {}
            }
        }
        requirement.enabled = has_true || !has_false;
    }
}

/// `processDependencyConditions`: the first condition path that holds a
/// bool decides. Helm splits the condition on `,` without trimming the
/// parts.
fn process_dependency_conditions(
    requirements: &mut [Requirement],
    coalesced: &Table,
    path: &str,
    warnings: &mut BTreeSet<Warning>,
) {
    for requirement in requirements {
        for condition in requirement.condition.trim().split(',') {
            if condition.is_empty() {
                continue;
            }
            match path_value(coalesced, &format!("{path}{condition}")) {
                Some(Node::Leaf(Value::Bool(enabled))) => {
                    requirement.enabled = enabled;
                    break;
                }
                Some(_) => {
                    warnings.insert(Warning::NonBoolCondition {
                        chart: requirement.name.clone(),
                        path: condition.to_string(),
                    });
                }
                None => {}
            }
        }
    }
}

/// `processDependencyImportValues`: children first.
fn process_dependency_import_values(
    chart: &mut Chart,
    warnings: &mut BTreeSet<Warning>,
) -> Result<(), ValuesError> {
    for dependency in &mut chart.dependencies {
        process_dependency_import_values(dependency, warnings)?;
    }
    process_import_values(chart, warnings)
}

/// `processImportValues` in merge mode: the chart's defaults become its
/// merged tree, under which the imported child tables sit.
fn process_import_values(
    chart: &mut Chart,
    warnings: &mut BTreeSet<Warning>,
) -> Result<(), ValuesError> {
    let Some(requirements) = &chart.requirements else {
        return Ok(());
    };
    let merged = merge_values(chart, &new_table(), warnings)?;
    let imported = new_table();
    for requirement in requirements {
        for import in &requirement.import_values {
            let (path, parent) = match import {
                ImportValue::Table { child, parent } => {
                    (format!("{}.{child}", requirement.name), Some(parent))
                }
                ImportValue::Exports(name) => {
                    (format!("{}.exports.{name}", requirement.name), None)
                }
            };
            let Some(table) = table_at(&merged, &path) else {
                warnings.insert(Warning::MissingImport {
                    chart: requirement.name.clone(),
                    path,
                });
                continue;
            };
            let table = match parent {
                Some(parent) => path_to_map(parent, table),
                None => table,
            };
            coalesce_tables(&imported, &table, "", true, warnings);
        }
    }
    let values = deep_copy(&merged);
    coalesce_tables(&values, &imported, "", true, warnings);
    chart.values = values;
    Ok(())
}

/// `Values.Table` followed by `AsMap`, which hands out the table itself
/// unless it is empty.
fn table_at(values: &Table, path: &str) -> Option<Table> {
    let mut table = Rc::clone(values);
    for segment in path.split('.') {
        let Some(Node::Table(nested)) = get(&table, segment) else {
            return None;
        };
        table = nested;
    }
    if table.borrow().is_empty() {
        return Some(new_table());
    }
    Some(table)
}

/// `Values.PathValue`: a non-table value at a dotted path.
fn path_value(values: &Table, path: &str) -> Option<Node> {
    let (parent, key) = match path.rsplit_once('.') {
        Some((parent, key)) => (table_at(values, parent)?, key),
        None => (Rc::clone(values), path),
    };
    match get(&parent, key) {
        Some(Node::Table(_)) | None => None,
        Some(value) => Some(value),
    }
}

/// `pathToMap`: `data` nested under a dotted path; `.` is the root.
fn path_to_map(path: &str, data: Table) -> Table {
    if path == "." {
        return data;
    }
    let mut current = data;
    for segment in path.split('.').rev() {
        let wrapper = new_table();
        set(&wrapper, segment, Node::Table(current));
        current = wrapper;
    }
    current
}
