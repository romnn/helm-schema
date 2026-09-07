use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{GetBinding, GetBindingPlan};
use crate::eval_env::{BindingEvaluationMode, BindingValueMetadata, LocalBinding};
use crate::fragment_assignment::AssignmentKind;
use crate::helper_meta::HelperOutputMeta;
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use helm_schema_core::{Guard, Predicate, PredicateMemo};

mod branch_join;

use branch_join::{
    joined_binding_decisions, joined_branch_outcomes, joined_scalar_dispatch_arms,
    joined_truthy_reduction_arms,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct SymbolicLocalState {
    pub(crate) range_domains: HashMap<String, Vec<String>>,
    pub(crate) get_bindings: HashMap<String, GetBinding>,
    pub(crate) fragment_values: HashMap<String, LocalBinding>,
    pub(crate) default_paths: HashMap<String, BTreeSet<helm_schema_core::ValuesPath>>,
    pub(crate) output_meta:
        HashMap<String, BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Runtime scalar values retained independently of fragment provenance.
    /// Branch joins guard each alternative by the branch that assigned it,
    /// so later helper conditions consume the same value semantics Helm does.
    pub(crate) scalar_dispatches: HashMap<String, ScalarValueDispatch>,
    /// Sufficient conditions under which a monotone local accumulator is
    /// nonempty. An explicit [`Predicate::False`] is the empty seed; a
    /// missing entry means the local's truthiness is not structurally known.
    pub(crate) truthy_reductions: HashMap<String, Predicate>,
    /// Locals whose branch predicate exceeded the bounded stamping budget.
    /// Their truthiness must abstain instead of falling back to the local's
    /// unguarded value paths.
    pub(crate) truthiness_abstentions: BTreeSet<String>,
    /// Locals that received a reassignment which does not imply truthiness
    /// (an explicit falsy write, or a condition-valued write). Such a write
    /// makes the accumulator last-write-wins instead of monotone, so a
    /// range exit must drop the local's reduction rather than read it
    /// existentially. Straight-line `if`/`else` joins stay exact and keep
    /// consuming the reduction. A declaration seeds a fresh accumulator and
    /// resets the mark.
    pub(crate) truthiness_clears: BTreeSet<String>,
    /// Values paths defaulted by structural `set X "K" (X.K | default V)`
    /// helper mutations that have already run in source order.
    pub(crate) chart_value_defaults: BTreeSet<helm_schema_core::ValuesPath>,
    /// Locals bound to a type descriptor. Each described path retains the
    /// predicates under which that path supplied the selected value.
    pub(crate) typeof_sources:
        HashMap<String, BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    /// Locals bound to a total integer cast of one values path
    /// (`$replicas := int (default 1 .Values.controller.replicas)`):
    /// comparisons on the local may strengthen through the raw-integer
    /// sound subsets exactly as the inline cast expression would.
    pub(crate) int_cast_sources: HashMap<String, IntCastSource>,
    /// Range variables bound to the MEMBER identity of a directly ranged
    /// path (`$v` in `range $k, $v := .Values.x` holds each `x.*` value).
    /// Conditions and assignments resolve through these; hole rendering
    /// does not, so member reads do not manufacture placed rows.
    pub(crate) range_member_values: HashMap<String, AbstractValue>,
    /// For a range variable bound over a local-dict OVERLAY (`$services :=
    /// .Values.service.additionalServices` + `set $services "default"
    /// (omit …)`): one literal entry the range DEFINITELY iterates on
    /// every render. Unfaithful member conditions re-decode under this
    /// binding as a sound subset (positive-polarity consumers only).
    pub(crate) definite_range_member_values: HashMap<String, AbstractValue>,
    local_scopes: Vec<LocalScopeFrame>,
}

/// The values path (and optional literal-integer `default` fallback) behind
/// a local's `int`/`int64` cast binding. The fallback matters for
/// soundness: a raw `0` at the path is numerically empty, so `default`
/// substitutes the literal before the comparison runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IntCastSource {
    pub(crate) path: helm_schema_core::ValuesPath,
    pub(crate) default_int: Option<i64>,
}

