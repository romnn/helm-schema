//! Control-region evaluation: each branch's contributions are evaluated
//! under the branch's decoded condition (plus the negations of prior arms)
//! and dissolve into the surrounding container as guarded arms. Local
//! bindings join across branches with the same rules as the symbolic
//! walker.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr, TemplateHeader, range_variable_name_expr};
use helm_schema_syntax::{ControlKind, ControlRegion, Node, ScalarPart};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{literal_dict_range_keys, parse_literal_list_range_expr};
use crate::eval_effect::{SelectionTruthReachability, SelectionTruthSource};
use crate::scalar_value::{
    ScalarValue, ScalarValueDispatch, TruthCondition, any_predicates_with_memo,
};
use crate::value_path_context::{guard_value_is_truthy, predicate_any};
use crate::{Guard, ValueKind};
use helm_schema_core::{GuardValue, Predicate};

use super::domain::{
    AbstractFragment, Guarded, PathCondition, Splice, SpliceMeta, and_conditions_with_memo,
};
use super::eval::{
    Adopted, AdoptionPlan, ArmSpec, Contributions, Interpreter, NodeView, ParentShape, ParentShell,
    ParentShellArm, SourceWindow, branch_window,
};

/// Exact range sequences resolved from a statically known list iterable.
/// Each alternative preserves one list's item order and bindings.
pub(super) struct RangeIterations {
    pub(super) alternatives: Vec<RangeIterationAlternative>,
    pub(super) has_unresolved: bool,
    /// A statically nonempty iterable promotes the body outcome at the
    /// join: bindings set in every iteration survive the region.
    pub(super) nonempty: bool,
}

pub(super) struct RangeIterationAlternative {
    pub(super) truth: TruthCondition,
    pub(super) items: Vec<RangeIterationBinding>,
}

/// State produced by evaluating a control header before any arm is selected.
struct ControlHeaderTransition {
    in_scope: crate::symbolic_local_state::SymbolicLocalState,
    after_scope: crate::symbolic_local_state::SymbolicLocalState,
}

#[derive(PartialEq)]
pub(super) struct RangeIterationBinding {
    pub(super) dot: AbstractValue,
    pub(super) variable: Option<(String, AbstractValue)>,
    /// The KEY variable of a destructured header (`$i` in
    /// `range $i, $v := …`), bound to the iteration ordinal for lists so
    /// last-element arithmetic (`eq (len …) (add1 $i)`) decodes per
    /// unrolled iteration.
    pub(super) key: Option<(String, AbstractValue)>,
}

