//! Helm v4.2.3 values coalescing for chart directories on disk.
//!
//! Schemas validate the COALESCED values document, so test instances and
//! probe batteries compose over what Helm hands the templates, not over the
//! root `values.yaml` alone.

use std::path::Path;

use color_eyre::eyre::{self, WrapErr as _};
use serde_json::{Map, Value};

/// Coalesce `overrides` over the chart at `chart_dir` the way Helm v4.2.3
/// does before rendering, and return the document a schema validates.
///
/// This ports `ProcessDependencies` and `CoalesceValues`
/// (`pkg/chart/v2/util/dependencies.go`, `pkg/chart/common/util/coalesce.go`):
/// dependencies are enabled by tags, then by the first condition path that
/// holds a bool, and are enabled when neither decides; every chart that
/// declares dependencies then has its defaults replaced by its merged tree;
/// finally the user values are coalesced over that tree. A null override
/// deletes the key, and the deletion survives the dependency stage.
/// Packaged (`.tgz`) dependencies and `import-values` are not modelled.
///
/// # Errors
///
/// Returns an error when a chart manifest or values file cannot be read or
/// parsed.
pub fn coalesce_chart_values(chart_dir: &Path, overrides: Value) -> eyre::Result<Value> {
    let user = match overrides {
        Value::Object(user) => user,
        _ => Map::new(),
    };
    let mut root = load_chart(chart_dir)?;
    process_dependency_enabled(&mut root, &user, &[]);
    process_dependency_values(&mut root);
    let mut values = Value::Object(coalesce(&root, user, false));
    drop_nulls(&mut values);
    Ok(values)
}

struct ChartNode {
    values: Map<String, Value>,
    declares_dependencies: bool,
    dependencies: Vec<DependencyNode>,
}

struct DependencyNode {
    key: String,
    declared: bool,
    conditions: Vec<String>,
    tags: Vec<String>,
    chart: ChartNode,
}

fn load_chart(chart_dir: &Path) -> eyre::Result<ChartNode> {
    let values = match yaml_file(&chart_dir.join("values.yaml"))? {
        Some(Value::Object(values)) => values,
        _ => Map::new(),
    };

    let mut vendored = Vec::new();
    let vendored_dir = chart_dir.join("charts");
    if vendored_dir.is_dir() {
        let mut entries = std::fs::read_dir(&vendored_dir)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        entries.sort();
        for dir in entries {
            let name = match yaml_file(&dir.join("Chart.yaml"))? {
                Some(manifest) => manifest
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                None => None,
            };
            if let Some(name) = name {
                vendored.push((name, dir));
            }
        }
    }

    // An `apiVersion: v1` chart declares its dependencies in `requirements.yaml`.
    let mut declarations = None;
    for manifest in [
        yaml_file(&chart_dir.join("Chart.yaml"))?,
        yaml_file(&chart_dir.join("requirements.yaml"))?,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(Value::Array(entries)) = manifest.get("dependencies") {
            declarations = Some(entries.clone());
        }
    }

    // Helm keeps undeclared vendored charts first, then the declared ones in
    // declaration order.
    let mut dependencies = Vec::new();
    let declared_names: Vec<&str> = declarations
        .iter()
        .flatten()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect();
    for (name, dir) in &vendored {
        if !declared_names.contains(&name.as_str()) {
            dependencies.push(DependencyNode {
                key: name.clone(),
                declared: false,
                conditions: Vec::new(),
                tags: Vec::new(),
                chart: load_chart(dir)?,
            });
        }
    }
    for entry in declarations.iter().flatten() {
        let Some(name) = entry.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some((_, dir)) = vendored.iter().find(|(vendored, _)| vendored == name) else {
            continue;
        };
        let key = entry
            .get("alias")
            .and_then(Value::as_str)
            .unwrap_or(name)
            .to_string();
        let conditions = entry
            .get("condition")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|condition| !condition.is_empty())
            .map(str::to_string)
            .collect();
        let tags = entry
            .get("tags")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        dependencies.push(DependencyNode {
            key,
            declared: true,
            conditions,
            tags,
            chart: load_chart(dir)?,
        });
    }

    Ok(ChartNode {
        values,
        declares_dependencies: declarations.is_some(),
        dependencies,
    })
}

/// Helm's `processDependencyEnabled`: decide each declared dependency
/// against the chart coalesced over `values`, drop the disabled ones, and
/// recurse with that coalesced document.
fn process_dependency_enabled(chart: &mut ChartNode, values: &Map<String, Value>, path: &[String]) {
    if !chart.declares_dependencies {
        return;
    }
    let coalesced = Value::Object(coalesce(chart, values.clone(), false));
    chart
        .dependencies
        .retain(|dependency| !dependency.declared || is_enabled(dependency, &coalesced, path));
    let Value::Object(coalesced) = coalesced else {
        return;
    };
    for dependency in &mut chart.dependencies {
        let mut child_path = path.to_vec();
        child_path.push(dependency.key.clone());
        process_dependency_enabled(&mut dependency.chart, &coalesced, &child_path);
    }
}

/// Tags first, then the first condition path that holds a bool; with
/// neither, the dependency is enabled.
fn is_enabled(dependency: &DependencyNode, coalesced: &Value, path: &[String]) -> bool {
    let mut enabled = true;
    if let Some(Value::Object(tags)) = coalesced.get("tags") {
        let mut has_true = false;
        let mut has_false = false;
        for tag in &dependency.tags {
            match tags.get(tag) {
                Some(Value::Bool(true)) => has_true = true,
                Some(Value::Bool(false)) => has_false = true,
                _ => {}
            }
        }
        enabled = has_true || !has_false;
    }
    for condition in &dependency.conditions {
        let mut value = Some(coalesced);
        for segment in path.iter().map(String::as_str).chain(condition.split('.')) {
            value = value.and_then(|value| value.get(segment));
        }
        if let Some(Value::Bool(condition_value)) = value {
            return *condition_value;
        }
    }
    enabled
}