#[derive(Clone, Debug, Default)]
struct LocalScopeFrame {
    previous_values: HashMap<String, VariableLocalState>,
}

#[derive(Clone, Debug, Default)]
struct VariableLocalState {
    range_domain: Option<Vec<String>>,
    get_binding: Option<GetBinding>,
    fragment_value: Option<LocalBinding>,
    default_paths: Option<BTreeSet<helm_schema_core::ValuesPath>>,
    output_meta: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    scalar_dispatch: Option<ScalarValueDispatch>,
    truthy_reduction: Option<Predicate>,
    truthiness_abstained: bool,
    truthiness_cleared: bool,
    typeof_source: Option<BTreeMap<helm_schema_core::ValuesPath, HelperOutputMeta>>,
    int_cast_source: Option<IntCastSource>,
    range_member_value: Option<AbstractValue>,
    definite_range_member_value: Option<AbstractValue>,
}

/// One complete local-state exit selected by a structural control condition.
#[derive(Clone, Debug)]
pub(crate) struct ControlOutcome {
    condition: Predicate,
    truth: Option<TruthCondition>,
    state: SymbolicLocalState,
}

impl ControlOutcome {
    pub(crate) fn new(truth: TruthCondition, state: SymbolicLocalState) -> Self {
        let condition = truth
            .predicate()
            .cloned()
            .unwrap_or_else(|| truth.when_true());
        Self {
            condition,
            truth: Some(truth),
            state,
        }
    }

    pub(crate) fn unresolved_from_changed(entry: &SymbolicLocalState, known: &[Self]) -> Self {
        let variables = known
            .iter()
            .flat_map(|outcome| outcome.state.fragment_values.keys())
            .chain(entry.fragment_values.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut state = entry.clone();
        for variable in variables {
            if known.iter().all(|outcome| {
                outcome.state.fragment_values.get(&variable) == entry.fragment_values.get(&variable)
            }) {
                continue;
            }
            state.clear_variable(&variable);
            state
                .fragment_values
                .insert(variable, LocalBinding::unknown());
        }
        Self {
            condition: Predicate::False,
            truth: None,
            state,
        }
    }

    pub(crate) fn condition(&self) -> Predicate {
        self.condition.clone()
    }

    pub(crate) fn guard_all(&mut self, condition: Predicate, memo: &PredicateMemo) {
        self.condition = crate::scalar_value::conjoin_predicates_with_memo(
            condition.clone(),
            self.condition.clone(),
            memo,
        )
        .unwrap_or(Predicate::False);
        let Some(truth) = self.truth.take() else {
            return;
        };
        self.truth = Some(TruthCondition::all_with_memo(
            [
                TruthCondition::from_predicate_with_memo(condition, memo),
                truth,
            ],
            memo,
        ));
    }

    pub(crate) fn replace_state(&mut self, state: &SymbolicLocalState) {
        self.state = state.clone();
    }
}

impl SymbolicLocalState {
    pub(crate) fn with_root(value: AbstractValue, mode: BindingEvaluationMode) -> Self {
        let mut state = Self::default();
        state
            .fragment_values
            .insert(String::new(), LocalBinding::new(value, mode));
        state
    }

    pub(crate) fn join_branch_outcomes(&mut self, entry: &Self, outcomes: &[Self]) {
        *self = joined_branch_outcomes(entry, outcomes);
    }

    /// Rebuild scalar locals from the mutually exclusive arms of one exact
    /// `if` chain. Ordinary local facts join by equality or value choice;
    /// scalar values retain the condition that selected each reassignment.
    pub(crate) fn join_scalar_dispatch_arms(
        &mut self,
        entry: &Self,
        arms: &[(TruthCondition, Self)],
        has_unconditional_else: bool,
        memo: &PredicateMemo,
    ) {
        if let Some(joined) = joined_scalar_dispatch_arms(entry, arms, has_unconditional_else, memo)
        {
            self.scalar_dispatches = joined;
        }
    }

