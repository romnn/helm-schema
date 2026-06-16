use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::fragment_binding::FragmentBinding;
use crate::helper_summary::HelperOutputMeta;

use super::SymbolicLocalState;

pub(super) fn joined_branch_outcomes(
    entry: &SymbolicLocalState,
    outcomes: Vec<SymbolicLocalState>,
) -> SymbolicLocalState {
    if outcomes.is_empty() {
        return entry.clone();
    }

    let mut joined = entry.clone();
    joined.range_domains = join_range_domains(&outcomes);
    joined.get_bindings = join_eq_map(&outcomes, |state| &state.get_bindings);
    joined.fragment_bindings = join_fragment_bindings(&outcomes);
    joined.default_paths = join_set_maps(&outcomes, |state| &state.default_paths);
    joined.output_meta = join_output_meta(&outcomes);
    joined.chart_value_defaults = intersect_chart_defaults(&outcomes);
    joined.local_scopes = entry.local_scopes.clone();
    joined
}

fn outcome_variable_names<T, F>(outcomes: &[SymbolicLocalState], map: F) -> BTreeSet<String>
where
    F: Fn(&SymbolicLocalState) -> &HashMap<String, T>,
{
    outcomes
        .iter()
        .flat_map(|state| map(state).keys().cloned())
        .collect()
}

fn join_eq_map<T, F>(outcomes: &[SymbolicLocalState], map: F) -> HashMap<String, T>
where
    T: Clone + Eq,
    F: Fn(&SymbolicLocalState) -> &HashMap<String, T>,
{
    let mut joined = HashMap::new();
    for variable in outcome_variable_names(outcomes, &map) {
        let Some(first) = map(&outcomes[0]).get(&variable) else {
            continue;
        };
        if outcomes
            .iter()
            .skip(1)
            .all(|state| map(state).get(&variable) == Some(first))
        {
            joined.insert(variable, first.clone());
        }
    }
    joined
}

fn join_range_domains(outcomes: &[SymbolicLocalState]) -> HashMap<String, Vec<String>> {
    let mut joined = HashMap::new();
    for variable in outcome_variable_names(outcomes, |state| &state.range_domains) {
        let mut literals = BTreeSet::new();
        let mut present_in_all_outcomes = true;
        for outcome in outcomes {
            let Some(domain) = outcome.range_domains.get(&variable) else {
                present_in_all_outcomes = false;
                break;
            };
            literals.extend(domain.iter().cloned());
        }
        if present_in_all_outcomes {
            joined.insert(variable, literals.into_iter().collect());
        }
    }
    joined
}

fn join_fragment_bindings(outcomes: &[SymbolicLocalState]) -> HashMap<String, FragmentBinding> {
    let mut joined = HashMap::new();
    for variable in outcome_variable_names(outcomes, |state| &state.fragment_bindings) {
        let mut bindings = Vec::new();
        let mut present_in_all_outcomes = true;
        for outcome in outcomes {
            let Some(binding) = outcome.fragment_bindings.get(&variable) else {
                present_in_all_outcomes = false;
                break;
            };
            bindings.push(binding.clone());
        }
        if present_in_all_outcomes && let Some(binding) = FragmentBinding::choice(bindings) {
            joined.insert(variable, binding);
        }
    }
    joined
}

fn join_set_maps<F>(outcomes: &[SymbolicLocalState], map: F) -> HashMap<String, BTreeSet<String>>
where
    F: Fn(&SymbolicLocalState) -> &HashMap<String, BTreeSet<String>>,
{
    let mut joined = HashMap::new();
    for variable in outcome_variable_names(outcomes, &map) {
        let mut values = BTreeSet::new();
        let mut present_in_all_outcomes = true;
        for outcome in outcomes {
            let Some(paths) = map(outcome).get(&variable) else {
                present_in_all_outcomes = false;
                break;
            };
            values.extend(paths.iter().cloned());
        }
        if present_in_all_outcomes {
            joined.insert(variable, values);
        }
    }
    joined
}

fn join_output_meta(
    outcomes: &[SymbolicLocalState],
) -> HashMap<String, BTreeMap<String, HelperOutputMeta>> {
    let mut joined = HashMap::new();
    for variable in outcome_variable_names(outcomes, |state| &state.output_meta) {
        let mut merged: BTreeMap<String, HelperOutputMeta> = BTreeMap::new();
        let mut present_in_all_outcomes = true;
        for outcome in outcomes {
            let Some(meta_by_path) = outcome.output_meta.get(&variable) else {
                present_in_all_outcomes = false;
                break;
            };
            for (path, meta) in meta_by_path {
                merged.entry(path.clone()).or_default().merge_ref(meta);
            }
        }
        if present_in_all_outcomes {
            joined.insert(variable, merged);
        }
    }
    joined
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
