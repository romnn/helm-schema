use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::binding::FragmentBinding;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_scope_eval::AssignmentKind;
use crate::helper_analysis::HelperOutputMeta;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolicLocalState {
    pub(crate) range_domains: HashMap<String, Vec<String>>,
    pub(crate) get_bindings: HashMap<String, GetBinding>,
    pub(crate) fragment_bindings: HashMap<String, FragmentBinding>,
    pub(crate) default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    /// Values paths defaulted by structural `set X "K" (X.K | default V)`
    /// helper mutations that have already run in source order.
    pub(crate) chart_value_defaults: BTreeSet<String>,
    local_scopes: Vec<LocalScopeFrame>,
}

#[derive(Clone, Debug)]
pub(crate) struct SymbolicLocalStateSnapshot {
    state: SymbolicLocalState,
}

#[derive(Clone, Debug, Default)]
struct LocalScopeFrame {
    previous_values: HashMap<String, VariableLocalState>,
}

#[derive(Clone, Debug, Default)]
struct VariableLocalState {
    range_domain: Option<Vec<String>>,
    get_binding: Option<GetBinding>,
    fragment_binding: Option<FragmentBinding>,
    default_paths: Option<BTreeSet<String>>,
    output_meta: Option<BTreeMap<String, HelperOutputMeta>>,
}

impl SymbolicLocalState {
    pub(crate) fn snapshot(&self) -> SymbolicLocalStateSnapshot {
        SymbolicLocalStateSnapshot {
            state: self.clone(),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: SymbolicLocalStateSnapshot) {
        *self = snapshot.state;
    }

    pub(crate) fn join_branch_outcomes(
        &mut self,
        entry: &SymbolicLocalStateSnapshot,
        outcomes: Vec<SymbolicLocalStateSnapshot>,
    ) {
        let outcomes = outcomes
            .into_iter()
            .map(|snapshot| snapshot.state)
            .collect();
        *self = Self::joined_branch_outcomes(&entry.state, outcomes);
    }

    pub(crate) fn enter_local_scope(&mut self) {
        self.local_scopes.push(LocalScopeFrame::default());
    }

    pub(crate) fn exit_local_scope(&mut self) {
        let Some(scope) = self.local_scopes.pop() else {
            return;
        };
        for (variable, previous) in scope.previous_values {
            self.restore_variable_state(&variable, previous);
        }
    }

    pub(crate) fn apply_get_binding(&mut self, plan: GetBindingPlan) {
        match plan.kind {
            AssignmentKind::Declaration => self.declare_get_binding(plan.variable, plan.binding),
            AssignmentKind::Assignment => self.assign_get_binding(plan.variable, plan.binding),
        }
    }

    pub(crate) fn declare_fragment_binding(
        &mut self,
        variable: String,
        binding: Option<FragmentBinding>,
    ) {
        self.record_scope_shadow(&variable);
        self.range_domains.remove(&variable);
        self.get_bindings.remove(&variable);
        self.set_fragment_binding(variable, binding);
    }

    pub(crate) fn assign_fragment_binding(
        &mut self,
        variable: String,
        binding: Option<FragmentBinding>,
    ) {
        if self.local_scopes.is_empty() || self.variable_has_current_value(&variable) {
            self.set_fragment_binding(variable, binding);
        } else {
            self.declare_fragment_binding(variable, binding);
        }
    }

    pub(crate) fn set_default_paths(&mut self, variable: &str, paths: BTreeSet<String>) {
        if paths.is_empty() {
            self.default_paths.remove(variable);
        } else {
            self.default_paths.insert(variable.to_string(), paths);
        }
    }

    pub(crate) fn set_output_meta(
        &mut self,
        variable: String,
        meta: BTreeMap<String, HelperOutputMeta>,
    ) {
        if meta.is_empty() {
            self.output_meta.remove(&variable);
        } else {
            self.output_meta.insert(variable, meta);
        }
    }

    pub(crate) fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.record_scope_shadow(&variable);
        self.get_bindings.remove(&variable);
        self.fragment_bindings.remove(&variable);
        self.default_paths.remove(&variable);
        self.output_meta.remove(&variable);
        self.range_domains.insert(variable, literals);
    }