    pub(crate) fn join_binding_decisions(
        &mut self,
        fallthrough: &Self,
        decisions: &[Option<std::rc::Rc<crate::eval_env::BindingDecision>>],
        outcomes: &[Self],
    ) {
        self.fragment_values = joined_binding_decisions(fallthrough, decisions, outcomes);
    }

    /// Join every semantic local domain from a complete set of control exits.
    pub(crate) fn join_control_outcomes(
        &mut self,
        entry: &Self,
        outcomes: &[ControlOutcome],
        memo: &PredicateMemo,
    ) {
        if outcomes.is_empty() {
            *self = entry.clone();
            return;
        }
        let states = outcomes
            .iter()
            .map(|outcome| outcome.state.clone())
            .collect::<Vec<_>>();
        let decisions = outcomes
            .iter()
            .map(|outcome| {
                outcome
                    .truth
                    .clone()
                    .map(crate::eval_env::BindingDecision::new)
            })
            .collect::<Vec<_>>();
        let scalar_arms = outcomes
            .iter()
            .filter_map(|outcome| {
                outcome
                    .truth
                    .clone()
                    .map(|truth| (truth, outcome.state.clone()))
            })
            .collect::<Vec<_>>();
        self.join_branch_outcomes(entry, &states);
        self.join_binding_decisions(entry, &decisions, &states);
        if scalar_arms.len() == outcomes.len() {
            self.join_scalar_dispatch_arms(entry, &scalar_arms, true, memo);
            self.join_truthy_reduction_arms(entry, &scalar_arms, true, memo);
        }
    }

    pub(crate) fn widen_changed_fragment_bindings(
        &mut self,
        entry: &Self,
        decision: std::rc::Rc<crate::eval_env::BindingDecision>,
    ) {
        for (variable, binding) in &mut self.fragment_values {
            if entry.fragment_values.get(variable) == Some(binding) {
                continue;
            }
            *binding = LocalBinding::unresolved_with_candidate(
                std::rc::Rc::clone(&decision),
                binding.clone(),
            );
        }
    }

    /// Rebuild local truthiness from the mutually exclusive arms of one
    /// exact `if` chain. Every arm, including the implicit fallthrough,
    /// contributes its selection condition so an untouched entry reduction
    /// cannot erase a kill-switch reassignment at the union.
    pub(crate) fn join_truthy_reduction_arms(
        &mut self,
        entry: &Self,
        arms: &[(TruthCondition, Self)],
        has_unconditional_else: bool,
        memo: &PredicateMemo,
    ) {
        if let Some(joined) =
            joined_truthy_reduction_arms(entry, arms, has_unconditional_else, memo)
        {
            self.truthy_reductions.extend(joined);
        }
    }

