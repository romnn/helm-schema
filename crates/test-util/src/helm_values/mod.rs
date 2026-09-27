//! The values documents Helm v4.2.3 validates a chart's schemas against.
//!
//! Helm validates three different documents, and a schema must accept each
//! one Helm validates for an input it renders:
//!
//! - [`AcceptanceDocument::LintRaw`]: `helm lint`'s values rule. The FIRST
//!   YAML document of the root `values.yaml`, under the overrides with
//!   `CoalesceTables`; only the root schema, no dependency defaults.
//! - [`AcceptanceDocument::LintCoalescedTwice`]: `helm lint`'s template
//!   rule. `ProcessDependencies`, `CoalesceValues`, then `CoalesceValues`
//!   AGAIN over that result, so a default a null override deleted in the
//!   first pass is back in the second. The rule reads the overrides after
//!   the values rule has rewritten their nested tables in place.
//! - [`AcceptanceDocument::Template`]: `helm template` / `helm install`.
//!   `ProcessDependencies`, then one `CoalesceValues`.
//!
//! The port keeps Go's map reference semantics (see `go_value`), because
//! the documents above depend on them. An input Helm reads in a way the
//! port does not model is refused with [`ValuesError::Unmodelled`] rather
//! than composed into a silently different document.
//!
//! Overrides are the map Helm's `values.Options.MergeValues` hands the
//! coalescer; [`ValuesOptions`] produces it from `-f` files and `--set`,
//! `--set-string` and `--set-json` flags.

mod chart;
mod coalesce;
mod dependencies;
mod go_value;
mod ignore;
mod options;
mod semver;
mod yaml;

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

pub use options::ValuesOptions;

use chart::Chart;
use go_value::{Node, Table, get, new_table, table_from_json, table_to_json};

/// Which of the documents Helm validates schemas against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum AcceptanceDocument {
    /// `helm lint`'s values rule; checked against the root schema only.
    LintRaw,
    /// `helm lint`'s template rule: the twice-coalesced document.
    LintCoalescedTwice,
    /// `helm template` and `helm install`: the once-coalesced document.
    Template,
}

impl AcceptanceDocument {
    /// Every document kind, in the order Helm's commands validate them.
    pub const ALL: [Self; 3] = [Self::LintRaw, Self::LintCoalescedTwice, Self::Template];
}

/// A document Helm validates, split the way its schema check walks it.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedValues {
    /// The whole document; the root chart's schema validates it.
    pub root: Value,
    /// Each enabled dependency's scope, which that dependency's own schema
    /// validates, including its inherited `global` and nested scopes.
    /// [`AcceptanceDocument::LintRaw`] checks no dependency schema.
    pub dependencies: Vec<DependencyValues>,
    /// The warnings Helm logs while it composes the document, once each.
    pub warnings: BTreeSet<Warning>,
}

/// One dependency scope of a [`ValidatedValues`] document.
#[derive(Clone, Debug, PartialEq)]
pub struct DependencyValues {
    /// The keys from the root to this scope, e.g. `["child", "grandchild"]`.
    pub scope: Vec<String>,
    /// The scope's document.
    pub values: Value,
}

/// A warning Helm logs while composing values; the paths are Helm's dotted
/// keys, which start with the chart names.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Warning {
    /// `warning: skipped value for PATH: Not a table.` An override table
    /// meets a non-table chart default.
    DefaultNotATable {
        /// The key's dotted path.
        path: String,
    },
    /// `warning: cannot overwrite table with non table for PATH`. A table
    /// meets a non-table destination, which is kept.
    TableOverNonTable {
        /// The key's dotted path.
        path: String,
    },
    /// `warning: destination for PATH is a table. Ignoring non-table value`.
    NonTableUnderTable {
        /// The key's dotted path.
        path: String,
    },
    /// `warning: skipping globals because destination/source global is not
    /// a table.`
    GlobalsNotATable {
        /// The chart scope whose dependency's globals are skipped.
        scope: String,
    },
    /// `Conflict: cannot merge map onto non-map for KEY. Skipping.`
    GlobalMapOntoNonMap {
        /// The `global` key.
        key: String,
    },
    /// `key KEY is table. Skipping`
    GlobalNonMapOntoMap {
        /// The `global` key.
        key: String,
    },
    /// `returned non-bool value` for a dependency condition path.
    NonBoolCondition {
        /// The dependency.
        chart: String,
        /// The condition path.
        path: String,
    },
    /// `returned non-bool value` for a dependency tag.
    NonBoolTag {
        /// The dependency.
        chart: String,
        /// The tag.
        tag: String,
    },
    /// `ImportValues missing table`: an `import-values` source is absent.
    MissingImport {
        /// The dependency.
        chart: String,
        /// The missing table's dotted path.
        path: String,
    },
}

