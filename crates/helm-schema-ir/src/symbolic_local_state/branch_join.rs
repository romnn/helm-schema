use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::helper_meta::HelperOutputMeta;

use super::SymbolicLocalState;

pub(super) fn joined_branch_outcomes(
    entry: &SymbolicLocalState,
    outcomes: Vec<SymbolicLocalState>,
) -> SymbolicLocalState {
    if outcomes.is_empty() {
        return entry.clone();
    }

    SymbolicLocalState {
        range_domains: join_map(&outcomes, |state| &state.range_domains, join_literal_union),
        get_bindings: join_map(&outcomes, |state| &state.get_bindings, join_if_equal),
        fragment_values: join_map(&outcomes, |state| &state.fragment_values, join_value_choice),
        default_paths: join_map(&outcomes, |state| &state.default_paths, join_path_union),
        output_meta: join_map(&outcomes, |state| &state.output_meta, join_meta_by_path),
        typeof_sources: join_map(&outcomes, |state| &state.typeof_sources, join_if_equal),
        range_member_values: join_map(
            &outcomes,
            |state| &state.range_member_values,
            join_value_choice,
        ),
        chart_value_defaults: intersect_chart_defaults(&outcomes),
        local_scopes: entry.local_scopes.clone(),
    }
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

fn join_if_equal<T: Clone + Eq>(values: Vec<&T>) -> Option<T> {
    let (first, rest) = values.split_first()?;
    rest.iter()
        .all(|value| value == first)
        .then(|| (*first).clone())
}

fn join_value_choice(values: Vec<&AbstractValue>) -> Option<AbstractValue> {
    AbstractValue::choice(values.into_iter().cloned().collect())
}

fn join_literal_union(domains: Vec<&Vec<String>>) -> Option<Vec<String>> {
    let literals: BTreeSet<&String> = domains.into_iter().flatten().collect();
    Some(literals.into_iter().cloned().collect())
}

fn join_path_union(sets: Vec<&BTreeSet<String>>) -> Option<BTreeSet<String>> {
    Some(sets.into_iter().flatten().cloned().collect())
}

fn join_meta_by_path(
    metas: Vec<&BTreeMap<String, HelperOutputMeta>>,
) -> Option<BTreeMap<String, HelperOutputMeta>> {
    let mut merged: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
    for meta_by_path in metas {
        for (path, meta) in meta_by_path {
            merged.entry(path.clone()).or_default().merge(meta);
        }
    }
    Some(merged)
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