impl Interpreter<'_> {
    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
    pub(super) fn eval_control(
        &mut self,
        region: &ControlRegion,
        adopted: &[Adopted<'_>],
        escaped: Vec<DeferredNodes<'_>>,
    ) -> Contributions {
        if matches!(region.kind, ControlKind::Define | ControlKind::Block) {
            return self.eval_suppressed_control_continuation(region, adopted, escaped);
        }
        let branch_nodes = branch_node_lists(region, adopted);
        let (mut escaped_per_branch, mut escaped_after) = split_escaped(region, escaped);

        let entry_locals = self.locals.clone();
        let entry_scope = self.mark_scope();
        // Root-context `set` state joins across if/else arms like locals:
        // each arm evaluates from the entry state (arms are mutually
        // exclusive at runtime, so one arm's mutation must not leak into a
        // sibling's evaluation), and the outcomes join after the region —
        // complete literal-assignment chains into an exact value dispatch
        // (vault's five-arm `vault.mode`). Non-If regions keep the
        // sequential accumulation.
        let entry_root = (region.kind == ControlKind::If).then(|| self.capture_root_set_state());
        let mut root_arm_states: Vec<(Predicate, bool, RootSetState)> = Vec::new();
        let mut local_arm_states: Vec<(
            TruthCondition,
            crate::symbolic_local_state::SymbolicLocalState,
        )> = Vec::new();
        let mut binding_decisions = Vec::new();

        let mut out = Contributions::default();
        let mut outcomes = Vec::new();
        let mut arm_header_exprs: Vec<Option<TemplateExpr>> = Vec::new();
        let mut prior_conditions: Vec<PathCondition> = Vec::new();
        let mut prior_semantic_reachability: Vec<SelectionTruthReachability> = Vec::new();
        let mut has_unconditional_else = false;
        let mut promote_body_outcome = false;
        let mut header_transition: Option<ControlHeaderTransition> = None;
        let mut common_header_fallthrough = None;

        for (index, _branch) in region.branches.iter().enumerate() {
            self.locals = if index > 0 {
                header_transition.as_ref().map_or_else(
                    || entry_locals.clone(),
                    |transition| transition.in_scope.clone(),
                )
            } else {
                entry_locals.clone()
            };
            self.rewind(entry_scope);
            if let Some(entry_root) = &entry_root {
                self.restore_root_set_state(entry_root);
            }

            let mut arm_condition = Predicate::True;
            // Later arms run under the negations of every earlier decoded condition.
            // A range else uses header truth instead of the body-only collection marker.
            for prior in &prior_conditions {
                let negated = prior.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions_with_memo(
                    arm_condition,
                    negated,
                    self.db.predicate_memo().as_ref(),
                );
            }

            let arm = self.classify_branch(region, index);
            arm_header_exprs.push(match &arm {
                ArmSpec::If(Some(header)) => Some(header.expr().clone()),
                _ => None,
            });
            if matches!(arm, ArmSpec::Else) && index > 0 {
                has_unconditional_else = true;
            }
            let nodes = branch_nodes.get(index).cloned().unwrap_or_default();
            // Header reads carry the region's site: the unique resource the
            // region intersects (none when it spans several documents).
            let region_site = self.region_site(region.span);
            let previous_site = std::mem::replace(&mut self.current_site, region_site);
            if index == 0 {
                self.locals.enter_local_scope();
            }
            let (own_condition, extra, iterations, own_reachability, post_header_locals) =
                self.activate_arm(&arm, &nodes, region.span.start, index);
            self.current_site = previous_site;
            if !matches!(arm, ArmSpec::Else) {
                let in_scope = post_header_locals.unwrap_or_else(|| self.locals.clone());
                let mut after_scope = in_scope.clone();
                after_scope.exit_local_scope();
                if index == 0 {
                    common_header_fallthrough = Some(after_scope.clone());
                }
                header_transition = Some(ControlHeaderTransition {
                    in_scope,
                    after_scope,
                });
            }
            binding_decisions.push((!matches!(arm, ArmSpec::Else)).then(|| {
                crate::eval_env::BindingDecision::new(
                    own_reachability.truth_condition_with_memo(self.db.predicate_memo().as_ref()),
                )
            }));
            // The value-dispatch join needs mutually exclusive, total arm
            // conditions: an If arm whose header failed to decode (or a
            // with/range arm inside the chain) leaves later negations
            // incomplete.
            let arm_decoded = match &arm {
                ArmSpec::Else => true,
                ArmSpec::If(_) => own_condition
                    .as_ref()
                    .is_some_and(|condition| !condition.contains_approximation()),
                _ => false,
            };
            if let Some(own) = own_condition {
                arm_condition = and_conditions_with_memo(
                    arm_condition,
                    own.clone(),
                    self.db.predicate_memo().as_ref(),
                );
                if matches!(arm, ArmSpec::Range { .. }) {
                    if let Some(selection) = own_reachability.when_true().exact_predicate() {
                        prior_conditions.push(selection);
                    }
                } else {
                    prior_conditions.push(own);
                }
            }
            let semantic_arm_truth = if region.kind == ControlKind::If {
                let join_reachability = if matches!(arm, ArmSpec::With(_) | ArmSpec::Range { .. }) {
                    // The carrier owns the decoded truth, but scalar-local
                    // joins still consume the legacy condition shape until
                    // Step 6b.3. Keep that adapter abstaining: promoting a
                    // with/range condition here lets a branch-local provider
                    // shape become a path-wide input contract.
                    SelectionTruthReachability::unknown(own_reachability.when_true().truth_source())
                } else {
                    own_reachability.clone()
                };
                let truth = TruthCondition::all_with_memo(
                    prior_semantic_reachability
                        .iter()
                        .map(|reachability| {
                            reachability
                                .truth_condition_with_memo(self.db.predicate_memo().as_ref())
                        })
                        .map(|truth| truth.negated_with_memo(self.db.predicate_memo().as_ref()))
                        .chain(std::iter::once(
                            join_reachability
                                .truth_condition_with_memo(self.db.predicate_memo().as_ref()),
                        )),
                    self.db.predicate_memo().as_ref(),
                );
                if matches!(arm, ArmSpec::If(_) | ArmSpec::With(_)) {
                    prior_semantic_reachability.push(join_reachability);
                }
                Some(truth)
            } else {
                None
            };
            if index == 0 && iterations.as_ref().is_some_and(|plan| plan.nonempty) {
                promote_body_outcome = true;
            }
            let arm_binding_kind = match &arm {
                ArmSpec::Range { binding_kind, .. } => *binding_kind,
                _ => crate::fragment_assignment::AssignmentKind::Declaration,
            };

            if matches!(arm, ArmSpec::Range { .. }) {
                self.loop_depth += 1;
            }
            let current_escaped = escaped_per_branch
                .get_mut(index)
                .map(std::mem::take)
                .unwrap_or_default();
            let branch_steps = self.branch_steps(region, index, adopted, nodes, current_escaped);
            let mut contributions = if arm_condition == Predicate::False {
                Contributions::default()
            } else {
                match &iterations {
                    Some(plan) => {
                        // Items within one alternative run sequentially. Distinct
                        // list alternatives start from the same state so a split
                        // path such as `a.b | c.d` cannot cross-pair its segments.
                        let alternative_entry = self.locals.clone();
                        let mut all = Contributions::default();
                        let mut alternative_outcomes = Vec::new();
                        // Items beyond the alternatives' SHARED PREFIX execute
                        // only under the (undecoded) alternative selection: an
                        // approximate conjunct on their CAPTURE conjunctions
                        // keeps strict captures from binding unconditionally
                        // (nats' jsonpatch appends "from" to `$opPathKeys` only
                        // for copy/move patches — demanding `from` of every
                        // patch member falsely rejects valid adds). Rows and
                        // type hints keep the ordinary join semantics, so
                        // per-alternative RENDER facts still lower exactly
                        // (kyverno's label-merge lists differ across callers).
                        let shared_items = if plan.alternatives.len() > 1 {
                            let (first, rest) = plan
                                .alternatives
                                .split_first()
                                .map_or((&[][..], &[][..]), |(first, rest)| {
                                    (first.items.as_slice(), rest)
                                });
                            (0..first.len())
                                .take_while(|&index| {
                                    rest.iter().all(|alternative| {
                                        alternative.items.get(index) == first.get(index)
                                    })
                                })
                                .count()
                        } else {
                            usize::MAX
                        };
                        for alternative in &plan.alternatives {
                            self.locals = alternative_entry.clone();
                            let mut remaining = Predicate::True;
                            let mut exit_outcomes = Vec::new();
                            for (item_index, item) in alternative.items.iter().enumerate() {
                                if remaining == Predicate::False {
                                    break;
                                }
                                if let Some((variable, binding)) = &item.variable {
                                    self.locals.bind_direct_fragment_value(
                                        arm_binding_kind,
                                        variable.clone(),
                                        binding.clone(),
                                    );
                                }
                                if let Some((variable, ordinal)) = &item.key {
                                    self.locals.bind_direct_fragment_value(
                                        arm_binding_kind,
                                        variable.clone(),
                                        ordinal.clone(),
                                    );
                                }
                                let item_scope = self.mark_scope();
                                if item_index >= shared_items {
                                    self.alternative_capture_approximates.push(
                                        Predicate::approximate(
                                            format!(
                                                "{}:{}:range alternative",
                                                self.source_offset, region.span.start
                                            ),
                                            std::collections::BTreeSet::new(),
                                        ),
                                    );
                                }
                                self.push_predicate(remaining.clone());
                                self.push_dot(
                                    Some(item.dot.clone()),
                                    crate::eval_env::BindingEvaluationMode::Direct,
                                );
                                let mut iteration = self.eval_branch_steps(&branch_steps);
                                self.rewind(item_scope);
                                let mut loop_control = iteration.take_loop_control();
                                let break_condition = loop_control.break_condition();
                                loop_control
                                    .guard_all(&remaining, self.db.predicate_memo().as_ref());
                                iteration.guard_all(&remaining, self.db.predicate_memo().as_ref());
                                all.extend(iteration);
                                exit_outcomes.append(&mut loop_control.breaks);
                                remaining = if break_condition == Predicate::False {
                                    remaining
                                } else if break_condition == Predicate::True {
                                    Predicate::False
                                } else {
                                    and_conditions_with_memo(
                                        remaining,
                                        break_condition.negated(),
                                        self.db.predicate_memo().as_ref(),
                                    )
                                };
                            }
                            if remaining != Predicate::False {
                                exit_outcomes.push(
                                    crate::symbolic_local_state::ControlOutcome::new(
                                        TruthCondition::exact_with_memo(
                                            remaining,
                                            self.db.predicate_memo().as_ref(),
                                        ),
                                        self.locals.clone(),
                                    ),
                                );
                            }
                            if !exit_outcomes.is_empty() {
                                self.locals.join_control_outcomes(
                                    &alternative_entry,
                                    &exit_outcomes,
                                    self.db.predicate_memo().as_ref(),
                                );
                            }
                            alternative_outcomes.push(
                                crate::symbolic_local_state::ControlOutcome::new(
                                    alternative.truth.clone(),
                                    self.locals.clone(),
                                ),
                            );
                        }
                        if plan.has_unresolved {
                            let unresolved =
                                crate::symbolic_local_state::ControlOutcome::unresolved_from_changed(
                                    &alternative_entry,
                                    &alternative_outcomes,
                                );
                            alternative_outcomes.push(unresolved);
                        }
                        self.locals.join_control_outcomes(
                            &alternative_entry,
                            &alternative_outcomes,
                            self.db.predicate_memo().as_ref(),
                        );
                        all
                    }
                    None => self.eval_branch_steps(&branch_steps),
                }
            };
            let branch_residual = self.branch_step_residuals(&branch_steps);
            let (later_branches, mut after) = split_escaped(region, branch_residual);
            for (target, mut specs) in later_branches.into_iter().enumerate().skip(index + 1) {
                if let Some(branch) = escaped_per_branch.get_mut(target) {
                    branch.append(&mut specs);
                }
            }
            escaped_after.append(&mut after);
            for entry in adopted {
                let start = entry.view.node.span_start();
                let mut target = 0;
                for (branch_index, branch) in region.branches.iter().enumerate() {
                    if start >= branch.header.end {
                        target = branch_index;
                    }
                }
                if target != index {
                    continue;
                }
                let Some(limit) = entry.view.window.end else {
                    continue;
                };
                let mut chain = Vec::new();
                let mut deferred = Vec::new();
                collect_deferred(
                    entry.view.node,
                    SourceWindow {
                        start: limit,
                        end: entry.defer_window.end,
                    },
                    entry.view.omitted_control,
                    &self.body_facts.adoption_plan,
                    &self.evaluated_parent_shells,
                    &mut chain,
                    &mut deferred,
                );
                let (later_branches, mut after) = split_escaped(region, deferred);
                for (target, mut specs) in later_branches.into_iter().enumerate().skip(index + 1) {
                    if let Some(branch) = escaped_per_branch.get_mut(target) {
                        branch.append(&mut specs);
                    }
                }
                escaped_after.append(&mut after);
            }
            if matches!(arm, ArmSpec::Range { .. }) {
                self.loop_depth -= 1;
                contributions.take_loop_control();
            }
            self.locals.exit_local_scope();
            if matches!(arm, ArmSpec::Range { .. })
                && iterations.is_none()
                && let Some(transition) = &header_transition
            {
                self.locals.widen_changed_fragment_bindings(
                    &transition.after_scope,
                    crate::eval_env::BindingDecision::new(TruthCondition::Unknown),
                );
            }
            // A branch-local reassignment's truthiness holds only where the
            // arm RAN: stamping the arm condition makes the cross-branch
            // union the exact disjunction (the range-sentinel flag pattern:
            // `$found = true` under `if eq .name "…"` inside `range env`
            // joins to the existential `Range(env) ∧ Eq(env.*.name, …)`).
            // An ambient range-key equality concretizes the stamp first, so
            // member wildcards rebind to the named member and the stamped
            // reduction stays encodable (velero's `$breaking` appends under
            // `eq $key "fs-restore-action-config"`).
            let stamped_condition = {
                let concretization = super::assignments::RangeKeyConcretization::from_conjuncts(
                    self.active_predicates.iter().chain([&arm_condition]),
                );
                if concretization.is_empty() {
                    arm_condition.clone()
                } else {
                    concretization.apply(&arm_condition)
                }
            };
            if let Some(semantic_arm_truth) = semantic_arm_truth {
                local_arm_states.push((semantic_arm_truth, self.locals.clone()));
            }
            self.locals.conjoin_changed_truthy_reductions(
                &entry_locals,
                &stamped_condition,
                self.db.predicate_memo().as_ref(),
            );
            outcomes.push(self.locals.clone());
            if entry_root.is_some() {
                root_arm_states.push((
                    arm_condition.clone(),
                    arm_decoded,
                    self.capture_root_set_state(),
                ));
            }

            contributions.extend(extra);
            contributions.guard_all(&arm_condition, self.db.predicate_memo().as_ref());
            out.extend(contributions);
        }

        self.locals = entry_locals.clone();
        self.rewind(entry_scope);
        if let Some(entry_root) = &entry_root {
            self.restore_root_set_state(entry_root);
            self.join_root_set_arms(entry_root, &root_arm_states, has_unconditional_else);
        }
        let binding_fallthrough = header_transition
            .as_ref()
            .map_or(&entry_locals, |transition| &transition.after_scope);
        if promote_body_outcome {
            // A statically nonempty exact range definitely ran its body:
            // bindings written there survive without an entry-state merge.
            outcomes.truncate(1);
        } else if !has_unconditional_else {
            outcomes.push(binding_fallthrough.clone());
        }
        if region.kind == ControlKind::If {
            let common_entry = common_header_fallthrough.as_ref().unwrap_or(&entry_locals);
            self.apply_reassignment_exclusions(
                common_entry,
                &mut outcomes,
                &arm_header_exprs,
                region.span.start,
            );
            self.apply_omission_exclusions(common_entry, &mut outcomes, &arm_header_exprs);
        }
        self.locals.join_branch_outcomes(&entry_locals, &outcomes);
        self.locals
            .join_binding_decisions(binding_fallthrough, &binding_decisions, &outcomes);
        if region.kind == ControlKind::If {
            self.locals.join_truthy_reduction_arms(
                binding_fallthrough,
                &local_arm_states,
                has_unconditional_else,
                self.db.predicate_memo().as_ref(),
            );
            self.locals.join_scalar_dispatch_arms(
                binding_fallthrough,
                &local_arm_states,
                has_unconditional_else,
                self.db.predicate_memo().as_ref(),
            );
        }
        out.loop_control.replace_states(&self.locals);

        // Descendants after the region evaluate outside its branch scope.
        self.add_parent_alternatives(&mut escaped_after);
        if !escaped_after.is_empty() {
            let remaining = out.loop_control.exit_condition().negated();
            let steps = source_ordered_branch_steps(
                escaped_after
                    .into_iter()
                    .map(BranchStep::Deferred)
                    .collect(),
            );
            out.extend(self.eval_branch_steps_with_remaining(&steps, remaining));
        }
        out
    }

    /// Suppresses a definition body while evaluating descendants after its closing action.
    fn eval_suppressed_control_continuation<'n>(
        &mut self,
        region: &'n ControlRegion,
        adopted: &[Adopted<'n>],
        escaped: Vec<DeferredNodes<'n>>,
    ) -> Contributions {
        // A definition that opens a YAML container can make the CST adopt
        // later executable actions beneath that container.
        // The source window, rather than the CST parent, distinguishes the
        // suppressed body from siblings after the closing action, and
        // containers opened inside the definition cannot own those siblings.
        let (_, mut after) = split_escaped(region, escaped);
        for entry in adopted {
            let Some(limit) = entry.view.window.end else {
                continue;
            };
            let mut chain = Vec::new();
            let mut deferred = Vec::new();
            collect_deferred(
                entry.view.node,
                SourceWindow {
                    start: limit,
                    end: entry.defer_window.end,
                },
                entry.view.omitted_control,
                &self.body_facts.adoption_plan,
                &self.evaluated_parent_shells,
                &mut chain,
                &mut deferred,
            );
            let (_, mut continuation) = split_escaped(region, deferred);
            after.append(&mut continuation);
        }
        for spec in &mut after {
            spec.chain
                .retain(|parent| parent.shape.start < region.span.start);
        }
        self.add_parent_alternatives(&mut after);
        let steps = source_ordered_branch_steps(
            after
                .into_iter()
                .map(|spec| {
                    if spec.chain.is_empty() {
                        let views = spec
                            .nodes
                            .into_iter()
                            .map(|node| NodeView {
                                node,
                                window: spec.window,
                                omitted_control: spec.omitted_control,
                                control_cursor: 0,
                            })
                            .collect();
                        BranchStep::Direct(views)
                    } else {
                        BranchStep::Deferred(spec)
                    }
                })
                .collect(),
        );
        self.eval_branch_steps(&steps)
    }

    fn branch_steps<'n>(
        &self,
        region: &'n ControlRegion,
        index: usize,
        adopted: &[Adopted<'n>],
        nodes: Vec<NodeView<'n>>,
        escaped: Vec<DeferredNodes<'n>>,
    ) -> Vec<BranchStep<'n>> {
        if region.kind != ControlKind::If || index == 0 {
            let mut steps = nodes
                .into_iter()
                .map(|node| BranchStep::Direct(vec![node]))
                .collect::<Vec<_>>();
            steps.extend(escaped.into_iter().map(BranchStep::Deferred));
            return source_ordered_branch_steps(steps);
        }
        let Some(candidate) = adopted
            .iter()
            .filter(|candidate| branch_window(region, candidate.view.node.span_start()).0 < index)
            .max_by_key(|candidate| candidate.view.node.span_start())
        else {
            let mut steps = nodes
                .into_iter()
                .map(|node| BranchStep::Direct(vec![node]))
                .collect::<Vec<_>>();
            steps.extend(escaped.into_iter().map(BranchStep::Deferred));
            return source_ordered_branch_steps(steps);
        };
        let Some(shape) = self
            .body_facts
            .adoption_plan
            .parent_shapes
            .get(&candidate.view.node.span_start())
            .copied()
        else {
            let mut steps = nodes
                .into_iter()
                .map(|node| BranchStep::Direct(vec![node]))
                .collect::<Vec<_>>();
            steps.extend(escaped.into_iter().map(BranchStep::Deferred));
            return source_ordered_branch_steps(steps);
        };
        let parent = DeferredParent {
            shape,
            arms: self
                .evaluated_parent_shells
                .get(&shape.start)
                .cloned()
                .unwrap_or_default(),
        };
        let Some(branch) = region.branches.get(index) else {
            let mut steps = nodes
                .into_iter()
                .map(|node| BranchStep::Direct(vec![node]))
                .collect::<Vec<_>>();
            steps.extend(escaped.into_iter().map(BranchStep::Deferred));
            return source_ordered_branch_steps(steps);
        };
        let body_starts = branch
            .body
            .iter()
            .map(Node::span_start)
            .collect::<BTreeSet<_>>();
        let mut steps = Vec::new();
        let mut previous_was_deferred_output = false;
        for view in nodes.iter().copied() {
            // A control with no CST body cannot prove containment by itself.
            // Require one governed sibling to fit the same deferred parent.
            let empty_control_has_parent_evidence = match view.node {
                Node::Control(control)
                    if control.branches.iter().all(|branch| branch.body.is_empty()) =>
                {
                    nodes.iter().any(|candidate| {
                        control.span.start < candidate.node.span_start()
                            && candidate.node.span_start() < control.span.end
                            && self.parent_contains_batch(&parent, &[candidate.node])
                    })
                }
                _ => true,
            };
            let deferred = if matches!(
                view.node,
                Node::Opaque(opaque)
                    if opaque.kind == helm_schema_syntax::OpaqueKind::ActionLineText
            ) {
                previous_was_deferred_output
            } else {
                body_starts.contains(&view.node.span_start())
                    && empty_control_has_parent_evidence
                    && self.parent_contains_batch(&parent, &[view.node])
            };
            previous_was_deferred_output = deferred && matches!(view.node, Node::Output(_));
            match (steps.last_mut(), deferred) {
                (Some(BranchStep::Direct(direct)), false) => direct.push(view),
                (Some(BranchStep::Deferred(spec)), true) => spec.nodes.push(view.node),
                (_, false) => steps.push(BranchStep::Direct(vec![view])),
                (_, true) => {
                    let (_, window) = branch_window(region, view.node.span_start());
                    steps.push(BranchStep::Deferred(DeferredNodes {
                        chain: vec![parent.clone()],
                        resolved_chains: Vec::new(),
                        nodes: vec![view.node],
                        window,
                        defer_end: window.end,
                        omitted_control: Some(region.span.start),
                    }));
                }
            }
        }
        steps.extend(escaped.into_iter().map(BranchStep::Deferred));
        source_ordered_branch_steps(steps)
    }

    fn branch_step_residuals<'n>(&self, steps: &[BranchStep<'n>]) -> Vec<DeferredNodes<'n>> {
        let mut residual = Vec::new();
        for step in steps {
            if let BranchStep::Deferred(spec) = step {
                residual.extend(self.deferred_residual(spec));
            }
        }
        residual
    }

    fn eval_branch_steps(&mut self, steps: &[BranchStep<'_>]) -> Contributions {
        self.eval_branch_steps_with_remaining(steps, Predicate::True)
    }

    fn eval_branch_steps_with_remaining(
        &mut self,
        steps: &[BranchStep<'_>],
        mut remaining: Predicate,
    ) -> Contributions {
        let mut out = Contributions::default();
        for step in steps {
            if remaining == Predicate::False {
                break;
            }
            let local_entry = self.locals.clone();
            let entry_scope = self.mark_scope();
            self.push_predicate(remaining.clone());
            let mut next = match step {
                BranchStep::Direct(nodes) => self.eval_node_list(nodes),
                BranchStep::Deferred(spec) => {
                    let mut spec = spec.clone();
                    self.add_parent_alternatives(std::slice::from_mut(&mut spec));
                    let mut contributions = Contributions::default();
                    self.eval_deferred(&spec, &mut contributions);
                    contributions
                }
            };
            self.rewind(entry_scope);
            if remaining != Predicate::True {
                self.locals.join_control_outcomes(
                    &local_entry,
                    &[
                        crate::symbolic_local_state::ControlOutcome::new(
                            TruthCondition::exact_with_memo(
                                remaining.clone(),
                                self.db.predicate_memo().as_ref(),
                            ),
                            self.locals.clone(),
                        ),
                        crate::symbolic_local_state::ControlOutcome::new(
                            TruthCondition::exact_with_memo(
                                remaining.clone().negated(),
                                self.db.predicate_memo().as_ref(),
                            ),
                            local_entry.clone(),
                        ),
                    ],
                    self.db.predicate_memo().as_ref(),
                );
            }
            let exit_condition = next.loop_control.exit_condition();
            next.guard_all(&remaining, self.db.predicate_memo().as_ref());
            out.extend(next);
            remaining = if exit_condition == Predicate::False {
                remaining
            } else if exit_condition == Predicate::True {
                Predicate::False
            } else {
                and_conditions_with_memo(
                    remaining,
                    exit_condition.negated(),
                    self.db.predicate_memo().as_ref(),
                )
            };
        }
        out
    }

    fn add_parent_alternatives(&self, deferred: &mut [DeferredNodes<'_>]) {
        for spec in deferred {
            spec.resolved_chains = self.resolve_parent_chains(spec);
        }
    }

    fn resolve_parent_chains(&self, spec: &DeferredNodes<'_>) -> Vec<GuardedParentChain> {
        let parents: Vec<DeferredParent> = spec
            .chain
            .iter()
            .take_while(|parent| self.parent_contains_batch(parent, &spec.nodes))
            .cloned()
            .collect();
        let mut pending = vec![(Predicate::True, parents, 0)];
        let mut resolved = Vec::new();
        while let Some((condition, mut parents, cursor)) = pending.pop() {
            let Some(parent) = parents.get(cursor) else {
                resolved.push(GuardedParentChain { condition, parents });
                continue;
            };
            let parent_start = parent.shape.start;
            let (selections, present) = self.parent_selections(&parent.shape, spec);
            if selections.len() == 1
                && selections.first().is_some_and(|selection| {
                    selection.condition == Predicate::True
                        && selection
                            .parents
                            .first()
                            .is_some_and(|parent| parent.shape.start == parent_start)
                })
            {
                if let Some(selection) = selections.into_iter().next()
                    && let Some(selected) = selection.parents.into_iter().next()
                    && let Some(parent) = parents.get_mut(cursor)
                {
                    *parent = selected;
                }
                pending.push((condition, parents, cursor + 1));
                continue;
            }

            let prefix = parents.get(..cursor).unwrap_or_default().to_vec();
            for selection in selections {
                let combined = self
                    .db
                    .predicate_memo()
                    .normalize(Predicate::all(vec![condition.clone(), selection.condition]));
                if combined == Predicate::False {
                    continue;
                }
                let mut selected = prefix.clone();
                selected.extend(
                    selection
                        .parents
                        .into_iter()
                        .take_while(|parent| self.parent_contains_batch(parent, &spec.nodes)),
                );
                let next = (selected.len() > prefix.len()).then_some(prefix.len() + 1);
                pending.push((combined, selected, next.unwrap_or(prefix.len())));
            }
            let absent = self
                .db
                .predicate_memo()
                .normalize(Predicate::all(vec![condition, present.negated()]));
            if absent != Predicate::False {
                let prefix_len = prefix.len();
                pending.push((absent, prefix, prefix_len));
            }
        }
        resolved.sort_by_key(|chain| {
            std::cmp::Reverse(
                chain
                    .parents
                    .iter()
                    .map(|parent| parent.shape.start)
                    .max()
                    .unwrap_or(0),
            )
        });
        resolved
    }

    fn parent_contains_batch(&self, parent: &DeferredParent, nodes: &[&Node]) -> bool {
        let mut rendered = nodes
            .iter()
            .filter(|node| !matches!(node, Node::Comment(_) | Node::Opaque(_)));
        let Some(first) = rendered.next() else {
            return false;
        };
        std::iter::once(first)
            .chain(rendered)
            .all(|node| self.deferred_node_belongs_inside(node, parent))
    }

    fn deferred_node_belongs_inside(&self, node: &Node, parent: &DeferredParent) -> bool {
        let rendered_indent = match node {
            Node::Mapping(entry) => return entry.indent > parent.indent(),
            Node::Sequence(item) => {
                return item.indent > parent.indent()
                    || (item.indent == parent.indent()
                        && parent.shape.kind() == super::eval::ParentKind::Entry
                        && parent.accepts_same_indent());
            }
            Node::Scalar(line) => return line.indent > parent.indent(),
            Node::Control(region) => self.control_render_indent(region.span),
            Node::Output(action) => Some(self.output_render_indent(action.span)),
            Node::Comment(_) | Node::Opaque(_) => return true,
        };
        rendered_indent.is_some_and(|indent| {
            indent > parent.indent()
                || (indent == parent.indent()
                    && parent.shape.kind() == super::eval::ParentKind::Entry
                    && parent.accepts_same_indent())
        })
    }

    fn parent_selections(
        &self,
        shape: &ParentShape,
        spec: &DeferredNodes<'_>,
    ) -> (Vec<ParentSelection>, PathCondition) {
        let plan = &self.body_facts.adoption_plan;
        let candidates = plan.slot_members_through(shape);
        let deferred_start = spec
            .nodes
            .iter()
            .map(|node| node.span_start())
            .min()
            .unwrap_or(usize::MAX);
        let mut evaluated = Vec::new();
        for (index, start) in candidates.iter().copied().enumerate() {
            let Some(arms) = self.evaluated_parent_shells.get(&start) else {
                continue;
            };
            let presence = self.db.predicate_memo().normalize(Predicate::Or(
                arms.iter().map(|arm| arm.condition.clone()).collect(),
            ));
            if presence == Predicate::False {
                continue;
            }
            let end = candidates.get(index + 1).copied().unwrap_or(deferred_start);
            evaluated.push((start, end, presence, arms.clone()));
        }

        let mut selections = Vec::new();
        let mut later = Predicate::False;
        for (start, end, presence, arms) in evaluated.into_iter().rev() {
            let condition = self.db.predicate_memo().normalize(Predicate::all(vec![
                presence.clone(),
                later.clone().negated(),
            ]));
            let mut parents = Vec::new();
            if let Some(shape) = plan.parent_shapes.get(&start).copied() {
                parents.push(DeferredParent { shape, arms });
                for shape in plan.trailing_open_chain(start, end) {
                    let Some(arms) = self.evaluated_parent_shells.get(&shape.start).cloned() else {
                        break;
                    };
                    parents.push(DeferredParent { shape, arms });
                }
            }
            if condition != Predicate::False && !parents.is_empty() {
                selections.push(ParentSelection { condition, parents });
            }
            later = self
                .db
                .predicate_memo()
                .normalize(Predicate::Or(vec![presence, later]));
        }
        selections.reverse();
        (selections, later)
    }

    /// Evaluate one deferred descendant batch within its source window.
    ///
    /// Reattachment follows only parent shells whose headers rendered on the
    /// same path.
    /// Explicitly-indented output keeps floating past containers it does not
    /// render inside.
    fn eval_deferred(&mut self, spec: &DeferredNodes<'_>, out: &mut Contributions) {
        let views: Vec<NodeView<'_>> = spec
            .nodes
            .iter()
            .map(|node| NodeView {
                node,
                window: spec.window,
                omitted_control: spec.omitted_control,
                control_cursor: 0,
            })
            .collect();
        let mut contributions = self.eval_node_list(&views);
        let loop_control = contributions.take_loop_control();
        let chains = if spec.resolved_chains.is_empty() {
            vec![GuardedParentChain {
                condition: Predicate::True,
                parents: spec
                    .chain
                    .iter()
                    .take_while(|parent| self.parent_contains_batch(parent, &spec.nodes))
                    .cloned()
                    .collect(),
            }]
        } else {
            spec.resolved_chains.clone()
        };
        for chain in chains {
            let mut placed = self.place_deferred_under_chain(contributions.clone(), &chain.parents);
            placed.guard_all(&chain.condition, self.db.predicate_memo().as_ref());
            out.extend(placed);
        }
        out.loop_control.extend(loop_control);
    }

    fn deferred_residual<'n>(&self, spec: &DeferredNodes<'n>) -> Vec<DeferredNodes<'n>> {
        let mut residual = Vec::new();
        if let Some(window_end) = spec.window.end
            && spec
                .defer_end
                .is_none_or(|defer_end| window_end < defer_end)
        {
            let residual_window = SourceWindow {
                start: window_end,
                end: spec.defer_end,
            };
            for node in &spec.nodes {
                let mut chain = spec.chain.clone();
                collect_deferred(
                    node,
                    residual_window,
                    spec.omitted_control,
                    &self.body_facts.adoption_plan,
                    &self.evaluated_parent_shells,
                    &mut chain,
                    &mut residual,
                );
            }
        }
        residual
    }

    fn place_deferred_under_chain(
        &mut self,
        mut contributions: Contributions,
        parents: &[DeferredParent],
    ) -> Contributions {
        let mut chain = parents.iter().rev();
        let Some(innermost) = chain.next() else {
            return contributions;
        };
        let mut value = contributions.take_floating_below(
            innermost.indent(),
            innermost.accepts_same_indent(),
            None,
            innermost.shape.established_content_mark,
        );
        let mut floating = std::mem::take(&mut contributions.floating);
        value.extend(contributions.assemble());
        let mut pending = innermost.clone();
        for parent in chain {
            let mut wrapper = Contributions::default();
            (value, floating) = self.wrap_deferred(pending, value, floating);
            wrapper.values = value;
            wrapper.floating = floating;
            let mut parent_value = wrapper.take_floating_below(
                parent.indent(),
                parent.accepts_same_indent(),
                None,
                parent.shape.established_content_mark,
            );
            floating = std::mem::take(&mut wrapper.floating);
            parent_value.extend(wrapper.assemble());
            value = parent_value;
            pending = parent.clone();
        }
        (value, floating) = self.wrap_deferred(pending, value, floating);
        let mut placed = Contributions::default();
        placed.extend_guarded_fragment(value, self.db.predicate_memo().as_ref());
        placed.floating.extend(floating);
        placed
    }

    /// Place deferred content under every live shell of one container level.
    ///
    /// The complement bypasses the container because a descendant may still
    /// render when a conditional parent header does not.
    fn wrap_deferred(
        &mut self,
        parent: DeferredParent,
        value: Guarded<AbstractFragment>,
        floating: Vec<super::eval::FloatingOutput>,
    ) -> (Guarded<AbstractFragment>, Vec<super::eval::FloatingOutput>) {
        let mut grouped = BTreeMap::<usize, Vec<ParentShellArm>>::new();
        for arm in parent.arms {
            grouped.entry(arm.source_start).or_default().push(arm);
        }
        let mut effective = Vec::new();
        let mut present = Predicate::False;
        for arms in grouped.into_values().rev() {
            let group_presence = self.db.predicate_memo().normalize(Predicate::Or(
                arms.iter().map(|arm| arm.condition.clone()).collect(),
            ));
            let available = present.clone().negated();
            for mut arm in arms {
                arm.condition = self
                    .db
                    .predicate_memo()
                    .normalize(Predicate::all(vec![arm.condition, available.clone()]));
                if arm.condition != Predicate::False {
                    effective.push(arm);
                }
            }
            present = self
                .db
                .predicate_memo()
                .normalize(Predicate::Or(vec![group_presence, present]));
        }
        let mut wrapped = Guarded::empty();
        let mut next_floating = Vec::new();
        for arm in effective {
            if !value.is_empty() {
                let mut contribution = Contributions::default();
                match arm.shell {
                    ParentShell::Entry(key) => contribution.merge_entry(key, value.clone()),
                    ParentShell::Item => contribution.items.push(value.clone()),
                }
                let mut assembled = contribution.assemble();
                assembled.guard_all(&arm.condition, self.db.predicate_memo().as_ref());
                wrapped.extend(assembled);
            }
            for mut output in floating.clone() {
                output
                    .value
                    .guard_all(&arm.condition, self.db.predicate_memo().as_ref());
                output
                    .value
                    .arms
                    .retain(|(condition, _)| *condition != Predicate::False);
                if !output.value.is_empty() {
                    next_floating.push(output);
                }
            }
        }
        let absent = self.db.predicate_memo().normalize(present.negated());
        if absent != Predicate::False {
            let mut bypass = value;
            bypass.guard_all(&absent, self.db.predicate_memo().as_ref());
            bypass
                .arms
                .retain(|(condition, _)| *condition != Predicate::False);
            wrapped.extend(bypass);
            for mut output in floating {
                output
                    .value
                    .guard_all(&absent, self.db.predicate_memo().as_ref());
                output
                    .value
                    .arms
                    .retain(|(condition, _)| *condition != Predicate::False);
                if !output.value.is_empty() {
                    next_floating.push(output);
                }
            }
        }
        (wrapped, next_floating)
    }

    fn classify_branch(&self, region: &ControlRegion, index: usize) -> ArmSpec {
        if let Some(arm) = self
            .body_facts
            .control_facts
            .get(&region.span.start)
            .and_then(|facts| facts.arms.get(index))
        {
            return arm.clone();
        }
        match (region.kind, index) {
            (ControlKind::If, 0) => ArmSpec::If(None),
            (ControlKind::With, 0) => ArmSpec::With(None),
            (ControlKind::Range, 0) => ArmSpec::Range {
                header: None,
                destructured: false,
                binding_kind: crate::fragment_assignment::AssignmentKind::Declaration,
                value_variable: None,
                key_variable: None,
            },
            _ => ArmSpec::Else,
        }
    }

    /// Activate one arm: decode its condition, record the condition reads
    /// the current pipeline also records, install dot bindings and range
    /// domains, and return the arm's own condition plus any structural
    /// contributions the arm itself renders (range headers that render list
    /// or mapping content) and the exact iteration plan when the iterable is
    /// statically known.
    fn activate_arm(
        &mut self,
        arm: &ArmSpec,
        nodes: &[NodeView<'_>],
        region_start: usize,
        branch_index: usize,
    ) -> (
        Option<PathCondition>,
        Contributions,
        Option<RangeIterations>,
        SelectionTruthReachability,
        Option<crate::symbolic_local_state::SymbolicLocalState>,
    ) {
        match arm {
            ArmSpec::Else => (
                None,
                Contributions::default(),
                None,
                SelectionTruthReachability::exact_with_memo(
                    Predicate::True,
                    SelectionTruthSource::RawInput,
                    self.db.predicate_memo().as_ref(),
                ),
                None,
            ),
            ArmSpec::If(header) => {
                let (condition, truth) =
                    self.activate_if(header.as_ref(), region_start, branch_index);
                (condition, Contributions::default(), None, truth, None)
            }
            ArmSpec::With(header) => {
                let (condition, truth) =
                    self.activate_with(header.as_ref(), region_start, branch_index);
                (condition, Contributions::default(), None, truth, None)
            }
            ArmSpec::Range {
                header,
                destructured,
                binding_kind,
                value_variable,
                key_variable,
            } => {
                let (condition, contributions, iterations, truth, post_header_locals) = self
                    .activate_range(
                        header.as_ref(),
                        *destructured,
                        *binding_kind,
                        value_variable.as_deref(),
                        key_variable.as_deref(),
                        nodes,
                        region_start,
                    );
                (
                    condition,
                    contributions,
                    iterations,
                    truth,
                    Some(post_header_locals),
                )
            }
        }
    }

    /// A member condition re-decoded with each range variable bound to a
    /// DEFINITELY-ITERATED entry of its overlay iterable (the `set
    /// $services "default" (omit …)` member): that entry iterates on every
    /// render, so a faithful decode under the binding is a sound SUBSET of
    /// "the condition holds for some iteration" — usable only where firing
    /// less often is safe (fail terminals, positive-polarity captures).
    fn definite_member_condition_sound_subset(&mut self, expr: &TemplateExpr) -> Vec<Guard> {
        let mut referenced_variables = BTreeSet::new();
        expr.walk(|candidate| {
            if let TemplateExpr::Variable(variable) = candidate {
                referenced_variables.insert(variable.trim_start_matches('$').to_string());
            }
        });
        let definite: Vec<(String, AbstractValue)> = self
            .locals
            .definite_range_member_values
            .iter()
            .filter(|(variable, _)| referenced_variables.contains(variable.trim_start_matches('$')))
            .map(|(variable, value)| (variable.clone(), value.clone()))
            .collect();
        if definite.is_empty() {
            return Vec::new();
        }
        let mut saved = Vec::new();
        for (variable, value) in definite {
            saved.push((
                variable.clone(),
                self.locals.range_member_values.insert(variable, value),
            ));
        }
        let (predicate, faithful) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(expr),
                context.condition_lowering_is_usable_for_control(expr),
            )
        };
        for (variable, previous) in saved {
            match previous {
                Some(previous) => {
                    self.locals.range_member_values.insert(variable, previous);
                }
                None => {
                    self.locals.range_member_values.remove(&variable);
                }
            }
        }
        if !faithful {
            return Vec::new();
        }
        predicate.contract_guards().unwrap_or_default()
    }

    fn activate_if(
        &mut self,
        header: Option<&TemplateHeader>,
        region_start: usize,
        branch_index: usize,
    ) -> (Option<PathCondition>, SelectionTruthReachability) {
        let Some(header) = header else {
            return (
                None,
                SelectionTruthReachability::unknown(SelectionTruthSource::RawInput),
            );
        };
        let (mut predicate, mut faithful, bound_values, truthiness_abstains) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.condition_lowering_is_usable_for_control(header.expr()),
                context.bound_output_paths_expr(header.expr()),
                context.condition_uses_truthiness_abstention(header.expr()),
            )
        };
        let header_binding = match header.expr() {
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => Some(value.as_ref()),
            _ => None,
        };
        let (helper_paths, evaluated_truth) = if let Some(value) = header_binding {
            let facts = self.control_header_value_facts(value);
            self.eval_assignment_exprs(std::slice::from_ref(header.expr()));
            facts
        } else {
            self.absorb_header_execution_effects(header.expr())
        };
        let evaluated_truth = abstain_truth(evaluated_truth, truthiness_abstains);
        let evaluated_truth_is_unknown = evaluated_truth.when_true().exact_predicate().is_none();
        if let Some(exact) = evaluated_truth.when_true().exact_predicate() {
            predicate = exact;
            faithful = true;
        }
        // A member condition over a local-dict OVERLAY's ranged member is
        // not faithfully decoded by the overlaid path's wildcard members
        // alone: the overlay's own literal entries iterate too (traefik's
        // synthetic "default" service). Binding the definite entry moves
        // such a condition onto DIFFERENT paths, which is how this
        // recognizes one; that binding's decode is then the sound subset the
        // branch below reaches for, and it is what fail terminals and
        // positive-polarity captures need — the wildcard decode alone goes
        // dormant on an empty overlaid map.
        let overlay_entry_subset = if faithful && predicate_reads_member_wildcard(&predicate) {
            let subset = self.definite_member_condition_sound_subset(header.expr());
            let subset_paths: BTreeSet<helm_schema_core::ValuesPath> = subset
                .iter()
                .flat_map(helm_schema_core::Guard::value_paths)
                .collect();
            (!subset.is_empty() && subset_paths != predicate.value_paths()).then_some(subset)
        } else {
            None
        };
        let overlay_entry_is_partial = overlay_entry_subset.is_some();
        let faithful = faithful && !overlay_entry_is_partial;
        if !faithful {
            let marker = format!("{}:{region_start}:{branch_index}", self.source_offset);
            predicate = if truthiness_abstains {
                Predicate::approximate(
                    marker,
                    self.value_path_context()
                        .resolved_values_paths_from_expr(header.expr()),
                )
            } else {
                let evaluated_subset = (!overlay_entry_is_partial)
                    .then(|| evaluated_truth.when_true().proven_selected_subset())
                    .filter(|subset| *subset != Predicate::False);
                self.approximate_arm_predicate(
                    header.expr(),
                    marker,
                    overlay_entry_subset.unwrap_or_default(),
                    evaluated_subset,
                )
            };
        }
        for path in &bound_values {
            self.push_control_read(path, &[]);
        }
        // Helper-body conditions over bound helper calls resolve through the
        // call's summary: its claim paths become guard reads, and when the
        // condition itself decodes nothing they stand in as the arm's truthy
        // conditions (the summary lane's rule for `if include …` headers).
        for path in &helper_paths {
            self.push_control_read(path, &[]);
        }
        if evaluated_truth_is_unknown
            && matches!(predicate.kind(), helm_schema_core::PredicateKind::True)
            && !helper_paths.is_empty()
        {
            predicate = Predicate::all(
                helper_paths
                    .iter()
                    .cloned()
                    .map(Predicate::truthy_path)
                    .collect(),
            );
        }
        // Conjuncts a flat guard cannot spell (a decoded literal-dispatch
        // arm like `¬(a ∨ (b ∧ c))`) stay RAW predicates: the guard
        // flattening DROPS them, and a fail conjunction missing a conjunct
        // negates into states the validator never rejects (datadog's
        // cluster-agent NOTES checks). Row conditions tolerate raw
        // conjuncts — the DNF conversion widens.
        let conjuncts: Vec<Predicate> = match predicate.kind() {
            helm_schema_core::PredicateKind::And(items) => items.to_vec(),
            _ => vec![predicate.clone()],
        };
        for conjunct in conjuncts {
            if let Some(guards) = conjunct.contract_guards() {
                for guard in &guards {
                    for path in guard.value_paths() {
                        self.push_control_read(&path.encode(), std::slice::from_ref(guard));
                    }
                    self.push_predicate(Predicate::from(guard.clone()));
                }
            } else if !matches!(conjunct.kind(), helm_schema_core::PredicateKind::True) {
                for path in conjunct.value_paths() {
                    self.push_control_read(&path.encode(), &[]);
                }
                self.push_predicate(conjunct);
            }
        }
        let semantic_truth = semantic_truth_reachability(
            evaluated_truth,
            &predicate,
            faithful,
            self.db.predicate_memo().as_ref(),
        );
        (Some(predicate), semantic_truth)
    }

    /// The approximate stand-in for an arm condition that could not be
    /// lowered exactly: its sound subset unions the evaluator's partial
    /// when-true condition with the bounded structural subsets, and an
    /// empty union degrades to a bare approximate marker.
    fn approximate_arm_predicate(
        &mut self,
        expr: &TemplateExpr,
        marker: String,
        overlay_entry_subset: Vec<Guard>,
        evaluated_subset: Option<Predicate>,
    ) -> Predicate {
        let mut sound_subset = overlay_entry_subset;
        if sound_subset.is_empty() {
            sound_subset = self.first_iteration_dedup_sound_subset(expr);
        }
        if sound_subset.is_empty() {
            sound_subset = self.definite_member_condition_sound_subset(expr);
        }
        let heuristic_subset = (!sound_subset.is_empty())
            .then(|| Predicate::all(sound_subset.into_iter().map(Predicate::from).collect()));
        let positive_subset = any_predicates_with_memo(
            evaluated_subset
                .into_iter()
                .chain(heuristic_subset)
                .collect(),
            self.db.predicate_memo().as_ref(),
        );
        if positive_subset == Predicate::False {
            self.value_path_context()
                .approximate_condition_predicate_expr(expr, &marker)
        } else {
            let mut paths = self
                .value_path_context()
                .resolved_values_paths_from_expr(expr);
            paths.extend(
                positive_subset
                    .value_paths()
                    .into_iter()
                    .map(|path| path.encode()),
            );
            Predicate::approximate_with_sound_predicate(marker, paths, positive_subset)
        }
    }

    /// A range-body dedup test — `not (hasKey $acc …)` over an accumulator
    /// that is PROVABLY an empty dict at this evaluation point — holds on
    /// the range's first iteration: nothing has been recorded yet. When
    /// the single enclosing loop ranges a resolvable collection, "the
    /// collection has at most one member" makes every iteration the first,
    /// so that size bound is a sound subset of the guard (signoz's
    /// case-folding `additionalEnvs` dedup). Nested loops abstain: a
    /// second loop level reruns the test with a grown accumulator.
    pub(super) fn first_iteration_dedup_sound_subset(&self, expr: &TemplateExpr) -> Vec<Guard> {
        let TemplateExpr::Call { function, args } = expr.deparen() else {
            return Vec::new();
        };
        let [arg] = args.as_slice() else {
            return Vec::new();
        };
        if function != "not" {
            return Vec::new();
        }
        let TemplateExpr::Call {
            function: test,
            args: test_args,
        } = arg.deparen()
        else {
            return Vec::new();
        };
        if test != "hasKey" || test_args.len() != 2 {
            return Vec::new();
        }
        let Some(name_expr) = test_args.first() else {
            return Vec::new();
        };
        let TemplateExpr::Variable(name) = name_expr.deparen() else {
            return Vec::new();
        };
        let accumulator_is_empty = self
            .locals
            .fragment_values
            .get(name.trim_start_matches('$'))
            .and_then(crate::eval_env::LocalBinding::value)
            .is_some_and(|value| {
                matches!(
                    value,
                    crate::abstract_value::AbstractValue::Dict(entries) if entries.is_empty()
                )
            });
        if !accumulator_is_empty || self.loop_depth != 1 {
            return Vec::new();
        }
        let ranged_paths: BTreeSet<&helm_schema_core::ValuesPath> = self
            .active_predicates
            .iter()
            .filter_map(|predicate| match predicate.kind() {
                helm_schema_core::PredicateKind::Guard(Guard::Range { path }) => Some(path),
                _ => None,
            })
            .collect();
        let mut ranged_paths = ranged_paths.into_iter();
        match (ranged_paths.next(), ranged_paths.next()) {
            (Some(path), None) => vec![Guard::AtMostOneMember { path: path.clone() }],
            _ => Vec::new(),
        }
    }

    pub(super) fn activate_with(
        &mut self,
        header: Option<&TemplateHeader>,
        region_start: usize,
        branch_index: usize,
    ) -> (Option<PathCondition>, SelectionTruthReachability) {
        let Some(header) = header else {
            self.push_dot(None, crate::eval_env::BindingEvaluationMode::Evaluated);
            return (
                None,
                SelectionTruthReachability::unknown(SelectionTruthSource::RawInput),
            );
        };
        let (mut predicate, mut faithful, bound_values, dot, truthiness_abstains) = {
            let context = self.value_path_context();
            let predicate = context.with_condition_predicate_expr(header.expr());
            let faithful = context.condition_lowering_is_usable_for_control(header.expr());
            (
                predicate,
                faithful,
                context.bound_output_paths_expr(header.expr()),
                context.with_body_fragment_value_expr(header_range_source(header.expr())),
                context.condition_uses_truthiness_abstention(header.expr()),
            )
        };
        let header_binding = match header.expr() {
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => Some(value.as_ref()),
            _ => None,
        };
        let (helper_paths, evaluated_truth) = if let Some(value) = header_binding {
            let facts = self.control_header_value_facts(value);
            self.eval_assignment_exprs(std::slice::from_ref(header.expr()));
            facts
        } else {
            self.absorb_header_execution_effects(header.expr())
        };
        let evaluated_truth = abstain_truth(evaluated_truth, truthiness_abstains);
        let evaluated_truth_is_unknown = evaluated_truth.when_true().exact_predicate().is_none();
        if let Some(exact) = evaluated_truth.when_true().exact_predicate() {
            predicate = exact;
            faithful = true;
        }
        if evaluated_truth_is_unknown
            && matches!(predicate.kind(), helm_schema_core::PredicateKind::True)
            && !helper_paths.is_empty()
        {
            predicate = Predicate::all(
                helper_paths
                    .iter()
                    .cloned()
                    .map(Predicate::truthy_path)
                    .collect(),
            );
        }
        if !faithful {
            let marker = format!("{}:{region_start}:{branch_index}", self.source_offset);
            let context = self.value_path_context();
            predicate = if truthiness_abstains {
                Predicate::approximate(
                    marker,
                    context.resolved_values_paths_from_expr(header.expr()),
                )
            } else {
                context.approximate_condition_predicate_expr(header.expr(), &marker)
            };
        }
        // The with-predicate is pushed before its reads so the reads carry
        // the `Guard::With` markers, mirroring the current walker. Inexact
        // conjuncts stay raw, the same rule as `if` headers.
        let conjuncts: Vec<Predicate> = match predicate.kind() {
            helm_schema_core::PredicateKind::And(items) => items.to_vec(),
            _ => vec![predicate.clone()],
        };
        for conjunct in conjuncts {
            if let Some(guards) = conjunct.contract_guards() {
                for guard in &guards {
                    self.push_predicate(Predicate::from(guard.clone()));
                }
            } else if !matches!(conjunct.kind(), helm_schema_core::PredicateKind::True) {
                self.push_predicate(conjunct);
            }
        }
        for path in &bound_values {
            self.push_control_read(path, &[]);
        }
        for guard in &predicate.contract_guards().unwrap_or_default() {
            for path in guard.value_paths() {
                self.push_control_read(&path.encode(), &[]);
            }
        }
        self.push_dot(dot, crate::eval_env::BindingEvaluationMode::Evaluated);
        let semantic_truth = semantic_truth_reachability(
            evaluated_truth,
            &predicate,
            faithful,
            self.db.predicate_memo().as_ref(),
        );
        (Some(predicate), semantic_truth)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
    fn activate_range(
        &mut self,
        header: Option<&TemplateHeader>,
        destructured: bool,
        binding_kind: crate::fragment_assignment::AssignmentKind,
        value_variable: Option<&str>,
        key_variable: Option<&str>,
        nodes: &[NodeView<'_>],
        region_start: usize,
    ) -> (
        Option<PathCondition>,
        Contributions,
        Option<RangeIterations>,
        SelectionTruthReachability,
        crate::symbolic_local_state::SymbolicLocalState,
    ) {
        let Some(header) = header else {
            let post_header_locals = self.locals.clone();
            self.push_dot(None, crate::eval_env::BindingEvaluationMode::Direct);
            return (
                None,
                Contributions::default(),
                None,
                SelectionTruthReachability::unknown(SelectionTruthSource::RawInput),
                post_header_locals,
            );
        };
        let range_source = header_range_source(header.expr());
        let range_subject = self.value_path_context().range_subject_expr(range_source);
        let iterable_value = range_subject.value.clone();
        let header_value_variable = value_variable
            .map(str::to_string)
            .or_else(|| helm_schema_ast::range_variable_name_expr(header.expr()));
        let header_binding_variable = match header.expr() {
            TemplateExpr::VariableDefinition { name, .. }
            | TemplateExpr::Assignment { name, .. } => {
                self.eval_assignment_exprs(std::slice::from_ref(header.expr()));
                Some(name.trim_start_matches('$').to_string())
            }
            _ => {
                let _ = self.absorb_header_execution_effects(header.expr());
                None
            }
        };
        if let Some(source) = &header_binding_variable {
            if let Some(target) = &header_value_variable
                && target != source
            {
                self.locals
                    .bind_current_variable_state(binding_kind, source, target.clone());
            }
            if let Some(target) = key_variable
                && target != source
            {
                self.locals
                    .bind_current_variable_state(binding_kind, source, target.to_string());
            }
        } else {
            let value = iterable_value.clone().unwrap_or(AbstractValue::Unknown);
            if let Some(variable) = &header_value_variable {
                self.locals.bind_fragment_value(
                    binding_kind,
                    variable.clone(),
                    Some(value.clone()),
                );
            }
            if let Some(variable) = key_variable {
                self.locals
                    .bind_fragment_value(binding_kind, variable.to_string(), Some(value));
            }
        }
        let post_header_locals = self.locals.clone();
        if binding_kind == crate::fragment_assignment::AssignmentKind::Declaration {
            for variable in header_value_variable
                .as_deref()
                .into_iter()
                .chain(key_variable)
            {
                self.locals.clear_current_binding(variable);
            }
        }
        if let Some((variable, literals)) = parse_literal_list_range_expr(header.expr()) {
            if binding_kind == crate::fragment_assignment::AssignmentKind::Assignment {
                self.locals.range_domains.insert(variable, literals);
            } else {
                self.locals.insert_range_domain(variable, literals);
            }
        } else if let Some(variable) = key_variable
            && let Some(keys) = literal_dict_range_keys(header.expr())
        {
            // `range $k, $v := dict "a" … "b" …` iterates exactly the
            // literal keys: `$k`'s domain makes `get map $k` reads decode
            // to the finite member set.
            if binding_kind == crate::fragment_assignment::AssignmentKind::Assignment {
                self.locals.range_domains.insert(variable.to_string(), keys);
            } else {
                self.locals.insert_range_domain(variable.to_string(), keys);
            }
        }
        let range_is_statically_nonempty = iterable_value
            .as_ref()
            .is_some_and(AbstractValue::definitely_nonempty_iterable);
        let exact_binding_truth = range_subject.truth_reachability.exact_predicate();
        let derived_range_condition = range_subject
            .input_identity
            .is_none()
            .then(|| range_subject.truth_reachability.exact_predicate())
            .flatten();
        let source_paths = range_subject
            .influence_paths
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let member_identity = range_subject.member_identity.clone();
        let direct_path = member_identity
            .as_ref()
            .map(|identity| identity.path.clone());
        let input_identity = range_subject.input_identity.clone();
        let input_identity_path = input_identity
            .as_ref()
            .map(|identity| identity.path.clone());
        let shape = self.range_body_shape(nodes);
        let renders_scalar_items = shape.emits_sequence_items
            && shape.items_all_scalar
            && matches!(
                range_source.deparen(),
                TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
            )
            && input_identity_path.is_some();
        let emit_header_read = destructured || !shape.emits_sequence_items || renders_scalar_items;
        let renders_mapping_entries =
            destructured && !shape.emits_sequence_items && shape.has_dynamic_entries;
        // Member identity is independent of whole-value identity: a merged
        // map can still visit members from one values-backed layer, while a
        // split list carries its source only as influence and has no such
        // member path.
        if let Some(identity) = &member_identity {
            self.observed_facts
                .range_modes
                .mark_member_identity(&identity.path);
            if destructured {
                self.observed_facts
                    .range_modes
                    .mark_destructured(&identity.path);
            }
            if identity.json_decoded {
                self.observed_facts
                    .range_modes
                    .mark_json_decoded(&identity.path);
            }
        }
        if let Some(identity) = &input_identity {
            self.observed_facts
                .range_modes
                .mark_input_identity(&identity.path);
            if destructured {
                self.observed_facts
                    .range_modes
                    .mark_destructured(&identity.path);
            }
            if identity.json_decoded {
                self.observed_facts
                    .range_modes
                    .mark_json_decoded(&identity.path);
            }
        }
        let input_contract_identity = input_identity.as_ref().or_else(|| {
            member_identity.as_ref().filter(|identity| {
                identity
                    .path
                    .segments()
                    .any(helm_schema_core::Segment::is_each_member)
            })
        });
        if iterable_value
            .as_ref()
            .and_then(AbstractValue::selection_chain_identity_paths)
            .is_none()
            && let Some(identity) = input_contract_identity
        {
            let capture = crate::eval_effect::FailCapture {
                conjunction: self.fail_capture_conjunction(Vec::new()),
                ranged: self.capture_ranged_modes(),
                kind: crate::eval_effect::CaptureKind::RangeInput {
                    path: identity.path.clone(),
                    destructured,
                    json_decoded: identity.json_decoded,
                },
            };
            if !capture
                .conjunction
                .iter()
                .any(|predicate| matches!(predicate.kind(), helm_schema_core::PredicateKind::False))
            {
                self.observed_facts.captures.insert(capture);
            }
        }
        self.record_selection_range_captures(iterable_value.as_ref(), destructured);
        let mut own = Vec::new();
        let mut extra = Contributions::default();
        for path in &source_paths {
            let predicate = Predicate::from(Guard::Range { path: path.clone() });
            if emit_header_read && !renders_scalar_items {
                // A helper-scope read carries the range guard only when the
                // range iterates the path ITSELF (or the destructured form):
                // executing the helper executes the header, which aborts on
                // a non-rangeable subject no matter what the body renders,
                // so the iterable claim must not depend on rendered rows —
                // a shared accumulator joining several ranged sources buries
                // those rows' range conjuncts inside `any_of` alternatives
                // (the bitnami `common.images.pullSecrets` shape). A DERIVED
                // iterable's influencing paths keep the bare read: guarding
                // them would recondition strict captures riding the same
                // read identity on rangeability the source never has.
                let direct_range_of_path =
                    destructured || input_identity_path.as_ref() == Some(path);
                if !self.helper_scope || direct_range_of_path {
                    let guard = Guard::Range { path: path.clone() };
                    self.push_control_read(&path.encode(), std::slice::from_ref(&guard));
                } else {
                    self.push_control_read(&path.encode(), &[]);
                }
            }
            if derived_range_condition.is_none() {
                own.push(predicate.clone());
            }
            // A strict call in a guaranteed iteration executes regardless of
            // the values paths that produced the derived iterable. Keep the
            // range guard on rendered rows, but do not let it hide runtime
            // effects that the body necessarily evaluates.
            if !range_is_statically_nonempty && derived_range_condition.is_none() {
                self.push_predicate(predicate);
            }
        }
        if let Some(condition) = derived_range_condition {
            if !range_is_statically_nonempty {
                self.push_predicate(condition.clone());
            }
            own.push(condition);
        }
        if renders_scalar_items {
            for path in &source_paths {
                extra.push_value_arm(splice_arm(
                    &path.encode(),
                    ValueKind::Scalar,
                    self.current_site.as_ref(),
                ));
            }
        }
        if renders_mapping_entries {
            // Templated-key entries render at the body's own entry indent:
            // the fragment attaches to the container that indent opens (the
            // CST can nest a shallow-marker region under a preceding open
            // entry), the same float rule as explicitly-indented output.
            for path in &source_paths {
                let (condition, node) = splice_arm(
                    &path.encode(),
                    ValueKind::Fragment,
                    self.current_site.as_ref(),
                );
                let mut value = super::domain::Guarded::empty();
                value.arms.push((condition, node));
                match shape.dynamic_entry_indent {
                    Some(width) => extra.floating.push(super::eval::FloatingOutput {
                        width,
                        column_only: false,
                        origin: region_start,
                        value,
                    }),
                    None => extra.values.extend(value),
                }
            }
        }
        // Helper bodies iterate statically known list iterables exactly
        // (per-item dots and item-variable bindings); other iterables run
        // the one symbolic iteration with the resolved item dot.
        let iterations = iterable_value.as_ref().and_then(|iterable| {
            Self::exact_range_iterations(
                iterable,
                &range_subject.output_meta,
                range_subject.value_alternatives.as_ref(),
                header,
                value_variable,
                key_variable,
                self.db.predicate_memo().as_ref(),
            )
        });
        let own_condition = Predicate::all(own);
        let truth = SelectionTruthReachability::exact_with_memo(
            exact_binding_truth.unwrap_or_else(|| own_condition.clone()),
            SelectionTruthSource::RawInput,
            self.db.predicate_memo().as_ref(),
        );
        if let Some(iterations) = iterations {
            return (
                Some(own_condition),
                extra,
                Some(iterations),
                truth,
                post_header_locals,
            );
        }
        if let Some(identity) = &member_identity {
            self.active_range_modes.push((
                identity.path.clone(),
                crate::range_modes::RangeMode {
                    member_identity: true,
                    json_decoded: identity.json_decoded,
                    destructured,
                    ..crate::range_modes::RangeMode::default()
                },
            ));
        }
        if let Some(identity) = &input_identity {
            self.active_range_modes.push((
                identity.path.clone(),
                crate::range_modes::RangeMode {
                    input_identity: true,
                    json_decoded: identity.json_decoded,
                    destructured,
                    ..crate::range_modes::RangeMode::default()
                },
            ));
        }
        let range_binding_path = direct_path.clone();
        let dot = range_subject
            .member_value
            .clone()
            .map(|value| value.to_context_value());
        if (self.helper_scope
            || binding_kind == crate::fragment_assignment::AssignmentKind::Assignment)
            && let Some((variable, binding)) =
                helm_schema_ast::range_variable_name_expr(header.expr()).zip(dot.clone())
        {
            self.locals
                .bind_direct_fragment_value(binding_kind, variable, binding);
        }
        // The value binding carries the member identity (`x.*`), while the
        // key binding retains its distinct collection-key provenance. This
        // distinction is required because arrays yield integer keys and maps
        // yield string keys even when their member values have the same shape.
        let member_variable = match value_variable {
            Some(variable) => Some(variable.to_string()),
            None if !destructured => helm_schema_ast::range_variable_name_expr(header.expr()),
            None => None,
        };
        if let Some((variable, binding)) = member_variable.clone().zip(dot.clone()) {
            self.locals.range_member_values.insert(variable, binding);
        }
        // A local dict assembled by an unconditional `set` over a
        // values-backed map iterates its literal entries on EVERY render
        // (`$services := .Values.service.additionalServices` followed by
        // `set $services "default" (omit …)`). One such entry becomes the
        // member variable's DEFINITE binding: unfaithful conditions over
        // the member re-decode under it as a sound subset — traefik's
        // http3 terminal reaches its fail through the always-present
        // "default" service.
        if let Some(AbstractValue::Overlay { entries, .. }) = &iterable_value
            && let Some(variable) = member_variable
            && let Some(entry) = entries.values().next()
        {
            self.locals
                .definite_range_member_values
                .insert(variable, entry.clone());
        }
        if let Some((variable, path)) = key_variable.zip(range_binding_path) {
            self.locals
                .range_member_values
                .insert(variable.to_string(), AbstractValue::RangeKey(path));
        }
        self.push_dot(dot, crate::eval_env::BindingEvaluationMode::Direct);
        (Some(own_condition), extra, None, truth, post_header_locals)
    }

    fn exact_range_iterations(
        iterable: &AbstractValue,
        output_meta: &BTreeMap<helm_schema_core::ValuesPath, crate::helper_meta::HelperOutputMeta>,
        value_alternatives: Option<&crate::value_path_context::RangeValueAlternatives>,
        header: &TemplateHeader,
        value_variable: Option<&str>,
        key_variable: Option<&str>,
        memo: &helm_schema_core::PredicateMemo,
    ) -> Option<RangeIterations> {
        // The proven lane refines the joined value when every alternative is
        // exactly iterable; when it abstains, the joined value still carries
        // the same alternatives as a `Choice`/`FirstTruthy` and iterates them
        // below. Preferring an abstaining proven lane would abandon an exact
        // iteration the value itself supplies. Either way the lane's
        // unresolved remainder is a fact about the subject, not about which
        // lane produced the entries, so it survives the fallback.
        let has_unresolved = value_alternatives.is_some_and(|proven| proven.has_unresolved);
        let proven_alternatives =
            value_alternatives.and_then(|proven| exact_proven_iterations(proven, memo));
        let alternatives = if let Some(alternatives) = proven_alternatives {
            alternatives
        } else {
            match iterable {
                AbstractValue::List(items) => vec![(
                    TruthCondition::exact_with_memo(Predicate::True, memo),
                    items
                        .iter()
                        .enumerate()
                        .map(|(ordinal, item)| {
                            (
                                AbstractValue::StringSet(BTreeSet::from([ordinal.to_string()])),
                                item.clone(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )],
                AbstractValue::Dict(entries) => vec![(
                    TruthCondition::exact_with_memo(Predicate::True, memo),
                    entries
                        .iter()
                        .map(|(key, value)| {
                            (
                                AbstractValue::StringSet(BTreeSet::from([key.clone()])),
                                value.clone(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )],
                AbstractValue::Choice(choices) => {
                    exact_iteration_alternatives(choices.iter(), output_meta, memo)?
                }
                // The selected candidate is one of the statically-known
                // alternatives, so per-alternative exact iteration is the same
                // over-approximation the unordered choice gets.
                AbstractValue::FirstTruthy(candidates) => {
                    exact_iteration_alternatives(candidates.iter(), output_meta, memo)?
                }
                _ => return None,
            }
        };
        // A destructured header binds its declared value variable; a plain
        // `range $x := …` binds `$x` to the successive elements.
        let variable = value_variable
            .map(str::to_string)
            .or_else(|| range_variable_name_expr(header.expr()));
        let alternatives = alternatives
            .into_iter()
            .map(|(truth, items)| {
                let items = items
                    .into_iter()
                    .map(|(key, item)| RangeIterationBinding {
                        dot: item.clone(),
                        variable: variable.as_ref().map(|variable| (variable.clone(), item)),
                        key: key_variable.map(|variable| (variable.to_string(), key)),
                    })
                    .collect();
                RangeIterationAlternative { truth, items }
            })
            .collect();
        Some(RangeIterations {
            alternatives,
            has_unresolved,
            nonempty: iterable.definitely_nonempty_iterable(),
        })
    }

    fn range_body_shape(&mut self, nodes: &[NodeView<'_>]) -> RangeBodyShape {
        let mut shape = RangeBodyShape {
            emits_sequence_items: false,
            items_all_scalar: true,
            has_dynamic_entries: false,
            dynamic_entry_indent: None,
        };
        for view in nodes {
            self.observe_range_body_node(view.node, &mut shape);
        }
        shape
    }

    fn observe_range_body_node(&mut self, node: &Node, shape: &mut RangeBodyShape) {
        match node {
            Node::Sequence(item) => {
                shape.emits_sequence_items = true;
                // Mirrors the scalar-sequence-items rule: only items whose
                // whole content is a non-fragment scalar count (a nested
                // mapping entry, a bare dash, or a fragment-rendering hole
                // disqualifies the body).
                let scalar_item = item.children.is_empty()
                    && (item.block.is_some()
                        || item
                            .value
                            .as_ref()
                            .is_some_and(|value| !self.scalar_parts_render_fragment(value)));
                if !scalar_item {
                    shape.items_all_scalar = false;
                }
                for child in &item.children {
                    self.observe_range_body_node(child, shape);
                }
            }
            Node::Mapping(entry) => {
                if entry
                    .key
                    .parts
                    .iter()
                    .any(|part| matches!(part, ScalarPart::Hole(_)))
                {
                    shape.has_dynamic_entries = true;
                    shape.dynamic_entry_indent.get_or_insert(entry.indent);
                }
                for child in &entry.children {
                    self.observe_range_body_node(child, shape);
                }
            }
            Node::Control(region) => {
                for branch in &region.branches {
                    for child in &branch.body {
                        self.observe_range_body_node(child, shape);
                    }
                }
            }
            Node::Opaque(opaque)
                if opaque.kind == helm_schema_syntax::OpaqueKind::ActionLineText
                    && helm_schema_syntax::structural_mapping_colon(self.text(opaque.span))
                        .is_some() =>
            {
                // `{{ key }}…: value` line shape: a templated mapping entry.
                shape.has_dynamic_entries = true;
                shape
                    .dynamic_entry_indent
                    .get_or_insert(self.dynamic_entry_render_indent(opaque.span));
            }
            _ => {}
        }
    }
}

fn splice_arm(
    path: &str,
    kind: ValueKind,
    site: Option<&std::rc::Rc<super::domain::SiteFacts>>,
) -> (PathCondition, AbstractFragment) {
    (
        Predicate::True,
        AbstractFragment::Splice(Splice {
            values_path: helm_schema_core::ValuesPath::parse(path),
            kind,
            meta: SpliceMeta {
                site: site.cloned(),
                ..SpliceMeta::default()
            },
        }),
    )
}

/// Assign each region branch its body nodes plus the adopted escaped
/// siblings whose spans fall into the branch's source window.
fn branch_node_lists<'nodes>(
    region: &'nodes ControlRegion,
    adopted: &[Adopted<'nodes>],
) -> Vec<Vec<NodeView<'nodes>>> {
    let mut lists: Vec<Vec<NodeView<'nodes>>> = region
        .branches
        .iter()
        .map(|branch| branch.body.iter().map(NodeView::plain).collect())
        .collect();
    for entry in adopted {
        let start = entry.view.node.span_start();
        let (target, _) = branch_window(region, start);
        if let Some(list) = lists.get_mut(target) {
            list.push(entry.view);
        }
    }
    for list in &mut lists {
        list.sort_by_key(|view| view.node.span_start());
    }
    lists
}

/// One source container whose evaluated shells may own a deferred batch.
///
/// Every shell retains its owning predicate, so reattachment can bypass a
/// header that did not render without reevaluating it.
/// A sequence item is a container in its own right: content trailing an
/// ill-nested region that opened the item still renders *inside* that item,
/// so dropping the level would place the batch beside the sequence instead
/// of in it (reloader's `resources:` after the branch-selected `- image:`
/// line).
#[derive(Clone)]
pub(super) struct DeferredParent {
    shape: ParentShape,
    arms: Vec<ParentShellArm>,
}

impl DeferredParent {
    fn indent(&self) -> usize {
        self.shape.indent
    }

    /// Whether output rendered at the container's own indent belongs inside
    /// it. A key opened without inline content does accept it; a sequence
    /// item never does, because its dash occupies that column — output there
    /// opens the NEXT item (zalando's `toYaml .Values.extraEnvs | indent 8`
    /// renders whole `env` entries, not content of the preceding one).
    fn accepts_same_indent(&self) -> bool {
        self.shape.accepts_same_indent
    }
}

/// One batch of deferred descendants in an exact source window.
///
/// The container chain is in document order, outermost first.
#[derive(Clone)]
pub(super) struct DeferredNodes<'n> {
    chain: Vec<DeferredParent>,
    resolved_chains: Vec<GuardedParentChain>,
    nodes: Vec<&'n Node>,
    window: SourceWindow,
    defer_end: Option<usize>,
    omitted_control: Option<usize>,
}

/// Variants preserve source execution order.
/// Deferred changes rendered placement only, not evaluation order.
#[derive(Clone)]
enum BranchStep<'n> {
    Direct(Vec<NodeView<'n>>),
    Deferred(DeferredNodes<'n>),
}

fn source_ordered_branch_steps(mut steps: Vec<BranchStep<'_>>) -> Vec<BranchStep<'_>> {
    steps.sort_by_key(|step| match step {
        BranchStep::Direct(nodes) => nodes
            .first()
            .map_or(usize::MAX, |view| view.node.span_start()),
        BranchStep::Deferred(spec) => spec
            .nodes
            .first()
            .map_or(usize::MAX, |node| node.span_start()),
    });
    let mut ordered = Vec::new();
    for step in steps {
        match (ordered.last_mut(), step) {
            (Some(BranchStep::Direct(current)), BranchStep::Direct(mut next)) => {
                current.append(&mut next);
            }
            (_, step) => ordered.push(step),
        }
    }
    ordered
}

#[derive(Clone)]
struct GuardedParentChain {
    condition: PathCondition,
    parents: Vec<DeferredParent>,
}

struct ParentSelection {
    condition: PathCondition,
    parents: Vec<DeferredParent>,
}

/// Assign escaped batches to branch windows by span: branch `i` owns
/// `[header.end, next header start or region end)`; nodes past the region
/// end re-attach outside the branch scope.
fn split_escaped<'n>(
    region: &ControlRegion,
    escaped: Vec<DeferredNodes<'n>>,
) -> (Vec<Vec<DeferredNodes<'n>>>, Vec<DeferredNodes<'n>>) {
    let mut per_branch: Vec<Vec<DeferredNodes<'n>>> =
        region.branches.iter().map(|_| Vec::new()).collect();
    let mut after = Vec::new();
    for spec in escaped {
        let mut buckets: Vec<Vec<&'n Node>> = region.branches.iter().map(|_| Vec::new()).collect();
        let mut past = Vec::new();
        for node in spec.nodes {
            let start = node.span_start();
            if start >= region.span.end {
                past.push(node);
                continue;
            }
            let mut target = 0;
            for (index, branch) in region.branches.iter().enumerate() {
                if start >= branch.header.end {
                    target = index;
                }
            }
            if let Some(bucket) = buckets.get_mut(target) {
                bucket.push(node);
            }
        }
        for (index, nodes) in buckets.into_iter().enumerate() {
            if let Some(first) = nodes.first()
                && let Some(branch) = per_branch.get_mut(index)
            {
                let (_, branch_source_window) = branch_window(region, first.span_start());
                let window = spec.window.intersect(branch_source_window);
                if window.is_empty() {
                    continue;
                }
                branch.push(DeferredNodes {
                    chain: spec.chain.clone(),
                    resolved_chains: spec.resolved_chains.clone(),
                    nodes,
                    window,
                    defer_end: spec.defer_end,
                    omitted_control: spec.omitted_control,
                });
            }
        }
        if !past.is_empty() {
            let window = spec.window.intersect(SourceWindow {
                start: region.span.end,
                end: spec.window.end,
            });
            after.push(DeferredNodes {
                chain: spec.chain,
                resolved_chains: spec.resolved_chains,
                nodes: past,
                window,
                defer_end: spec.defer_end,
                omitted_control: spec.omitted_control,
            });
        }
    }
    (per_branch, after)
}

/// Collect descendants of an adopted node in a later source window with the
/// mapping-entry chain above them.
///
/// Controls through the omitted boundary own their branch bodies separately
/// and must not be revisited through the adopted chain.
pub(super) fn collect_deferred<'n>(
    node: &'n Node,
    window: SourceWindow,
    omitted_control: Option<usize>,
    plan: &AdoptionPlan,
    parent_shells: &HashMap<usize, Vec<ParentShellArm>>,
    chain: &mut Vec<DeferredParent>,
    out: &mut Vec<DeferredNodes<'n>>,
) {
    if plan
        .content_ends
        .get(&node.span_start())
        .is_some_and(|content_end| *content_end <= window.start)
    {
        return;
    }
    if matches!(node, Node::Control(region)
        if omitted_control.is_some_and(|omitted| region.span.start <= omitted))
    {
        return;
    }
    match node {
        Node::Mapping(entry) => {
            let Some(shape) = plan.parent_shapes.get(&entry.span.start).copied() else {
                return;
            };
            let arms = parent_shells
                .get(&entry.span.start)
                .cloned()
                .unwrap_or_default();
            chain.push(DeferredParent { shape, arms });
            let beyond = plan
                .child_indexes
                .get(&entry.span.start)
                .into_iter()
                .flat_map(|index| index.in_window(window))
                .filter_map(|index| entry.children.get(index))
                .collect::<Vec<_>>();
            if !beyond.is_empty() {
                out.push(DeferredNodes {
                    chain: chain.clone(),
                    resolved_chains: Vec::new(),
                    nodes: beyond,
                    window,
                    defer_end: window.end,
                    omitted_control,
                });
            }
            for child in plan
                .child_indexes
                .get(&entry.span.start)
                .into_iter()
                .flat_map(|index| index.crossing(window.start))
                .filter_map(|index| entry.children.get(index))
            {
                collect_deferred(
                    child,
                    window,
                    omitted_control,
                    plan,
                    parent_shells,
                    chain,
                    out,
                );
            }
            chain.pop();
        }
        Node::Sequence(item) => {
            let Some(shape) = plan.parent_shapes.get(&item.span.start).copied() else {
                return;
            };
            let arms = parent_shells
                .get(&item.span.start)
                .cloned()
                .unwrap_or_default();
            chain.push(DeferredParent { shape, arms });
            let beyond = plan
                .child_indexes
                .get(&item.span.start)
                .into_iter()
                .flat_map(|index| index.in_window(window))
                .filter_map(|index| item.children.get(index))
                .collect::<Vec<_>>();
            if !beyond.is_empty() {
                out.push(DeferredNodes {
                    chain: chain.clone(),
                    resolved_chains: Vec::new(),
                    nodes: beyond,
                    window,
                    defer_end: window.end,
                    omitted_control,
                });
            }
            for child in plan
                .child_indexes
                .get(&item.span.start)
                .into_iter()
                .flat_map(|index| index.crossing(window.start))
                .filter_map(|index| item.children.get(index))
            {
                collect_deferred(
                    child,
                    window,
                    omitted_control,
                    plan,
                    parent_shells,
                    chain,
                    out,
                );
            }
            chain.pop();
        }
        Node::Control(region) => {
            for branch in &region.branches {
                for child in &branch.body {
                    collect_deferred(
                        child,
                        window,
                        omitted_control,
                        plan,
                        parent_shells,
                        chain,
                        out,
                    );
                }
            }
        }
        _ => {}
    }
}

/// Structural range-body shape read off the CST (replacing the line scans
/// the template-tree pipeline uses for the same decisions).
struct RangeBodyShape {
    emits_sequence_items: bool,
    items_all_scalar: bool,
    has_dynamic_entries: bool,
    /// The rendered indent of the body's templated entries (the key hole's
    /// explicit `nindent` width when present, else the line indent).
    dynamic_entry_indent: Option<usize>,
}

/// The header's collection expression, unwrapped from its variable
/// bindings (`range $k, $v := <source>` iterates `<source>`).
fn header_range_source(expr: &TemplateExpr) -> &TemplateExpr {
    let mut source = expr;
    while let TemplateExpr::VariableDefinition { value, .. }
    | TemplateExpr::Assignment { value, .. } = source
    {
        source = value;
    }
    source
}

/// The exact `(key, item)` pairs one alternative iterates.
type IterationEntries = Vec<(AbstractValue, AbstractValue)>;

/// Per-alternative exact iteration entries, each under the truth of the
/// alternative that supplies them.
type IterationAlternatives = Vec<(TruthCondition, IterationEntries)>;

/// Per-alternative exact (key, item) iteration entries for alternatives
/// that are ALL statically-known lists or dicts; any other alternative
/// shape abstains.
fn exact_iteration_entries(alternative: &AbstractValue) -> Option<IterationEntries> {
    match alternative {
        AbstractValue::List(items) => Some(
            items
                .iter()
                .enumerate()
                .map(|(ordinal, item)| {
                    (
                        AbstractValue::StringSet(BTreeSet::from([ordinal.to_string()])),
                        item.clone(),
                    )
                })
                .collect(),
        ),
        AbstractValue::Dict(entries) => Some(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        AbstractValue::StringSet(BTreeSet::from([key.clone()])),
                        value.clone(),
                    )
                })
                .collect(),
        ),
        _ => None,
    }
}

/// Per-alternative exact entries for a range subject whose binding proved
/// its selection conditions.
///
/// Abstains as a whole — including when nothing was proven — so the caller
/// falls back to the joined iterable value. A binding whose selection is not
/// provable still bounds the iterable, and an alternative that is not itself
/// a list or dict (a `Choice` of lists, say) iterates exactly through that
/// joined value.
fn exact_proven_iterations(
    proven: &crate::value_path_context::RangeValueAlternatives,
    memo: &helm_schema_core::PredicateMemo,
) -> Option<IterationAlternatives> {
    if proven.known.is_empty() {
        return None;
    }
    let mut alternatives = Vec::new();
    for alternative in &proven.known {
        let items = exact_iteration_entries(&alternative.value)?;
        alternatives.push((
            TruthCondition::exact_with_memo(alternative.condition.clone(), memo),
            items,
        ));
    }
    Some(alternatives)
}

fn exact_iteration_alternatives<'v>(
    alternatives: impl Iterator<Item = &'v AbstractValue>,
    output_meta: &BTreeMap<helm_schema_core::ValuesPath, crate::helper_meta::HelperOutputMeta>,
    memo: &helm_schema_core::PredicateMemo,
) -> Option<IterationAlternatives> {
    let mut out = Vec::new();
    for alternative in alternatives {
        let truth = exact_iteration_alternative_truth(alternative, output_meta, memo);
        let items = exact_iteration_entries(alternative)?;
        out.push((truth, items));
    }
    Some(out)
}

fn exact_iteration_alternative_truth(
    alternative: &AbstractValue,
    output_meta: &BTreeMap<helm_schema_core::ValuesPath, crate::helper_meta::HelperOutputMeta>,
    memo: &helm_schema_core::PredicateMemo,
) -> TruthCondition {
    let paths = alternative.fragment_rendered_paths();
    if paths.is_empty() {
        return TruthCondition::Unknown;
    }
    let mut metadata = alternative.output_meta();
    for (path, meta) in output_meta {
        metadata.entry(path.clone()).or_default().merge(meta);
    }
    let mut branches = BTreeSet::new();
    for path in paths {
        let Some(meta) = metadata.get(&path) else {
            return TruthCondition::Unknown;
        };
        if meta.predicates.is_empty() {
            return TruthCondition::Unknown;
        }
        branches.extend(meta.predicates.iter().cloned());
    }
    TruthCondition::exact_with_memo(
        predicate_any(
            branches
                .into_iter()
                .map(|branch| Predicate::all(branch.into_iter().collect()))
                .collect(),
        ),
        memo,
    )
}

impl Interpreter<'_> {
    /// An `if` arm that REASSIGNED a local away from its entry `.Values`
    /// identity replaces the raw value on that arm, so the arms that kept
    /// the identity supply it only where the reassigning arm's condition is
    /// false (datadog's `latest` → `1.20.0` version sentinel). Each
    /// kept value gets that exclusion as branch meta before the join:
    /// downstream strict operand captures fire only where the raw value
    /// actually reaches the consumer. The exclusion is carried as an
    /// approximate predicate whose sound subset negates one exactly decoded
    /// equality conjunct of the losing arm's header (`¬E` implies
    /// `¬(… ∧ E)`); with no such conjunct the bare approximation makes
    /// those captures abstain.
    fn apply_reassignment_exclusions(
        &self,
        entry: &crate::symbolic_local_state::SymbolicLocalState,
        outcomes: &mut [crate::symbolic_local_state::SymbolicLocalState],
        header_exprs: &[Option<TemplateExpr>],
        region_start: usize,
    ) {
        let mut entry_identities: Vec<(&String, BTreeSet<helm_schema_core::ValuesPath>)> = entry
            .fragment_values
            .iter()
            .filter_map(|(name, value)| {
                let paths = value.paths();
                (!paths.is_empty()).then_some((name, paths))
            })
            .collect();
        entry_identities.sort_by_key(|(name, _)| name.as_str());
        for (name, entry_paths) in entry_identities {
            let mut exclusions = Vec::new();
            let mut keeping = Vec::new();
            // The union of the divert headers' equality spellings when EVERY
            // identity-losing arm is an exactly explained empty-string fold;
            // one unexplained arm drops the record entirely.
            let mut fold_spellings: Option<BTreeSet<GuardValue>> = Some(BTreeSet::new());
            for (index, outcome) in outcomes.iter().enumerate() {
                let Some(value) = outcome.fragment_values.get(name) else {
                    continue;
                };
                // A reassignment severs the identity when the arm's value
                // lost EVERY entry path: values-independent content (a
                // literal sentinel, derived text) and a switch to another
                // source path (datadog's empty-tag → agent-version
                // fallback) both mean the raw entry value no longer
                // reaches downstream consumers on that arm. A guarded
                // traversal advance INTO a member keeps its own machinery.
                let paths = value.paths();
                let advanced_into_member = paths
                    .iter()
                    .any(|path| entry_paths.iter().any(|entry| path.is_descendant_of(entry)));
                if paths.is_empty() || (paths.is_disjoint(&entry_paths) && !advanced_into_member) {
                    let marker = format!("reassign:{}:{region_start}:{index}", self.source_offset);
                    let header = header_exprs.get(index).and_then(Option::as_ref);
                    exclusions.push(self.reassignment_exclusion(header, marker));
                    fold_spellings = match (
                        fold_spellings,
                        value.value().and_then(|value| {
                            self.empty_fold_spellings(header, name, &value, &entry_paths)
                        }),
                    ) {
                        (Some(mut spellings), Some(arm_spellings)) => {
                            spellings.extend(arm_spellings);
                            Some(spellings)
                        }
                        _ => None,
                    };
                } else if !paths.is_disjoint(&entry_paths) {
                    keeping.push(index);
                }
            }
            if exclusions.is_empty() {
                continue;
            }
            let exclusion: BTreeSet<Predicate> = exclusions.into_iter().collect();
            let fold_spellings = fold_spellings.filter(|spellings| !spellings.is_empty());
            for index in keeping {
                if let Some(outcome) = outcomes.get_mut(index)
                    && let Some(value) = outcome.fragment_values.get(name).cloned()
                {
                    let mut excluded =
                        value.map_values(|value| attach_reassignment_exclusion(&value, &exclusion));
                    if let Some(spellings) = &fold_spellings {
                        excluded = excluded
                            .map_values(|value| attach_empty_fold_spellings(value, spellings));
                    }
                    outcome.fragment_values.insert(name.clone(), excluded);
                }
            }
        }
    }

    /// The exact raw spellings one identity-losing arm diverts to the EMPTY
    /// string: the arm must reassign the local to `""` under a bare
    /// `eq $local <literal>` header whose decode yields only equality
    /// guards on the local's single entry path (the stringified
    /// `if eq $x "<nil>" { $x = "" }` normalization idiom). Anything else
    /// abstains, which makes a downstream `coalesce` rescue refuse the
    /// unexplained empty alternative.
    fn empty_fold_spellings(
        &self,
        header: Option<&TemplateExpr>,
        name: &str,
        value: &AbstractValue,
        entry_paths: &BTreeSet<helm_schema_core::ValuesPath>,
    ) -> Option<BTreeSet<GuardValue>> {
        if !matches!(
            value,
            AbstractValue::StringSet(set) if set.len() == 1 && set.contains("")
        ) {
            return None;
        }
        let TemplateExpr::Call { function, args } = header?.deparen() else {
            return None;
        };
        if function != "eq" || args.len() != 2 {
            return None;
        }
        let local = name.trim_start_matches('$');
        let is_local = |expr: &TemplateExpr| {
            matches!(
                expr.deparen(),
                TemplateExpr::Variable(variable) if variable.trim_start_matches('$') == local
            )
        };
        let is_string_literal = |expr: &TemplateExpr| {
            matches!(
                expr.deparen(),
                TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_))
            )
        };
        let [left, right] = args.as_slice() else {
            return None;
        };
        if !((is_local(left) && is_string_literal(right))
            || (is_string_literal(left) && is_local(right)))
        {
            return None;
        }
        let mut entry_paths = entry_paths.iter();
        let (Some(path), None) = (entry_paths.next(), entry_paths.next()) else {
            return None;
        };
        let predicate = self.value_path_context().condition_predicate_expr(header?);
        let disjuncts = match predicate.kind() {
            helm_schema_core::PredicateKind::Or(items) => items.to_vec(),
            _ => vec![predicate],
        };
        let mut spellings = BTreeSet::new();
        for disjunct in disjuncts {
            let helm_schema_core::PredicateKind::Guard(Guard::Eq {
                path: guard_path,
                value,
            }) = disjunct.kind()
            else {
                return None;
            };
            if guard_path != path {
                return None;
            }
            spellings.insert(value.clone());
        }
        (!spellings.is_empty()).then_some(spellings)
    }

    /// The exclusion predicate for one identity-losing arm: the negation of
    /// its header condition, sound-approximated. The header's `and`
    /// conjuncts decode individually (against the region's ENTRY locals) so
    /// an exact equality sentinel survives beside undecodable siblings; a
    /// single negated equality conjunct is a sound subset of the full
    /// negation. Anything else abstains.
    fn reassignment_exclusion(&self, header: Option<&TemplateExpr>, marker: String) -> Predicate {
        let Some(header) = header else {
            return Predicate::approximate(marker, BTreeSet::new());
        };
        let paths = self
            .value_path_context()
            .resolved_values_paths_from_expr(header);
        let subset = self.header_negation_sound_subset(header);
        if subset.is_empty() {
            Predicate::approximate(marker, paths)
        } else {
            Predicate::approximate_with_sound_subset(marker, paths, subset)
        }
    }

    /// A sound subset of the NEGATION of a branch header: guards that hold
    /// only in states where the header certainly does NOT. One negated
    /// equality conjunct is enough for a conjunction (dropping conjuncts
    /// weakens it, so negating one fires less often than negating all); a
    /// DISJUNCTION needs one negated conjunct per disjunct
    /// (external-secrets' `or (eq … "force") (and (eq … "auto") (include
    /// …))` `OpenShift` gate). Empty means no sound negation was found.
    fn header_negation_sound_subset(&self, header: &TemplateExpr) -> Vec<Guard> {
        if let TemplateExpr::Call { function, args } = header.deparen()
            && function == "or"
        {
            let mut guards = Vec::new();
            for disjunct in args {
                let subset = self.header_negation_sound_subset(disjunct);
                if subset.is_empty() {
                    return Vec::new();
                }
                guards.extend(subset);
            }
            return guards;
        }
        let context = self.value_path_context();
        let conjunct_exprs: Vec<&TemplateExpr> = match header.deparen() {
            TemplateExpr::Call { function, args } if function == "and" => args.iter().collect(),
            other => vec![other],
        };
        for expr in conjunct_exprs {
            if !context.condition_lowering_is_usable_for_control(expr) {
                continue;
            }
            let predicate = context.condition_predicate_expr(expr);
            let conjuncts = match predicate.kind() {
                helm_schema_core::PredicateKind::And(items) => items.to_vec(),
                _ => vec![predicate],
            };
            for conjunct in conjuncts {
                if let helm_schema_core::PredicateKind::Guard(Guard::Eq { path, value }) =
                    conjunct.kind()
                    && !path.encode().starts_with('$')
                    && !path
                        .segments()
                        .any(helm_schema_core::Segment::is_each_member)
                {
                    return vec![Guard::NotEq {
                        path: path.clone(),
                        value: value.clone(),
                    }];
                }
                // A falsiness conjunct (`if not $tag` selecting a fallback)
                // negates to the path's truthiness: the losing arm runs
                // only on falsy values, so the kept raw identity is
                // consumed exactly on the truthy ones (datadog's
                // empty-tag → agent-version fallback).
                if let helm_schema_core::PredicateKind::Not(inner) = conjunct.kind()
                    && let helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) =
                        inner.kind()
                    && !path.encode().starts_with('$')
                    && !path
                        .segments()
                        .any(helm_schema_core::Segment::is_each_member)
                {
                    return vec![Guard::Truthy { path: path.clone() }];
                }
            }
        }
        Vec::new()
    }

    /// Retain guards for keys an arm's `omit` removed from a values-backed
    /// local. Survival of an omitted key is certain exactly where the
    /// omitting arm certainly did not run, so every arm's copy of the
    /// binding carries the key with that negation subset as its RETAIN
    /// guards — the omitting arm's copy included: under the retain guards
    /// that arm never ran, so re-typing the member there is vacuous, and
    /// the uniform map lets the branch join collapse the alternatives.
    /// Conflicting or undecodable retains degrade to the empty guard list,
    /// which downstream reads as "subtract the member's typing, never
    /// re-add it".
    fn apply_omission_exclusions(
        &self,
        entry: &crate::symbolic_local_state::SymbolicLocalState,
        outcomes: &mut [crate::symbolic_local_state::SymbolicLocalState],
        header_exprs: &[Option<TemplateExpr>],
    ) {
        let mut names: Vec<String> = outcomes
            .iter()
            .flat_map(|outcome| outcome.output_meta.keys().cloned())
            .collect();
        names.sort();
        names.dedup();
        for name in names {
            let entry_omitted = binding_omitted_keys(entry.output_meta.get(&name));
            let mut additions: BTreeMap<String, Vec<Guard>> = BTreeMap::new();
            for (index, outcome) in outcomes.iter().enumerate() {
                for (key, retain) in binding_omitted_keys(outcome.output_meta.get(&name)) {
                    if entry_omitted.contains_key(&key) {
                        continue;
                    }
                    let candidate = if retain.is_empty() {
                        header_exprs
                            .get(index)
                            .and_then(Option::as_ref)
                            .map(|header| self.header_negation_sound_subset(header))
                            .unwrap_or_default()
                    } else {
                        retain
                    };
                    additions
                        .entry(key)
                        .and_modify(|existing| {
                            if *existing != candidate {
                                existing.clear();
                            }
                        })
                        .or_insert(candidate);
                }
            }
            if additions.is_empty() {
                continue;
            }
            // The binding-time meta snapshot the lowering prefers for local
            // reads predates this join: it carries the omitting arm's
            // conditions but not the retain guards, and the binding's
            // identity is branch-independent (only the key set varies).
            // Every arm's snapshot gets the post-join truth so the render
            // lowers as one unguarded splice.
            for outcome in outcomes.iter_mut() {
                if let Some(metas) = outcome.output_meta.get_mut(&name) {
                    for meta in metas.values_mut() {
                        meta.predicates.clear();
                        meta.omitted_keys.extend(additions.clone());
                    }
                }
            }
        }
    }
}

