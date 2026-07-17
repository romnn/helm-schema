use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::fragment_assignment::AssignmentKind;
use crate::helper_meta::HelperOutputMeta;
use helm_schema_core::Predicate;

mod branch_join;

use branch_join::joined_branch_outcomes;

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolicLocalState {
    pub(crate) range_domains: HashMap<String, Vec<String>>,
    pub(crate) get_bindings: HashMap<String, GetBinding>,
    pub(crate) fragment_values: HashMap<String, AbstractValue>,
    pub(crate) default_paths: HashMap<String, BTreeSet<String>>,
    pub(crate) output_meta: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    /// Sufficient conditions under which a monotone local accumulator is
    /// nonempty. An explicit [`Predicate::False`] is the empty seed; a
    /// missing entry means the local's truthiness is not structurally known.
    pub(crate) truthy_reductions: HashMap<String, Predicate>,
    /// Values paths defaulted by structural `set X "K" (X.K | default V)`
    /// helper mutations that have already run in source order.
    pub(crate) chart_value_defaults: BTreeSet<String>,
    /// Locals bound to a type descriptor. Each described path retains the
    /// predicates under which that path supplied the selected value.
    pub(crate) typeof_sources: HashMap<String, BTreeMap<String, HelperOutputMeta>>,
    /// Range variables bound to the MEMBER identity of a directly ranged
    /// path (`$v` in `range $k, $v := .Values.x` holds each `x.*` value).
    /// Conditions and assignments resolve through these; hole rendering
    /// does not, so member reads do not manufacture placed rows.
    pub(crate) range_member_values: HashMap<String, AbstractValue>,
    /// Locals whose CURRENT value came from a guarded self-advance
    /// (`$x = index $x $k` under a `hasKey $x $k` presence conjunct): one
    /// traversal step into a member. The branch join keeps the advanced
    /// (deepest) value instead of widening to a choice — facts derived
    /// from the advanced identity carry its presence guard, which only
    /// holds at runtime when the advance really happened.
    pub(crate) traversal_advances: BTreeSet<String>,
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
    traversal_advanced: bool,
    default_paths: Option<BTreeSet<String>>,
    output_meta: Option<BTreeMap<String, HelperOutputMeta>>,
    truthy_reduction: Option<Predicate>,
    typeof_source: Option<BTreeMap<String, HelperOutputMeta>>,
    range_member_value: Option<AbstractValue>,
}

impl SymbolicLocalState {
    pub(crate) fn join_branch_outcomes(&mut self, entry: &Self, outcomes: Vec<Self>) {
        *self = joined_branch_outcomes(entry, outcomes);
    }

    /// Conjoin `condition` onto every truthiness reduction this branch
    /// CHANGED relative to `entry`: the reassigned truthiness holds only
    /// where the branch ran, so the cross-branch union becomes the exact
    /// disjunction of guarded arms (the range-sentinel flag pattern).
    ///
    /// Bounded on both sides: an approximate arm condition would only
    /// poison every consumer into abstention, and unbounded conjoining at
    /// nested joins grows reductions combinatorially, so oversized results
    /// keep the old unstamped semantics instead.
    pub(crate) fn conjoin_changed_truthy_reductions(
        &mut self,
        entry: &Self,
        condition: &Predicate,
    ) {
        const MAX_STAMPED_GUARDS: usize = 6;
        if matches!(condition, Predicate::True) || condition.contains_approximation() {
            return;
        }
        let condition_guards = predicate_guard_count(condition);
        for (variable, reduction) in &mut self.truthy_reductions {
            if entry.truthy_reductions.get(variable) == Some(reduction)
                || matches!(reduction, Predicate::False)
                || reduction.contains_approximation()
            {
                continue;
            }
            if condition_guards + predicate_guard_count(reduction) > MAX_STAMPED_GUARDS {
                continue;
            }
            *reduction = Predicate::all(vec![condition.clone(), reduction.clone()]);
        }
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

    pub(crate) fn mark_traversal_advance(&mut self, variable: &str) {
        self.traversal_advances.insert(variable.to_string());
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
            traversal_advanced: self.traversal_advances.contains(variable),
            default_paths: self.default_paths.get(variable).cloned(),
            output_meta: self.output_meta.get(variable).cloned(),
            truthy_reduction: self.truthy_reductions.get(variable).cloned(),
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
            || self.truthy_reductions.contains_key(variable)
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
        if previous.traversal_advanced {
            self.traversal_advances.insert(variable.to_string());
        } else {
            self.traversal_advances.remove(variable);
        }
        restore_map_entry(&mut self.default_paths, variable, previous.default_paths);
        restore_map_entry(&mut self.output_meta, variable, previous.output_meta);
        restore_map_entry(
            &mut self.truthy_reductions,
            variable,
            previous.truthy_reduction,
        );
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
        self.traversal_advances.remove(variable);
        self.default_paths.remove(variable);
        self.output_meta.remove(variable);
        self.truthy_reductions.remove(variable);
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

fn predicate_guard_count(predicate: &Predicate) -> usize {
    match predicate {
        Predicate::True | Predicate::False | Predicate::Approximate { .. } => 0,
        Predicate::Guard(_) => 1,
        Predicate::Not(inner) => predicate_guard_count(inner),
        Predicate::And(items) | Predicate::Or(items) => {
            items.iter().map(predicate_guard_count).sum()
        }
    }
}