    /// Conjoin `condition` onto every truthiness reduction this branch
    /// CHANGED relative to `entry`: the reassigned truthiness holds only
    /// where the branch ran, so the cross-branch union becomes the exact
    /// disjunction of guarded arms (the range-sentinel flag pattern).
    ///
    /// Bounded on both sides: an approximate arm condition would only
    /// poison every consumer into abstention, and unbounded conjoining at
    /// nested joins grows reductions combinatorially, so oversized results
    /// drop the reduction instead of letting an unstamped branch claim
    /// escape through a later join.
    pub(crate) fn conjoin_changed_truthy_reductions(
        &mut self,
        entry: &Self,
        condition: &Predicate,
        memo: &PredicateMemo,
    ) {
        const MAX_STAMPED_GUARDS: usize = 6;
        if matches!(condition.kind(), helm_schema_core::PredicateKind::True)
            || condition.contains_approximation()
        {
            return;
        }
        // A range exit reads each reduction existentially ("some iteration
        // made the accumulator truthy"), which is exact only for monotone
        // accumulators. A falsy-capable reassignment inside the body makes
        // the final value last-write-wins, so the reduction abstains
        // instead of quantifying: `[{enabled: true}, {enabled: false}]`
        // ends the sentinel falsy while a member still satisfies ∃.
        if matches!(
            single_term(condition).map(Predicate::kind),
            Some(helm_schema_core::PredicateKind::Guard(Guard::Range { .. }))
        ) {
            let clears = &self.truthiness_clears;
            self.truthy_reductions
                .retain(|variable, _| !clears.contains(variable));
        }
        let mut dropped = Vec::new();
        self.truthy_reductions.retain(|variable, reduction| {
            let entry_reduction = entry.truthy_reductions.get(variable);
            if entry_reduction == Some(reduction)
                || matches!(reduction.kind(), helm_schema_core::PredicateKind::False)
                || reduction.contains_approximation()
            {
                return true;
            }
            if predicate_guard_count(condition) + predicate_guard_count(reduction)
                <= MAX_STAMPED_GUARDS
            {
                *reduction = quantify_range_member_reduction(condition, reduction)
                    .map(Predicate::from)
                    .unwrap_or_else(|| Predicate::all(vec![condition.clone(), reduction.clone()]));
                return true;
            }
            let changed = entry_reduction
                .and_then(|entry| changed_truthy_reduction(entry, reduction))
                .unwrap_or_else(|| reduction.clone());
            if predicate_implies(&changed, condition, memo) {
                return true;
            }
            let stamped =
                if let Some(quantified) = quantify_range_member_reduction(condition, &changed) {
                    Predicate::from(quantified)
                } else {
                    if predicate_guard_count(condition) + predicate_guard_count(&changed)
                        > MAX_STAMPED_GUARDS
                    {
                        dropped.push(variable.clone());
                        return false;
                    }
                    Predicate::all(vec![condition.clone(), changed])
                };
            if predicate_guard_count(&stamped) > MAX_STAMPED_GUARDS {
                dropped.push(variable.clone());
                return false;
            }
            *reduction = entry_reduction.map_or(stamped.clone(), |entry| {
                union_truthy_reductions(entry, &stamped)
            });
            true
        });
        for variable in dropped {
            self.truthiness_abstentions.insert(variable);
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
        self.bind_fragment_value_with_metadata(
            kind,
            variable,
            binding,
            BindingValueMetadata::default(),
        );
    }

    pub(crate) fn bind_fragment_value_with_metadata(
        &mut self,
        kind: AssignmentKind,
        variable: String,
        binding: Option<AbstractValue>,
        metadata: BindingValueMetadata,
    ) {
        self.bind_local_binding(
            kind,
            variable,
            binding.map(|value| {
                LocalBinding::new_with_metadata(value, BindingEvaluationMode::Evaluated, metadata)
            }),
        );
    }

    pub(crate) fn bind_local_binding(
        &mut self,
        kind: AssignmentKind,
        variable: String,
        binding: Option<LocalBinding>,
    ) {
        self.record_binding_shadow(kind, &variable);
        self.set_fragment_value(variable, binding);
    }

    pub(crate) fn bind_direct_fragment_value(
        &mut self,
        kind: AssignmentKind,
        variable: String,
        binding: AbstractValue,
    ) {
        self.record_binding_shadow(kind, &variable);
        let range_domain = self.range_domains.remove(&variable);
        self.clear_variable(&variable);
        if let Some(range_domain) = range_domain {
            self.range_domains.insert(variable.clone(), range_domain);
        }
        self.fragment_values
            .insert(variable, LocalBinding::direct(binding));
    }

    pub(crate) fn clear_current_binding(&mut self, variable: &str) {
        self.clear_variable(variable);
    }

    pub(crate) fn bind_current_variable_state(
        &mut self,
        kind: AssignmentKind,
        source: &str,
        target: String,
    ) {
        let binding = self.variable_state(source);
        self.record_binding_shadow(kind, &target);
        self.clear_variable(&target);
        self.restore_variable_state(&target, binding);
    }

    pub(crate) fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.record_scope_shadow(&variable);
        self.clear_variable(&variable);
        self.range_domains.insert(variable, literals);
    }

