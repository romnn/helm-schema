use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_assignment::AssignmentKind;
use crate::helper_meta::HelperOutputMeta;

mod branch_join;

use branch_join::joined_branch_outcomes;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolicLocalState {
    pub(crate) range_domains: HashMap<String, Vec<String>>,
    pub(crate) get_bindings: HashMap<String, GetBinding>,
    pub(crate) fragment_values: HashMap<String, AbstractValue>,
    pub(crate) default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    /// Values paths defaulted by structural `set X "K" (X.K | default V)`
    /// helper mutations that have already run in source order.
    pub(crate) chart_value_defaults: BTreeSet<String>,
    /// Locals bound to a TYPE DESCRIPTOR of a values path
    /// (`$tp := typeOf .Values.x`): comparing such a local to a literal is
    /// a type test on the path, never a value equality.
    pub(crate) typeof_sources: HashMap<String, String>,
    /// Range variables bound to the MEMBER identity of a directly ranged
    /// path (`$v` in `range $k, $v := .Values.x` holds each `x.*` value).
    /// Conditions and assignments resolve through these; hole rendering
    /// does not, so member reads do not manufacture placed rows.
    pub(crate) range_member_values: HashMap<String, AbstractValue>,
    local_scopes: Vec<LocalScopeFrame>,
}

#[derive(Clone, Debug, Default)]
struct LocalScopeFrame {
    previous_values: HashMap<String, VariableLocalState>,
}

#[derive(Clone, Debug, Default)]
struct VariableLocalState {
    range_domain: Option<Vec<String>>,
    get_binding: Option<GetBinding>,
    fragment_value: Option<AbstractValue>,
    default_paths: Option<BTreeSet<String>>,
    output_meta: Option<BTreeMap<String, HelperOutputMeta>>,
    typeof_source: Option<String>,
    range_member_value: Option<AbstractValue>,
}

impl SymbolicLocalState {
    pub(crate) fn join_branch_outcomes(&mut self, entry: &Self, outcomes: Vec<Self>) {
        *self = joined_branch_outcomes(entry, outcomes);
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
        self.record_binding_shadow(plan.kind, &plan.variable);
        self.set_get_binding(plan.variable, plan.binding);
    }

    pub(crate) fn bind_fragment_value(
        &mut self,
        kind: AssignmentKind,
        variable: String,
        binding: Option<AbstractValue>,
    ) {
        self.record_binding_shadow(kind, &variable);
        self.set_fragment_value(variable, binding);
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
        self.clear_variable(&variable);
        self.range_domains.insert(variable, literals);
    }

    pub(crate) fn set_chart_value_defaults(&mut self, defaults: BTreeSet<String>) {
        self.chart_value_defaults = defaults;
    }

    pub(crate) fn append_chart_value_defaults(&mut self, defaults: &mut BTreeSet<String>) {
        self.chart_value_defaults.append(defaults);
    }

    /// Record the pre-write state of `variable` into the current scope frame
    /// so `exit_local_scope` restores it. `:=` always shadows; `=` writes
    /// through to the existing binding (the write survives scope exit), so it
    /// shadows only when the variable has no current value — Go templates
    /// treat that as a fresh declaration.
    fn record_binding_shadow(&mut self, kind: AssignmentKind, variable: &str) {
        if matches!(kind, AssignmentKind::Assignment) && self.variable_has_current_value(variable) {
            return;
        }
        self.record_scope_shadow(variable);
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
            fragment_value: self.fragment_values.get(variable).cloned(),
            default_paths: self.default_paths.get(variable).cloned(),
            output_meta: self.output_meta.get(variable).cloned(),
            typeof_source: self.typeof_sources.get(variable).cloned(),
            range_member_value: self.range_member_values.get(variable).cloned(),
        }
    }

    fn variable_has_current_value(&self, variable: &str) -> bool {
        self.range_domains.contains_key(variable)
            || self.get_bindings.contains_key(variable)
            || self.fragment_values.contains_key(variable)
            || self.default_paths.contains_key(variable)
            || self.output_meta.contains_key(variable)
            || self.typeof_sources.contains_key(variable)
            || self.range_member_values.contains_key(variable)
    }

    fn set_get_binding(&mut self, variable: String, binding: GetBinding) {
        self.clear_variable(&variable);
        self.get_bindings.insert(variable, binding);
    }

    fn restore_variable_state(&mut self, variable: &str, previous: VariableLocalState) {
        restore_map_entry(&mut self.range_domains, variable, previous.range_domain);
        restore_map_entry(&mut self.get_bindings, variable, previous.get_binding);
        restore_map_entry(&mut self.fragment_values, variable, previous.fragment_value);
        restore_map_entry(&mut self.default_paths, variable, previous.default_paths);
        restore_map_entry(&mut self.output_meta, variable, previous.output_meta);
        restore_map_entry(&mut self.typeof_sources, variable, previous.typeof_source);
        restore_map_entry(
            &mut self.range_member_values,
            variable,
            previous.range_member_value,
        );
    }

    fn set_fragment_value(&mut self, variable: String, binding: Option<AbstractValue>) {
        self.clear_variable(&variable);
        if let Some(binding) = binding {
            self.fragment_values.insert(variable, binding);
        }
    }

    /// Binding a variable in one domain displaces whatever it held in every
    /// other domain (and any stale entry in its own).
    fn clear_variable(&mut self, variable: &str) {
        self.range_domains.remove(variable);
        self.get_bindings.remove(variable);
        self.fragment_values.remove(variable);
        self.default_paths.remove(variable);
        self.output_meta.remove(variable);
        self.typeof_sources.remove(variable);
        self.range_member_values.remove(variable);
    }
}

fn restore_map_entry<T>(map: &mut HashMap<String, T>, variable: &str, value: Option<T>) {
    if let Some(value) = value {
        map.insert(variable.to_string(), value);
    } else {
        map.remove(variable);
    }
}
