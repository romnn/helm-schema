//! Helm's value coalescing (`pkg/chart/common/util/coalesce.go`).
//!
//! Every function takes Helm's dotted key `prefix` and records the
//! warnings Helm logs for type conflicts.

use std::collections::BTreeSet;
use std::rc::Rc;

use super::chart::Chart;
use super::go_value::{Node, Table, deep_copy, entries, get, new_table, remove, set};
use super::{ValuesError, Warning};

const GLOBAL: &str = "global";

/// `CoalesceValues`: the chart and its dependencies under a deep copy of
/// `values`, with null overrides deleting the defaults they meet.
pub(super) fn coalesce_values(
    chart: &Chart,
    values: &Table,
    warnings: &mut BTreeSet<Warning>,
) -> Result<Table, ValuesError> {
    let dest = deep_copy(values);
    coalesce(chart, &dest, "", false, warnings)?;
    Ok(dest)
}

/// `MergeValues`: like [`coalesce_values`], but every null is kept.
pub(super) fn merge_values(
    chart: &Chart,
    values: &Table,
    warnings: &mut BTreeSet<Warning>,
) -> Result<Table, ValuesError> {
    let dest = deep_copy(values);
    coalesce(chart, &dest, "", true, warnings)?;
    Ok(dest)
}

fn concat(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

fn coalesce(
    chart: &Chart,
    dest: &Table,
    prefix: &str,
    merge: bool,
    warnings: &mut BTreeSet<Warning>,
) -> Result<(), ValuesError> {
    coalesce_chart_defaults(chart, dest, prefix, merge, warnings);
    let prefix = concat(prefix, &chart.name);
    for (index, dependency) in chart.dependencies.iter().enumerate() {
        // A dependency that writes into its parent's `global` tables (see
        // `coalesce_globals`) changes what the siblings after it see; among
        // siblings in no fixed order, Helm's result then depends on chance.
        let unordered = chart.unordered >= 2 && index < chart.unordered;
        let global_before = unordered.then(|| get(dest, GLOBAL).map(|global| global.to_json()));
        let scope = match get(dest, &dependency.name) {
            None => {
                let scope = new_table();
                set(dest, &dependency.name, Node::Table(Rc::clone(&scope)));
                scope
            }
            Some(Node::Table(scope)) => scope,
            Some(Node::Nil | Node::Leaf(_)) => {
                return Err(ValuesError::NotValidated(format!(
                    "Helm aborts: type mismatch on {}",
                    dependency.name
                )));
            }
        };
        coalesce_globals(&scope, dest, &prefix, warnings);
        coalesce(dependency, &scope, &prefix, merge, warnings)?;
        if let Some(before) = global_before
            && get(dest, GLOBAL).map(|global| global.to_json()) != before
        {
            return Err(ValuesError::Unmodelled(format!(
                "{prefix}: dependency {} writes into the shared global tables, and Helm \
                 coalesces it in random order with its siblings",
                dependency.name
            )));
        }
    }
    Ok(())
}

/// `coalesceGlobals`: the parent's `global` entries win over the
/// dependency's own. A table entry is merged into a SHALLOW copy of the
/// parent's table, so the dependency's nested global tables are written
/// into tables the parent still holds, as in Helm.
fn coalesce_globals(scope: &Table, parent: &Table, prefix: &str, warnings: &mut BTreeSet<Warning>) {
    let own = match get(scope, GLOBAL) {
        None => new_table(),
        Some(Node::Table(own)) => own,
        Some(Node::Nil | Node::Leaf(_)) => {
            warnings.insert(Warning::GlobalsNotATable {
                scope: prefix.to_string(),
            });
            return;
        }
    };
    let inherited = match get(parent, GLOBAL) {
        None => new_table(),
        Some(Node::Table(inherited)) => inherited,
        Some(Node::Nil | Node::Leaf(_)) => {
            warnings.insert(Warning::GlobalsNotATable {
                scope: prefix.to_string(),
            });
            return;
        }
    };
    for (key, value) in entries(&inherited) {
        if let Node::Table(table) = &value {
            let copy: Table = Rc::new(table.borrow().clone().into());
            match get(&own, &key) {
                None => set(&own, &key, Node::Table(copy)),
                Some(Node::Table(existing)) => {
                    coalesce_tables(&copy, &existing, &concat(prefix, &key), true, warnings);
                    set(&own, &key, Node::Table(copy));
                }
                Some(Node::Nil | Node::Leaf(_)) => {
                    warnings.insert(Warning::GlobalMapOntoNonMap { key });
                }
            }
        } else if matches!(get(&own, &key), Some(Node::Table(_))) {
            warnings.insert(Warning::GlobalNonMapOntoMap { key });
        } else {
            set(&own, &key, value);
        }
    }
    set(scope, GLOBAL, Node::Table(own));
}

/// `coalesceValues`: a deep copy of the chart's defaults under `values`.
fn coalesce_chart_defaults(
    chart: &Chart,
    values: &Table,
    prefix: &str,
    merge: bool,
    warnings: &mut BTreeSet<Warning>,
) {
    let prefix = concat(prefix, &chart.name);
    let defaults = deep_copy(&chart.values);
    for (key, default) in entries(&defaults) {
        match get(values, &key) {
            Some(Node::Nil) if !merge => remove(values, &key),
            Some(Node::Table(dest)) => match &default {
                Node::Table(src) => {
                    // A dependency's scope is merged, keeping its nulls for
                    // the dependency's own pass.
                    let merge = merge
                        || chart
                            .dependencies
                            .iter()
                            .any(|dependency| dependency.name == key);
                    if !merge {
                        clean_nil_values(src);
                    }
                    coalesce_tables(&dest, src, &concat(&prefix, &key), merge, warnings);
                }
                Node::Leaf(_) => {
                    warnings.insert(Warning::DefaultNotATable {
                        path: concat(&prefix, &key),
                    });
                }
                Node::Nil => {}
            },
            Some(Node::Nil | Node::Leaf(_)) => {}
            None => {
                if !merge {
                    match &default {
                        Node::Nil => continue,
                        Node::Table(table) => clean_nil_values(table),
                        Node::Leaf(_) => {}
                    }
                }
                set(values, &key, default);
            }
        }
    }
}

/// `coalesceTablesFullKey`: `dst` wins. Without `merge`, a null in `dst`
/// deletes the key only where `src` held a non-null value. Like Helm, this
/// also writes `dst`'s nulls into `src`.
pub(super) fn coalesce_tables(
    dst: &Table,
    src: &Table,
    prefix: &str,
    merge: bool,
    warnings: &mut BTreeSet<Warning>,
) {
    if Rc::ptr_eq(dst, src) {
        return;
    }
    let src_non_nil: BTreeSet<String> = entries(src)
        .into_iter()
        .filter(|(_, value)| !value.is_nil())
        .map(|(key, _)| key)
        .collect();
    for (key, value) in entries(dst) {
        if value.is_nil() {
            set(src, &key, Node::Nil);
        }
    }
    for (key, value) in entries(src) {
        let path = concat(prefix, &key);
        match (get(dst, &key), &value) {
            (Some(Node::Nil), _) if !merge && src_non_nil.contains(&key) => remove(dst, &key),
            (None, _) => set(dst, &key, value),
            (Some(Node::Table(existing)), Node::Table(value)) => {
                coalesce_tables(&existing, value, &path, merge, warnings);
            }
            (Some(Node::Nil | Node::Leaf(_)), Node::Table(_)) => {
                warnings.insert(Warning::TableOverNonTable { path });
            }
            (Some(Node::Table(_)), Node::Leaf(_)) => {
                warnings.insert(Warning::NonTableUnderTable { path });
            }
            (Some(_), _) => {}
        }
    }
}

/// `cleanNilValues`: removes nulls from a table and its nested tables.
fn clean_nil_values(table: &Table) {
    for (key, value) in entries(table) {
        match value {
            Node::Nil => remove(table, &key),
            Node::Table(nested) => clean_nil_values(&nested),
            Node::Leaf(_) => {}
        }
    }
}