    pub(crate) fn set_chart_value_defaults(
        &mut self,
        defaults: BTreeSet<helm_schema_core::ValuesPath>,
    ) {
        self.chart_value_defaults = defaults;
    }

    pub(crate) fn append_chart_value_defaults(
        &mut self,
        defaults: &mut BTreeSet<helm_schema_core::ValuesPath>,
    ) {
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
            scalar_dispatch: self.scalar_dispatches.get(variable).cloned(),
            truthy_reduction: self.truthy_reductions.get(variable).cloned(),
            truthiness_abstained: self.truthiness_abstentions.contains(variable),
            truthiness_cleared: self.truthiness_clears.contains(variable),
            typeof_source: self.typeof_sources.get(variable).cloned(),
            int_cast_source: self.int_cast_sources.get(variable).cloned(),
            range_member_value: self.range_member_values.get(variable).cloned(),
            definite_range_member_value: self.definite_range_member_values.get(variable).cloned(),
        }
    }

    fn variable_has_current_value(&self, variable: &str) -> bool {
        self.range_domains.contains_key(variable)
            || self.get_bindings.contains_key(variable)
            || self.fragment_values.contains_key(variable)
            || self.default_paths.contains_key(variable)
            || self.output_meta.contains_key(variable)
            || self.scalar_dispatches.contains_key(variable)
            || self.truthy_reductions.contains_key(variable)
            || self.truthiness_abstentions.contains(variable)
            || self.typeof_sources.contains_key(variable)
            || self.int_cast_sources.contains_key(variable)
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
        restore_map_entry(
            &mut self.scalar_dispatches,
            variable,
            previous.scalar_dispatch,
        );
        restore_map_entry(
            &mut self.truthy_reductions,
            variable,
            previous.truthy_reduction,
        );
        if previous.truthiness_abstained {
            self.truthiness_abstentions.insert(variable.to_string());
        } else {
            self.truthiness_abstentions.remove(variable);
        }
        if previous.truthiness_cleared {
            self.truthiness_clears.insert(variable.to_string());
        } else {
            self.truthiness_clears.remove(variable);
        }
        restore_map_entry(&mut self.typeof_sources, variable, previous.typeof_source);
        restore_map_entry(
            &mut self.int_cast_sources,
            variable,
            previous.int_cast_source,
        );
        restore_map_entry(
            &mut self.range_member_values,
            variable,
            previous.range_member_value,
        );
        restore_map_entry(
            &mut self.definite_range_member_values,
            variable,
            previous.definite_range_member_value,
        );
    }

    fn set_fragment_value(&mut self, variable: String, binding: Option<LocalBinding>) {
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
        self.scalar_dispatches.remove(variable);
        self.truthy_reductions.remove(variable);
        self.truthiness_abstentions.remove(variable);
        self.truthiness_clears.remove(variable);
        self.typeof_sources.remove(variable);
        self.int_cast_sources.remove(variable);
        self.range_member_values.remove(variable);
        self.definite_range_member_values.remove(variable);
    }
}

fn restore_map_entry<T>(map: &mut HashMap<String, T>, variable: &str, value: Option<T>) {
    if let Some(value) = value {
        map.insert(variable.to_string(), value);
    } else {
        map.remove(variable);
    }
}