/// The omitted-key map recorded for one binding, unioned over its per-path
/// metas; disagreeing retain guards degrade to the empty (abstaining) list.
fn binding_omitted_keys(
    metas: Option<&BTreeMap<helm_schema_core::ValuesPath, crate::helper_meta::HelperOutputMeta>>,
) -> BTreeMap<String, Vec<Guard>> {
    let mut out: BTreeMap<String, Vec<Guard>> = BTreeMap::new();
    for meta in metas.into_iter().flatten().map(|(_, meta)| meta) {
        for (key, retain) in &meta.omitted_keys {
            out.entry(key.clone())
                .and_modify(|existing| {
                    if existing != retain {
                        existing.clear();
                    }
                })
                .or_insert_with(|| retain.clone());
        }
    }
    out
}

/// Records an empty-fold's divert spellings on every kept identity arm;
/// only `eval_coalesce`'s bounded empty rescue reads them.
fn attach_empty_fold_spellings(
    value: AbstractValue,
    spellings: &BTreeSet<GuardValue>,
) -> AbstractValue {
    match value {
        AbstractValue::OutputPath(path, mut meta) => {
            meta.empty_fold_spellings = Some(spellings.clone());
            AbstractValue::OutputPath(path, meta)
        }
        AbstractValue::Choice(choices) => AbstractValue::Choice(
            choices
                .into_iter()
                .map(|choice| attach_empty_fold_spellings(choice, spellings))
                .collect(),
        ),
        AbstractValue::FirstTruthy(candidates) => AbstractValue::FirstTruthy(
            candidates
                .into_iter()
                .map(|candidate| attach_empty_fold_spellings(candidate, spellings))
                .collect(),
        ),
        other => other,
    }
}

