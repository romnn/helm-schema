use std::collections::HashMap;

use helm_schema_core::{Guard, Predicate, PredicateMemo};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::GetBindingPlan;
use crate::eval_env::{BindingEvaluationMode, BindingValueMetadata, LocalBinding};
use crate::fragment_assignment::AssignmentKind;
use crate::scalar_value::TruthCondition;
use crate::symbolic_local_state::{ControlOutcome, SymbolicLocalState};

use super::LocalSlot;
use super::branch_join::joined_branch_outcomes;

impl SymbolicLocalState {
    pub(crate) fn with_root(value: AbstractValue, mode: BindingEvaluationMode) -> Self {
        let mut state = Self::default();
        state
            .slot_mut("")
            .binding
            .replace(LocalBinding::new(value, mode));
        state
    }

    pub(crate) fn join_branch_outcomes(&mut self, entry: &Self, outcomes: &[Self]) {
        *self = joined_branch_outcomes(entry, outcomes);
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

    /// Widen every fragment binding this state changed relative to `entry`
    /// into an unresolved choice between the changed value and an unknown
    /// alternative: the exit of a loop whose iteration count is not exact
    /// may or may not have run the body that changed it.
    pub(crate) fn widen_changed_fragment_bindings(&mut self, entry: &Self) {
        let decision = crate::eval_env::BindingDecision::new(TruthCondition::Unknown);
        for (variable, slot) in &mut self.slots {
            let Some(binding) = &mut slot.binding else {
                continue;
            };
            if entry.slot(variable).binding.as_ref() == Some(binding) {
                continue;
            }
            *binding = LocalBinding::unresolved_with_candidate(
                std::rc::Rc::clone(&decision),
                binding.clone(),
            );
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
            for slot in self.slots.values_mut() {
                if slot.truthiness_cleared {
                    slot.truthy_reduction = None;
                }
            }
        }
        for (variable, slot) in &mut self.slots {
            let Some(reduction) = &mut slot.truthy_reduction else {
                continue;
            };
            let keep = (|| {
                let entry_reduction = entry.slot(variable).truthy_reduction.as_ref();
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
                        .unwrap_or_else(|| {
                            Predicate::all(vec![condition.clone(), reduction.clone()])
                        });
                    return true;
                }
                let changed = entry_reduction
                    .and_then(|entry| changed_truthy_reduction(entry, reduction))
                    .unwrap_or_else(|| reduction.clone());
                if predicate_implies(&changed, condition, memo) {
                    return true;
                }
                let stamped = if let Some(quantified) =
                    quantify_range_member_reduction(condition, &changed)
                {
                    Predicate::from(quantified)
                } else {
                    if predicate_guard_count(condition) + predicate_guard_count(&changed)
                        > MAX_STAMPED_GUARDS
                    {
                        slot.truthiness_abstained = true;
                        return false;
                    }
                    Predicate::all(vec![condition.clone(), changed])
                };
                if predicate_guard_count(&stamped) > MAX_STAMPED_GUARDS {
                    slot.truthiness_abstained = true;
                    return false;
                }
                *reduction = entry_reduction.map_or(stamped.clone(), |entry| {
                    union_truthy_reductions(entry, &stamped)
                });
                true
            })();
            if !keep {
                slot.truthy_reduction = None;
            }
        }
    }

    pub(crate) fn enter_local_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub(crate) fn exit_local_scope(&mut self) {
        let Some(scope) = self.scopes.pop() else {
            return;
        };
        for (variable, previous) in scope {
            self.slots.insert(variable, previous);
        }
    }

    pub(crate) fn apply_get_binding(&mut self, plan: GetBindingPlan) {
        self.record_binding_shadow(plan.kind, &plan.variable);
        self.clear_current_binding(&plan.variable);
        self.slots.entry(plan.variable).or_default().get_binding = Some(plan.binding);
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
        self.clear_current_binding(&variable);
        if let Some(binding) = binding {
            self.slots.entry(variable).or_default().binding = Some(binding);
        }
    }

    pub(crate) fn bind_direct_fragment_value(
        &mut self,
        kind: AssignmentKind,
        variable: String,
        binding: AbstractValue,
    ) {
        self.record_binding_shadow(kind, &variable);
        let range_domain = self.slot_mut(&variable).range_domain.take();
        self.clear_current_binding(&variable);
        let slot = self.slots.entry(variable).or_default();
        slot.range_domain = range_domain;
        slot.binding = Some(LocalBinding::direct(binding));
    }

    /// Binding a variable in one domain displaces whatever it held in every
    /// other domain (and any stale entry in its own).
    pub(crate) fn clear_current_binding(&mut self, variable: &str) {
        self.slots.remove(variable);
    }

    pub(crate) fn bind_current_variable_state(
        &mut self,
        kind: AssignmentKind,
        source: &str,
        target: &str,
    ) {
        let binding = self.slot(source).clone();
        self.record_binding_shadow(kind, target);
        self.clear_current_binding(target);
        self.slots.insert(target.to_string(), binding);
    }

    pub(crate) fn insert_range_domain(&mut self, variable: String, literals: Vec<String>) {
        self.record_scope_shadow(&variable);
        self.clear_current_binding(&variable);
        self.slots.entry(variable).or_default().range_domain = Some(literals);
    }

    /// Record the pre-write state of `variable` into the current scope frame
    /// so `exit_local_scope` restores it. `:=` always shadows; `=` writes
    /// through to the existing binding (the write survives scope exit), so it
    /// shadows only when the variable has no current value — Go templates
    /// treat that as a fresh declaration.
    fn record_binding_shadow(&mut self, kind: AssignmentKind, variable: &str) {
        if matches!(kind, AssignmentKind::Assignment) && self.slot(variable).has_current_value() {
            return;
        }
        self.record_scope_shadow(variable);
    }

    fn record_scope_shadow(&mut self, variable: &str) {
        let previous = self.slot(variable).clone();
        let Some(scope) = self.scopes.last_mut() else {
            return;
        };
        scope.entry(variable.to_string()).or_insert(previous);
    }

    pub(crate) fn evaluation_bindings(&self) -> HashMap<String, LocalBinding> {
        self.slots
            .iter()
            .filter_map(|(name, slot)| {
                slot.binding
                    .clone()
                    .or_else(|| slot.range_member_value.clone().map(LocalBinding::direct))
                    .map(|binding| (name.clone(), binding))
            })
            .collect()
    }

    pub(crate) fn slot(&self, variable: &str) -> &LocalSlot {
        self.slots.get(variable).unwrap_or(&LocalSlot::EMPTY)
    }

    pub(crate) fn slot_mut(&mut self, variable: &str) -> &mut LocalSlot {
        self.slots.entry(variable.to_string()).or_default()
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