/// The lone non-trivial conjunct of a flat `And` formula, when there is
/// exactly one; a `False` conjunct or a second term abstains.
fn single_term(predicate: &Predicate) -> Option<&Predicate> {
    fn collect<'a>(predicate: &'a Predicate, term: &mut Option<&'a Predicate>) -> Option<()> {
        match predicate.kind() {
            helm_schema_core::PredicateKind::True => Some(()),
            helm_schema_core::PredicateKind::And(items) => {
                for item in items {
                    collect(item, term)?;
                }
                Some(())
            }
            helm_schema_core::PredicateKind::False => None,
            _ => {
                if term.replace(predicate).is_some() {
                    return None;
                }
                Some(())
            }
        }
    }

    let mut term = None;
    collect(predicate, &mut term)?;
    term
}

fn quantify_range_member_reduction(condition: &Predicate, reduction: &Predicate) -> Option<Guard> {
    let helm_schema_core::PredicateKind::Guard(Guard::Range { path: range_path }) =
        single_term(condition)?.kind()
    else {
        return None;
    };
    let member_predicate = single_term(reduction)?;
    let helm_schema_core::PredicateKind::Guard(
        Guard::Eq {
            path: member_path, ..
        }
        | Guard::Truthy { path: member_path },
    ) = member_predicate.kind()
    else {
        return None;
    };
    let range_segments: Vec<&helm_schema_core::Segment> = range_path.segments().collect();
    let member_segments: Vec<&helm_schema_core::Segment> = member_path.segments().collect();
    let [wildcard, member] = member_segments.get(range_segments.len()..)? else {
        return None;
    };
    if !wildcard.is_each_member() || member_segments.get(..range_segments.len())? != range_segments
    {
        return None;
    }

    match member_predicate.kind() {
        helm_schema_core::PredicateKind::Guard(Guard::Eq { value, .. }) => {
            Some(Guard::ContainsMemberEquals {
                path: range_path.clone(),
                member: member.literal()?.to_owned(),
                value: value.clone(),
            })
        }
        helm_schema_core::PredicateKind::Guard(Guard::Truthy { .. }) => {
            Some(Guard::ContainsTruthyMember {
                path: range_path.clone(),
                member: member.literal()?.to_owned(),
            })
        }
        _ => None,
    }
}

fn predicate_guard_count(predicate: &Predicate) -> usize {
    match predicate.kind() {
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Approximate { .. } => 0,
        helm_schema_core::PredicateKind::Guard(_) => 1,
        helm_schema_core::PredicateKind::Not(inner) => predicate_guard_count(inner),
        helm_schema_core::PredicateKind::And(items)
        | helm_schema_core::PredicateKind::Or(items) => {
            items.iter().map(predicate_guard_count).sum()
        }
    }
}

fn changed_truthy_reduction(entry: &Predicate, reduction: &Predicate) -> Option<Predicate> {
    if matches!(entry.kind(), helm_schema_core::PredicateKind::False) {
        return Some(reduction.clone());
    }
    let helm_schema_core::PredicateKind::Or(reduction_items) = reduction.kind() else {
        return None;
    };
    let entry_items = match entry.kind() {
        helm_schema_core::PredicateKind::Or(items) => items,
        _ => std::slice::from_ref(entry),
    };
    if !entry_items
        .iter()
        .all(|item| reduction_items.contains(item))
    {
        return None;
    }
    let changed = reduction_items
        .iter()
        .filter(|item| !entry_items.contains(item))
        .cloned()
        .collect::<Vec<_>>();
    Some(match changed.as_slice() {
        [] => Predicate::False,
        [predicate] => predicate.clone(),
        _ => Predicate::Or(changed),
    })
}

fn union_truthy_reductions(left: &Predicate, right: &Predicate) -> Predicate {
    if matches!(left.kind(), helm_schema_core::PredicateKind::False) {
        return right.clone();
    }
    if matches!(right.kind(), helm_schema_core::PredicateKind::False) || left == right {
        return left.clone();
    }
    match left.kind() {
        helm_schema_core::PredicateKind::Or(predicates) => {
            let mut predicates = predicates.to_vec();
            if !predicates.contains(right) {
                predicates.push(right.clone());
            }
            Predicate::Or(predicates)
        }
        _ => Predicate::Or(vec![left.clone(), right.clone()]),
    }
}

