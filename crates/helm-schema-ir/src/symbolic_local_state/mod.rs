use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::binding::FragmentBinding;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_assignment::AssignmentKind;
use crate::helper_analysis::HelperOutputMeta;

mod branch_join;

use branch_join::joined_branch_outcomes;

#[cfg(test)]
mod tests;

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
        *self = joined_branch_outcomes(&entry.state, outcomes);
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