    pub(crate) fn set_chart_value_defaults(&mut self, defaults: BTreeSet<String>) {
        self.chart_value_defaults = defaults;
    }

    pub(crate) fn append_chart_value_defaults(&mut self, defaults: &mut BTreeSet<String>) {
        self.chart_value_defaults.append(defaults);
    }

    fn joined_branch_outcomes(entry: &Self, outcomes: Vec<Self>) -> Self {
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

    fn record_scope_shadow(&mut self, variable: &str) {
        let previous = self.variable_state(variable);
        let Some(scope) = self.local_scopes.last_mut() else {
            return;
        };
        scope
            .previous_values
            .entry(variable.to_string())
            .or_insert(previous);
    }

    fn variable_state(&self, variable: &str) -> VariableLocalState {
        VariableLocalState {
            range_domain: self.range_domains.get(variable).cloned(),
            get_binding: self.get_bindings.get(variable).cloned(),
            fragment_binding: self.fragment_bindings.get(variable).cloned(),
            default_paths: self.default_paths.get(variable).cloned(),
            output_meta: self.output_meta.get(variable).cloned(),
        }
    }

    fn variable_has_current_value(&self, variable: &str) -> bool {
        self.range_domains.contains_key(variable)
            || self.get_bindings.contains_key(variable)
            || self.fragment_bindings.contains_key(variable)
            || self.default_paths.contains_key(variable)
            || self.output_meta.contains_key(variable)
    }

    fn declare_get_binding(&mut self, variable: String, binding: GetBinding) {
        self.record_scope_shadow(&variable);
        self.range_domains.remove(&variable);
        self.fragment_bindings.remove(&variable);
        self.default_paths.remove(&variable);
        self.output_meta.remove(&variable);
        self.get_bindings.insert(variable, binding);
    }

    fn assign_get_binding(&mut self, variable: String, binding: GetBinding) {
        if self.local_scopes.is_empty() || self.variable_has_current_value(&variable) {
            self.range_domains.remove(&variable);
            self.fragment_bindings.remove(&variable);
            self.default_paths.remove(&variable);
            self.output_meta.remove(&variable);
            self.get_bindings.insert(variable, binding);
        } else {
            self.declare_get_binding(variable, binding);
        }
    }

    fn restore_variable_state(&mut self, variable: &str, previous: VariableLocalState) {
        restore_map_entry(&mut self.range_domains, variable, previous.range_domain);
        restore_map_entry(&mut self.get_bindings, variable, previous.get_binding);
        restore_map_entry(
            &mut self.fragment_bindings,
            variable,
            previous.fragment_binding,
        );
        restore_map_entry(&mut self.default_paths, variable, previous.default_paths);
        restore_map_entry(&mut self.output_meta, variable, previous.output_meta);
    }

    fn set_fragment_binding(&mut self, variable: String, binding: Option<FragmentBinding>) {
        self.range_domains.remove(&variable);
        self.get_bindings.remove(&variable);
        self.default_paths.remove(&variable);
        self.output_meta.remove(&variable);
        if let Some(binding) = binding {
            self.fragment_bindings.insert(variable, binding);
        } else {
            self.fragment_bindings.remove(&variable);
        }
    }
}

fn restore_map_entry<T>(map: &mut HashMap<String, T>, variable: &str, value: Option<T>) {
    if let Some(value) = value {
        map.insert(variable.to_string(), value);
    } else {
        map.remove(variable);
    }
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
        let mut merged = BTreeMap::new();
        let mut present_in_all_outcomes = true;
        for outcome in outcomes {
            let Some(meta_by_path) = outcome.output_meta.get(&variable) else {
                present_in_all_outcomes = false;
                break;
            };
            for (path, meta) in meta_by_path {
                let entry: &mut HelperOutputMeta = merged.entry(path.clone()).or_default();
                entry.guards.extend(meta.guards.iter().cloned());
                entry.defaulted |= meta.defaulted;
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::SymbolicLocalState;
    use crate::binding::FragmentBinding;
    use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
    use crate::fragment_scope_eval::AssignmentKind;
    use crate::helper_analysis::HelperOutputMeta;

    #[test]
    fn snapshot_restore_replaces_all_local_state_maps() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "image".to_string(),
            Some(FragmentBinding::ValuesPath("image".to_string())),
        );
        state
            .chart_value_defaults
            .insert("serviceAccount.name".to_string());
        let snapshot = state.snapshot();
        state.enter_local_scope();

        state.declare_fragment_binding(
            "image".to_string(),
            Some(FragmentBinding::ValuesPath("otherImage".to_string())),
        );
        state.insert_range_domain("key".to_string(), vec!["a".to_string()]);
        state
            .chart_value_defaults
            .insert("serviceAccount.labels".to_string());
        state.exit_local_scope();

        state.restore(snapshot);

        assert_eq!(
            state.fragment_bindings.get("image"),
            Some(&FragmentBinding::ValuesPath("image".to_string()))
        );
        assert!(state.range_domains.is_empty());
        assert_eq!(
            state.chart_value_defaults,
            ["serviceAccount.name".to_string()].into_iter().collect()
        );
    }

    #[test]
    fn local_scope_restores_shadowed_fragment_binding() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );

        state.enter_local_scope();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("inner".to_string())),
        );
        state.exit_local_scope();

        assert_eq!(
            state.fragment_bindings.get("name"),
            Some(&FragmentBinding::ValuesPath("outer".to_string()))
        );
    }

    #[test]
    fn local_scope_keeps_assignment_to_outer_fragment_binding() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );

        state.enter_local_scope();
        state.assign_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("assigned".to_string())),
        );
        state.exit_local_scope();

        assert_eq!(
            state.fragment_bindings.get("name"),
            Some(&FragmentBinding::ValuesPath("assigned".to_string()))
        );
    }

    #[test]
    fn local_scope_restores_shadowed_get_binding() {
        let mut state = SymbolicLocalState::default();
        state.apply_get_binding(get_binding_plan(
            "value",
            AssignmentKind::Declaration,
            "outer",
            "key",
        ));

        state.enter_local_scope();
        state.apply_get_binding(get_binding_plan(
            "value",
            AssignmentKind::Declaration,
            "inner",
            "key",
        ));
        state.exit_local_scope();

        assert_eq!(
            state.get_bindings.get("value"),
            Some(&get_binding("outer", "key"))
        );
    }

    #[test]
    fn local_scope_keeps_assignment_to_outer_get_binding() {
        let mut state = SymbolicLocalState::default();
        state.apply_get_binding(get_binding_plan(
            "value",
            AssignmentKind::Declaration,
            "outer",
            "key",
        ));

        state.enter_local_scope();
        state.apply_get_binding(get_binding_plan(
            "value",
            AssignmentKind::Assignment,
            "assigned",
            "key",
        ));
        state.exit_local_scope();

        assert_eq!(
            state.get_bindings.get("value"),
            Some(&get_binding("assigned", "key"))
        );
    }

    #[test]
    fn fragment_assignment_replaces_outer_get_binding() {
        let mut state = SymbolicLocalState::default();
        state.apply_get_binding(get_binding_plan(
            "value",
            AssignmentKind::Declaration,
            "outer",
            "key",
        ));

        state.enter_local_scope();
        state.assign_fragment_binding(
            "value".to_string(),
            Some(FragmentBinding::ValuesPath("assigned".to_string())),
        );
        state.exit_local_scope();

        assert!(!state.get_bindings.contains_key("value"));
        assert_eq!(
            state.fragment_bindings.get("value"),
            Some(&FragmentBinding::ValuesPath("assigned".to_string()))
        );
    }

    #[test]
    fn local_scope_restores_range_domain_shadowing_outer_binding() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "key".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );

        state.enter_local_scope();
        state.insert_range_domain("key".to_string(), vec!["inner".to_string()]);
        state.exit_local_scope();

        assert!(!state.range_domains.contains_key("key"));
        assert_eq!(
            state.fragment_bindings.get("key"),
            Some(&FragmentBinding::ValuesPath("outer".to_string()))
        );
    }

    #[test]
    fn local_scope_restores_default_paths_for_shadowed_declaration() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );
        state.set_default_paths("name", BTreeSet::from(["outer.default".to_string()]));

        state.enter_local_scope();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("inner".to_string())),
        );
        state.set_default_paths("name", BTreeSet::from(["inner.default".to_string()]));
        state.exit_local_scope();

        assert_eq!(
            state.default_paths.get("name"),
            Some(&BTreeSet::from(["outer.default".to_string()]))
        );
    }

    #[test]
    fn local_scope_keeps_default_paths_for_outer_assignment() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );
        state.set_default_paths("name", BTreeSet::from(["outer.default".to_string()]));

        state.enter_local_scope();
        state.assign_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("assigned".to_string())),
        );
        state.set_default_paths("name", BTreeSet::from(["assigned.default".to_string()]));
        state.exit_local_scope();

        assert_eq!(
            state.default_paths.get("name"),
            Some(&BTreeSet::from(["assigned.default".to_string()]))
        );
    }

    #[test]
    fn local_scope_restores_output_meta_for_shadowed_declaration() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );
        state.set_output_meta("name".to_string(), output_meta("outer.output"));

        state.enter_local_scope();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("inner".to_string())),
        );
        state.set_output_meta("name".to_string(), output_meta("inner.output"));
        state.exit_local_scope();

        assert_eq!(
            state.output_meta.get("name"),
            Some(&output_meta("outer.output"))
        );
    }

    #[test]
    fn local_scope_keeps_output_meta_for_outer_assignment() {
        let mut state = SymbolicLocalState::default();
        state.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("outer".to_string())),
        );
        state.set_output_meta("name".to_string(), output_meta("outer.output"));

        state.enter_local_scope();
        state.assign_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("assigned".to_string())),
        );
        state.set_output_meta("name".to_string(), output_meta("assigned.output"));
        state.exit_local_scope();

        assert_eq!(
            state.output_meta.get("name"),
            Some(&output_meta("assigned.output"))
        );
    }

    #[test]
    fn branch_join_keeps_bindings_present_in_all_outcomes() {
        let mut entry = SymbolicLocalState::default();
        entry.declare_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("entry".to_string())),
        );
        let entry_snapshot = entry.snapshot();

        let mut first = entry.clone();
        first.assign_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("first".to_string())),
        );
        let mut second = entry.clone();
        second.assign_fragment_binding(
            "name".to_string(),
            Some(FragmentBinding::ValuesPath("second".to_string())),
        );

        let mut joined = entry;
        joined.join_branch_outcomes(&entry_snapshot, vec![first.snapshot(), second.snapshot()]);

        assert_eq!(
            joined.fragment_bindings.get("name"),
            Some(&FragmentBinding::Choice(
                [
                    FragmentBinding::ValuesPath("first".to_string()),
                    FragmentBinding::ValuesPath("second".to_string())
                ]
                .into_iter()
                .collect()
            ))
        );
    }

    #[test]
    fn branch_join_intersects_chart_value_defaults() {
        let mut entry = SymbolicLocalState::default();
        entry
            .chart_value_defaults
            .insert("already.defaulted".to_string());
        let entry_snapshot = entry.snapshot();

        let mut first = entry.clone();
        first.chart_value_defaults.insert("branch.only".to_string());
        let second = entry.clone();

        let mut joined = entry;
        joined.join_branch_outcomes(&entry_snapshot, vec![first.snapshot(), second.snapshot()]);

        assert_eq!(
            joined.chart_value_defaults,
            ["already.defaulted".to_string()].into_iter().collect()
        );
    }

    fn get_binding_plan(
        variable: &str,
        kind: AssignmentKind,
        base: &str,
        key_var: &str,
    ) -> GetBindingPlan {
        GetBindingPlan {
            variable: variable.to_string(),
            kind,
            binding: get_binding(base, key_var),
        }
    }

    fn get_binding(base: &str, key_var: &str) -> GetBinding {
        GetBinding {
            base: base.to_string(),
            key_var: key_var.to_string(),
        }
    }

    fn output_meta(path: &str) -> BTreeMap<String, HelperOutputMeta> {
        BTreeMap::from([(path.to_string(), HelperOutputMeta::default())])
    }
}