fn predicate_implies(antecedent: &Predicate, consequent: &Predicate, memo: &PredicateMemo) -> bool {
    if memo.exactly_implies(antecedent, consequent) {
        return true;
    }
    if let (
        helm_schema_core::PredicateKind::Or(antecedents),
        helm_schema_core::PredicateKind::Or(consequents),
    ) = (antecedent.kind(), consequent.kind())
    {
        return antecedents.iter().all(|antecedent| {
            consequents
                .iter()
                .any(|consequent| predicate_implies(antecedent, consequent, memo))
        });
    }
    match consequent.kind() {
        helm_schema_core::PredicateKind::And(predicates) => predicates
            .iter()
            .all(|predicate| predicate_implies(antecedent, predicate, memo)),
        helm_schema_core::PredicateKind::Or(predicates) => predicates
            .iter()
            .any(|predicate| predicate_implies(antecedent, predicate, memo)),
        _ => match antecedent.kind() {
            helm_schema_core::PredicateKind::Or(predicates) => predicates
                .iter()
                .all(|predicate| predicate_implies(predicate, consequent, memo)),
            helm_schema_core::PredicateKind::And(predicates) => predicates
                .iter()
                .any(|predicate| predicate_implies(predicate, consequent, memo)),
            _ => leaf_predicate_implies(antecedent, consequent),
        },
    }
}

fn leaf_predicate_implies(antecedent: &Predicate, consequent: &Predicate) -> bool {
    let Some(present_path) = predicate_present_path(antecedent) else {
        return false;
    };
    match consequent.kind() {
        helm_schema_core::PredicateKind::Not(inner) => matches!(
            inner.kind(),
            helm_schema_core::PredicateKind::Guard(Guard::Absent { path })
                if path == present_path || path_is_strict_ancestor(path, present_path)
        ),
        helm_schema_core::PredicateKind::Guard(Guard::HasKey { path, key }) => {
            let mut key_path = path.clone();
            key_path.push(key);
            present_path == &key_path || present_path.is_descendant_of(&key_path)
        }
        helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == present_path => {
            match antecedent.kind() {
                helm_schema_core::PredicateKind::Guard(Guard::Eq { value, .. }) => {
                    crate::value_path_context::guard_value_is_truthy(value)
                }
                helm_schema_core::PredicateKind::Guard(Guard::MatchesPattern {
                    pattern, ..
                }) => regex::Regex::new(pattern).is_ok_and(|pattern| !pattern.is_match("")),
                _ => false,
            }
        }
        helm_schema_core::PredicateKind::Guard(Guard::Range { path } | Guard::Truthy { path }) => {
            path_is_strict_ancestor(path, present_path)
        }
        _ => false,
    }
}

fn predicate_present_path(predicate: &Predicate) -> Option<&helm_schema_core::ValuesPath> {
    match predicate.kind() {
        helm_schema_core::PredicateKind::Not(inner) => match inner.kind() {
            helm_schema_core::PredicateKind::Guard(Guard::Absent { path }) => Some(path),
            _ => None,
        },
        helm_schema_core::PredicateKind::Guard(
            Guard::Truthy { path }
            | Guard::Eq { path, .. }
            | Guard::MatchesPattern { path, .. }
            | Guard::NotMatchesPattern { path, .. }
            | Guard::TypeIs { path, .. },
        ) => Some(path),
        _ => None,
    }
}

fn path_is_strict_ancestor(
    parent: &helm_schema_core::ValuesPath,
    child: &helm_schema_core::ValuesPath,
) -> bool {
    child.is_descendant_of(parent)
}