/// Helm's `processDependencyImportValues` without `import-values`: bottom
/// up, a chart that declares dependencies gets its merged tree as defaults.
fn process_dependency_values(chart: &mut ChartNode) {
    for dependency in &mut chart.dependencies {
        process_dependency_values(&mut dependency.chart);
    }
    if chart.declares_dependencies {
        chart.values = coalesce(chart, Map::new(), true);
    }
}

/// Helm's `coalesce`: the chart's defaults under `dest`, then each
/// dependency's scope with the parent's globals.
fn coalesce(chart: &ChartNode, mut dest: Map<String, Value>, merge: bool) -> Map<String, Value> {
    let dependency_keys: Vec<&str> = chart
        .dependencies
        .iter()
        .map(|dependency| dependency.key.as_str())
        .collect();
    coalesce_values(&mut dest, chart.values.clone(), &dependency_keys, merge);
    for dependency in &chart.dependencies {
        let mut scope = match dest.remove(&dependency.key) {
            None => Map::new(),
            Some(Value::Object(scope)) => scope,
            // Helm aborts on a non-map dependency scope; keep the value.
            Some(other) => {
                dest.insert(dependency.key.clone(), other);
                continue;
            }
        };
        coalesce_globals(&mut scope, dest.get("global"));
        let scope = coalesce(&dependency.chart, scope, merge);
        dest.insert(dependency.key.clone(), Value::Object(scope));
    }
    dest
}

/// Helm's `coalesceValues`: when coalescing, a null user value deletes the
/// key and a default fills a missing key with its own nulls cleaned; a
/// dependency's scope table is always merged, so the parent's nulls reach
/// the dependency stage.
fn coalesce_values(
    dest: &mut Map<String, Value>,
    defaults: Map<String, Value>,
    dependency_keys: &[&str],
    merge: bool,
) {
    for (key, default) in defaults {
        match dest.get_mut(&key) {
            Some(Value::Null) => {
                if !merge {
                    dest.remove(&key);
                }
            }
            Some(Value::Object(user)) => {
                if let Value::Object(mut default) = default {
                    let merge = merge || dependency_keys.contains(&key.as_str());
                    if !merge {
                        clean_nulls(&mut default);
                    }
                    coalesce_tables(user, default, merge);
                }
            }
            Some(_) => {}
            None => {
                let mut default = default;
                if !merge {
                    if default.is_null() {
                        continue;
                    }
                    if let Value::Object(default) = &mut default {
                        clean_nulls(default);
                    }
                }
                dest.insert(key, default);
            }
        }
    }
}

/// Helm's `coalesceTablesFullKey`: `dst` wins; without `merge`, a null in
/// `dst` deletes the key only where `src` held a non-null value.
fn coalesce_tables(dst: &mut Map<String, Value>, mut src: Map<String, Value>, merge: bool) {
    let mut src_non_null = Vec::new();
    for (key, value) in &src {
        if !value.is_null() {
            src_non_null.push(key.clone());
        }
    }
    for (key, value) in dst.iter() {
        if value.is_null() {
            src.insert(key.clone(), Value::Null);
        }
    }
    for (key, value) in src {
        match dst.get_mut(&key) {
            Some(Value::Null) if !merge && src_non_null.contains(&key) => {
                dst.remove(&key);
            }
            None => {
                dst.insert(key, value);
            }
            Some(Value::Object(existing)) => {
                if let Value::Object(value) = value {
                    coalesce_tables(existing, value, merge);
                }
            }
            Some(_) => {}
        }
    }
}

/// Helm's `coalesceGlobals`: the parent's globals win over the dependency's
/// own `global` entries, which fill what the parent does not set.
fn coalesce_globals(scope: &mut Map<String, Value>, parent_global: Option<&Value>) {
    let empty = Map::new();
    let parent_global = match parent_global {
        None => &empty,
        Some(Value::Object(parent_global)) => parent_global,
        // Helm warns and leaves the dependency's globals untouched.
        Some(_) => return,
    };
    let mut global = match scope.remove("global") {
        None => Map::new(),
        Some(Value::Object(global)) => global,
        Some(other) => {
            scope.insert("global".to_string(), other);
            return;
        }
    };
    for (key, parent_value) in parent_global {
        match (parent_value, global.remove(key)) {
            (Value::Object(parent_table), Some(Value::Object(own))) => {
                let mut merged = parent_table.clone();
                coalesce_tables(&mut merged, own, true);
                global.insert(key.clone(), Value::Object(merged));
            }
            (Value::Object(_), Some(own)) | (_, Some(own @ Value::Object(_))) => {
                global.insert(key.clone(), own);
            }
            (parent_value, _) => {
                global.insert(key.clone(), parent_value.clone());
            }
        }
    }
    scope.insert("global".to_string(), Value::Object(global));
}

fn clean_nulls(map: &mut Map<String, Value>) {
    map.retain(|_, value| !value.is_null());
    for value in map.values_mut() {
        if let Value::Object(nested) = value {
            clean_nulls(nested);
        }
    }
}

fn yaml_file(path: &Path) -> eyre::Result<Option<Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let document = serde_yaml::from_str(
        &std::fs::read_to_string(path).wrap_err_with(|| format!("read {}", path.display()))?,
    )
    .wrap_err_with(|| format!("parse {}", path.display()))?;
    Ok(Some(document))
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
