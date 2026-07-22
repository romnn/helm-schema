use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::helper_meta::HelperOutputMeta;
use helm_schema_core::Predicate;

use super::SymbolicLocalState;

pub(super) fn joined_branch_outcomes(
    entry: &SymbolicLocalState,
    outcomes: &[SymbolicLocalState],
) -> SymbolicLocalState {
    if outcomes.is_empty() {
        return entry.clone();
    }

    let (fragment_values, traversal_advances) = join_fragment_values(outcomes);
    SymbolicLocalState {
        range_domains: join_map(
            outcomes,
            |state| &state.range_domains,
            |values| Some(join_literal_union(values)),
        ),
        get_bindings: join_map(
            outcomes,
            |state| &state.get_bindings,
            |values| join_if_equal(&values),
        ),
        fragment_values,
        traversal_advances,
        default_paths: join_map(
            outcomes,
            |state| &state.default_paths,
            |values| Some(join_path_union(values)),
        ),
        output_meta: join_map(
            outcomes,
            |state| &state.output_meta,
            |values| Some(join_meta_by_path(values)),
        ),
        truthy_reductions: join_map(
            outcomes,
            |state| &state.truthy_reductions,
            |values| Some(join_predicate_union(values)),
        ),
        typeof_sources: join_map(
            outcomes,
            |state| &state.typeof_sources,
            |values| join_if_equal(&values),
        ),
        int_cast_sources: join_map(
            outcomes,
            |state| &state.int_cast_sources,
            |values| join_if_equal(&values),
        ),
        kube_version_sources: join_map(
            outcomes,
            |state| &state.kube_version_sources,
            |values| join_if_equal(&values),
        ),
        range_member_values: join_map(
            outcomes,
            |state| &state.range_member_values,
            join_value_choice,
        ),
        // A definite entry binding survives a join only where EVERY branch
        // kept the same one: a branch-dependent binding is no longer a
        // certainly-iterated member.
        definite_range_member_values: join_map(
            outcomes,
            |state| &state.definite_range_member_values,
            |values| join_if_equal(&values),
        ),
        chart_value_defaults: intersect_chart_defaults(outcomes),
        local_scopes: entry.local_scopes.clone(),
    }
}

/// Join fragment values, keeping a guarded traversal's ADVANCED value when
/// one branch stepped a local into a member (`$x = index $x $k` under a
/// presence conjunct on the member) and every other branch left it at an
/// ancestor: consumers of the advanced identity are presence-guarded on
/// it, so the join stays a finite exact path instead of a choice.
fn join_fragment_values(
    outcomes: &[SymbolicLocalState],
) -> (HashMap<String, AbstractValue>, BTreeSet<String>) {
    let variables: BTreeSet<&String> = outcomes
        .iter()
        .flat_map(|state| state.fragment_values.keys())
        .collect();
    let mut joined = HashMap::new();
    let mut advances = BTreeSet::new();
    for variable in variables {
        let Some(values) = outcomes
            .iter()
            .map(|state| state.fragment_values.get(variable))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        if let Some(advanced) = advanced_traversal_value(outcomes, variable, &values) {
            joined.insert(variable.clone(), advanced);
            advances.insert(variable.clone());
            continue;
        }
        if let Some(value) = join_value_choice(values) {
            joined.insert(variable.clone(), value);
        }
    }
    (joined, advances)
}

fn advanced_traversal_value(
    outcomes: &[SymbolicLocalState],
    variable: &str,
    values: &[&AbstractValue],
) -> Option<AbstractValue> {
    let paths = values
        .iter()
        .map(|value| match value {
            AbstractValue::ValuesPath(path) => Some(path.as_str()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let deepest = paths
        .iter()
        .copied()
        .max_by_key(|path| helm_schema_core::split_value_path(path).len())?;
    if !paths
        .iter()
        .all(|path| *path == deepest || helm_schema_core::values_path_is_descendant(deepest, path))
    {
        return None;
    }
    let marked = outcomes
        .iter()
        .zip(&paths)
        .any(|(state, path)| *path == deepest && state.traversal_advances.contains(variable));
    marked.then(|| AbstractValue::ValuesPath(deepest.to_string()))
}

/// Join one per-variable local-state map across branch outcomes.
///
/// A variable keeps a joined fact only when every branch outcome still carries
/// one (a branch that dropped or re-domained the variable drops the fact);
/// `join` says how the per-branch values combine into one.
fn join_map<T, F, J>(outcomes: &[SymbolicLocalState], map: F, join: J) -> HashMap<String, T>
where
    F: Fn(&SymbolicLocalState) -> &HashMap<String, T>,
    J: Fn(Vec<&T>) -> Option<T>,
{
    let variables: BTreeSet<&String> = outcomes
        .iter()
        .flat_map(|state| map(state).keys())
        .collect();
    let mut joined = HashMap::new();
    for variable in variables {
        let Some(values) = outcomes
            .iter()
            .map(|state| map(state).get(variable))
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        if let Some(value) = join(values) {
            joined.insert(variable.clone(), value);
        }
    }
    joined
}

fn join_if_equal<T: Clone + Eq>(values: &[&T]) -> Option<T> {
    let (first, rest) = values.split_first()?;
    rest.iter()
        .all(|value| value == first)
        .then(|| (*first).clone())
}

fn join_value_choice(values: Vec<&AbstractValue>) -> Option<AbstractValue> {
    AbstractValue::choice(values.into_iter().cloned().collect())
}

fn join_literal_union(domains: Vec<&Vec<String>>) -> Vec<String> {
    let literals: BTreeSet<&String> = domains.into_iter().flatten().collect();
    literals.into_iter().cloned().collect()
}

fn join_path_union(sets: Vec<&BTreeSet<String>>) -> BTreeSet<String> {
    sets.into_iter().flatten().cloned().collect()
}

fn join_meta_by_path(
    metas: Vec<&BTreeMap<String, HelperOutputMeta>>,
) -> BTreeMap<String, HelperOutputMeta> {
    let mut merged: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for meta_by_path in metas {
        for (path, meta) in meta_by_path {
            merged.entry(path.clone()).or_default().merge(meta);
        }
    }
    merged
}

fn join_predicate_union(predicates: Vec<&Predicate>) -> Predicate {
    let mut alternatives = BTreeSet::new();
    for predicate in predicates {
        match predicate {
            Predicate::True => return Predicate::True,
            Predicate::False => {}
            Predicate::Or(inner) => alternatives.extend(inner.iter().cloned()),
            predicate => {
                alternatives.insert(predicate.clone());
            }
        }
    }
    match alternatives.len() {
        0 => Predicate::False,
        1 => alternatives.pop_first().unwrap_or(Predicate::False),
        _ => Predicate::Or(alternatives.into_iter().collect()),
    }
}

fn intersect_chart_defaults(outcomes: &[SymbolicLocalState]) -> BTreeSet<String> {
    let Some((first, rest)) = outcomes.split_first() else {
        return BTreeSet::new();
    };
    let mut defaults = first.chart_value_defaults.clone();
    for outcome in rest {
        defaults.retain(|path| outcome.chart_value_defaults.contains(path));
    }
    defaults
}