fn attach_reassignment_exclusion(
    value: &AbstractValue,
    exclusion: &BTreeSet<Predicate>,
) -> AbstractValue {
    match value {
        AbstractValue::ValuesPath(path) => {
            let mut meta = crate::helper_meta::HelperOutputMeta {
                input_identity: true,
                ..Default::default()
            };
            meta.capture_exclusions.extend(exclusion.iter().cloned());
            AbstractValue::OutputPath(path.clone(), meta)
        }
        AbstractValue::JsonDecodedPath(path) => {
            let mut meta = crate::helper_meta::HelperOutputMeta {
                json_decoded: true,
                ..Default::default()
            };
            meta.capture_exclusions.extend(exclusion.iter().cloned());
            AbstractValue::OutputPath(path.clone(), meta)
        }
        AbstractValue::OutputPath(path, meta) => {
            let mut meta = meta.clone();
            meta.capture_exclusions.extend(exclusion.iter().cloned());
            AbstractValue::OutputPath(path.clone(), meta)
        }
        AbstractValue::Choice(choices) => AbstractValue::Choice(
            choices
                .iter()
                .map(|choice| attach_reassignment_exclusion(choice, exclusion))
                .collect(),
        ),
        AbstractValue::FirstTruthy(candidates) => AbstractValue::FirstTruthy(
            candidates
                .iter()
                .map(|candidate| attach_reassignment_exclusion(candidate, exclusion))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Snapshot of the interpreter's root-context `set` state: live bindings,
/// truthiness predicates, value dispatches, and the summary-exported
/// observed maps.
pub(super) struct RootSetState {
    bindings: HashMap<String, AbstractValue>,
    truthy: HashMap<String, Predicate>,
    dispatches: HashMap<String, ScalarValueDispatch>,
    mutations_observed: BTreeMap<String, AbstractValue>,
    predicates_observed: BTreeMap<String, Predicate>,
    dispatches_observed: BTreeMap<String, ScalarValueDispatch>,
}

impl Interpreter<'_> {
    pub(super) fn capture_root_set_state(&self) -> RootSetState {
        RootSetState {
            bindings: self.root_bindings.clone(),
            truthy: self.root_truthy_predicates.clone(),
            dispatches: self.root_value_dispatches.clone(),
            mutations_observed: self.root_set_mutations_observed.clone(),
            predicates_observed: self.root_set_predicates_observed.clone(),
            dispatches_observed: self.root_value_dispatches_observed.clone(),
        }
    }

    fn restore_root_set_state(&mut self, state: &RootSetState) {
        self.root_bindings = state.bindings.clone();
        self.root_truthy_predicates = state.truthy.clone();
        self.root_value_dispatches = state.dispatches.clone();
        self.root_set_mutations_observed = state.mutations_observed.clone();
        self.root_set_predicates_observed = state.predicates_observed.clone();
        self.root_value_dispatches_observed = state.dispatches_observed.clone();
    }

    /// Join per-arm root `set` outcomes after an if/else region.
    ///
    /// The replay applies each arm's changed keys in source order, matching
    /// the last-write-wins accumulation the pipeline had before the per-arm
    /// entry restore. When the chain is COMPLETE — an unconditional else and
    /// every arm condition decoded without approximation — and every arm
    /// leaves a key holding one scalar string literal, the key additionally
    /// joins into an exact [`ScalarValueDispatch`]: root-field equalities
    /// decode as the disjunction of the arms assigning the compared literal,
    /// and the key's truthiness becomes the disjunction of the arms assigning
    /// a truthy literal.
    fn join_root_set_arms(
        &mut self,
        entry: &RootSetState,
        arms: &[(Predicate, bool, RootSetState)],
        has_unconditional_else: bool,
    ) {
        let mut keys: BTreeSet<String> = BTreeSet::new();
        for (_, _, state) in arms {
            for (key, value) in &state.mutations_observed {
                if entry.mutations_observed.get(key) != Some(value) {
                    keys.insert(key.clone());
                }
            }
        }
        if keys.is_empty() {
            return;
        }
        for (_, _, state) in arms {
            for key in &keys {
                let Some(value) = state.mutations_observed.get(key) else {
                    continue;
                };
                if entry.mutations_observed.get(key) == Some(value) {
                    continue;
                }
                self.root_truthy_predicates.remove(key);
                self.root_set_predicates_observed.remove(key);
                self.root_value_dispatches.remove(key);
                self.root_value_dispatches_observed.remove(key);
                self.root_bindings.insert(key.clone(), value.clone());
                self.root_set_mutations_observed
                    .insert(key.clone(), value.clone());
                if let Some(predicate) = state.predicates_observed.get(key) {
                    self.root_truthy_predicates
                        .insert(key.clone(), predicate.clone());
                    self.root_set_predicates_observed
                        .insert(key.clone(), predicate.clone());
                }
                if let Some(dispatch) = state.dispatches_observed.get(key) {
                    self.root_value_dispatches
                        .insert(key.clone(), dispatch.clone());
                    self.root_value_dispatches_observed
                        .insert(key.clone(), dispatch.clone());
                }
            }
        }
        let complete = has_unconditional_else
            && arms
                .iter()
                .all(|(condition, decoded, _)| *decoded && !condition.contains_approximation());
        if !complete {
            return;
        }
        'keys: for key in &keys {
            let mut dispatch_arms = Vec::new();
            let mut joined_values: BTreeSet<AbstractValue> = BTreeSet::new();
            let mut truthy_conditions = Vec::new();
            for (condition, _, state) in arms {
                let value = state
                    .mutations_observed
                    .get(key)
                    .or_else(|| entry.mutations_observed.get(key));
                let Some(value) = value else {
                    continue 'keys;
                };
                let Some(literal) = root_dispatch_literal(value) else {
                    continue 'keys;
                };
                if guard_value_is_truthy(&literal) {
                    truthy_conditions.push(condition.clone());
                }
                dispatch_arms.push((condition.clone(), ScalarValue::Literal(literal)));
                joined_values.insert(value.clone());
            }
            let truthy = predicate_any(truthy_conditions);
            let joined_value = if joined_values.len() == 1 {
                joined_values
                    .into_iter()
                    .next()
                    .unwrap_or(AbstractValue::Unknown)
            } else {
                AbstractValue::Choice(joined_values)
            };
            self.root_bindings.insert(key.clone(), joined_value.clone());
            self.root_set_mutations_observed
                .insert(key.clone(), joined_value);
            self.root_truthy_predicates
                .insert(key.clone(), truthy.clone());
            self.root_set_predicates_observed
                .insert(key.clone(), truthy);
            let dispatch = ScalarValueDispatch {
                arms: dispatch_arms,
                complete: true,
            };
            self.root_value_dispatches
                .insert(key.clone(), dispatch.clone());
            self.root_value_dispatches_observed
                .insert(key.clone(), dispatch);
        }
    }
}

pub(super) fn semantic_truth_reachability(
    evaluated_truth: SelectionTruthReachability,
    predicate: &Predicate,
    faithful: bool,
    predicate_memo: &helm_schema_core::PredicateMemo,
) -> SelectionTruthReachability {
    if evaluated_truth.when_true().exact_predicate().is_none() && faithful {
        SelectionTruthReachability::exact_with_memo(
            predicate.clone(),
            SelectionTruthSource::RawInput,
            predicate_memo,
        )
    } else {
        evaluated_truth
    }
}

fn abstain_truth(truth: SelectionTruthReachability, abstains: bool) -> SelectionTruthReachability {
    if abstains {
        SelectionTruthReachability::unknown(SelectionTruthSource::RawInput)
    } else {
        truth
    }
}

/// The scalar literal one dispatch arm assigns (`set . "mode" "ha"`); only
/// singleton string literals qualify — anything else keeps the key on the
/// replayed last-write state.
fn root_dispatch_literal(value: &AbstractValue) -> Option<GuardValue> {
    match value {
        AbstractValue::StringSet(values) if values.len() == 1 => {
            values.first().map(GuardValue::string)
        }
        _ => None,
    }
}

/// Whether a decoded condition reads a WILDCARD member path (`x.*.y`): the
/// shape a ranged member's own condition takes.
fn predicate_reads_member_wildcard(predicate: &Predicate) -> bool {
    predicate.value_paths().iter().any(|path| {
        path.segments()
            .any(helm_schema_core::Segment::is_each_member)
    })
}