/// Why no document was composed.
#[derive(Debug, thiserror::Error)]
pub enum ValuesError {
    /// Helm validates no such document for this input: it aborts first
    /// (a non-map dependency scope, an unloadable chart, a malformed flag)
    /// or the rule does not run (no `values.yaml` for lint's values rule).
    #[error("Helm validates no document: {0}")]
    NotValidated(String),
    /// The input needs a Helm behaviour this port does not model.
    #[error("not modelled: {0}")]
    Unmodelled(String),
    /// A chart or values file could not be read.
    #[error("read {path}")]
    Io {
        /// The file or archive.
        path: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The overrides are not a map.
    #[error("values overrides must be a map, got {0}")]
    Overrides(Value),
}

/// Compose `overrides` over the chart at `chart_dir` into the `document`
/// Helm v4.2.3 validates.
///
/// `overrides` is the map Helm's `values.Options.MergeValues` produces,
/// e.g. from [`ValuesOptions::merge_values`].
///
/// # Errors
///
/// See [`ValuesError`].
pub fn acceptance_values(
    chart_dir: &Path,
    overrides: Value,
    document: AcceptanceDocument,
) -> Result<ValidatedValues, ValuesError> {
    let overrides = match overrides {
        Value::Null => new_table(),
        Value::Object(overrides) => table_from_json(overrides),
        other => return Err(ValuesError::Overrides(other)),
    };
    let mut warnings = BTreeSet::new();
    match document {
        AcceptanceDocument::LintRaw => {
            let root = lint_values_rule(chart_dir, &overrides, &mut warnings)?;
            Ok(ValidatedValues {
                root: Value::Object(table_to_json(&root)),
                dependencies: Vec::new(),
                warnings,
            })
        }
        AcceptanceDocument::LintCoalescedTwice => {
            // The values rule runs first and rewrites the overrides' nested
            // tables in place; the template rule reads what it left.
            match lint_values_rule(chart_dir, &overrides, &mut warnings) {
                Ok(_) | Err(ValuesError::NotValidated(_)) => {}
                Err(error) => return Err(error),
            }
            let mut chart = chart::load_directory(chart_dir)?;
            dependencies::process_dependencies(&mut chart, &overrides, &mut warnings)?;
            let once = coalesce::coalesce_values(&chart, &overrides, &mut warnings)?;
            let twice = coalesce::coalesce_values(&chart, &once, &mut warnings)?;
            validated(&chart, &twice, warnings)
        }
        AcceptanceDocument::Template => {
            let mut chart = chart::load_directory(chart_dir)?;
            check_dependencies(&chart)?;
            dependencies::process_dependencies(&mut chart, &overrides, &mut warnings)?;
            let values = coalesce::coalesce_values(&chart, &overrides, &mut warnings)?;
            validated(&chart, &values, warnings)
        }
    }
}

/// The document `helm template` validates and hands the templates: the
/// root of [`AcceptanceDocument::Template`].
///
/// # Errors
///
/// See [`acceptance_values`].
pub fn coalesce_chart_values(chart_dir: &Path, overrides: Value) -> Result<Value, ValuesError> {
    Ok(acceptance_values(chart_dir, overrides, AcceptanceDocument::Template)?.root)
}

/// Helm's `CoalesceTables(overrides, base)`: `overrides` wins, maps merge,
/// and a null override deletes the key only where `base` holds a non-null
/// value; a null with nothing to delete stays in the document.
#[must_use]
pub fn coalesce_tables(overrides: &Value, base: &Value) -> Value {
    let (Value::Object(overrides), Value::Object(base)) = (overrides, base) else {
        return overrides.clone();
    };
    let dst = table_from_json(overrides.clone());
    let mut warnings = BTreeSet::new();
    coalesce::coalesce_tables(
        &dst,
        &table_from_json(base.clone()),
        "",
        false,
        &mut warnings,
    );
    Value::Object(table_to_json(&dst))
}

/// `helm lint`'s values rule: `CoalesceTables(CoalesceTables({}, overrides),
/// values)`. The first call shares the overrides' nested tables, so the
/// second writes into them.
fn lint_values_rule(
    chart_dir: &Path,
    overrides: &Table,
    warnings: &mut BTreeSet<Warning>,
) -> Result<Table, ValuesError> {
    let path = chart_dir.join("values.yaml");
    if !path.exists() {
        return Err(ValuesError::NotValidated(
            "lint's values rule needs values.yaml".to_string(),
        ));
    }
    let values = chart::read_values_file(&path)?;
    let coalesced = new_table();
    coalesce::coalesce_tables(&coalesced, overrides, "", false, warnings);
    coalesce::coalesce_tables(&coalesced, &table_from_json(values), "", false, warnings);
    Ok(coalesced)
}

/// `action.CheckDependencies`: every root requirement names a loaded chart.
fn check_dependencies(chart: &Chart) -> Result<(), ValuesError> {
    let mut missing = Vec::new();
    for requirement in chart.requirements.iter().flatten() {
        if !chart
            .dependencies
            .iter()
            .any(|dependency| dependency.name == requirement.name)
        {
            missing.push(requirement.name.clone());
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    Err(ValuesError::NotValidated(format!(
        "found in Chart.yaml, but missing in charts/ directory: {}",
        missing.join(", ")
    )))
}

/// `ValidateAgainstSchema`'s walk: the root, then every enabled dependency
/// scope that is present.
fn validated(
    chart: &Chart,
    values: &Table,
    warnings: BTreeSet<Warning>,
) -> Result<ValidatedValues, ValuesError> {
    let mut dependencies = Vec::new();
    dependency_scopes(chart, values, &mut Vec::new(), &mut dependencies)?;
    Ok(ValidatedValues {
        root: Value::Object(table_to_json(values)),
        dependencies,
        warnings,
    })
}

fn dependency_scopes(
    chart: &Chart,
    values: &Table,
    scope: &mut Vec<String>,
    out: &mut Vec<DependencyValues>,
) -> Result<(), ValuesError> {
    for dependency in &chart.dependencies {
        let table = match get(values, &dependency.name) {
            None | Some(Node::Nil) => continue,
            Some(Node::Table(table)) => table,
            Some(Node::Leaf(_)) => {
                return Err(ValuesError::NotValidated(format!(
                    "{}: invalid type for values: expected object",
                    dependency.name
                )));
            }
        };
        scope.push(dependency.name.clone());
        out.push(DependencyValues {
            scope: scope.clone(),
            values: Value::Object(table_to_json(&table)),
        });
        dependency_scopes(dependency, &table, scope, out)?;
        scope.pop();
    }
    Ok(())
}
