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
use crate::eval_effect::RootValueDispatch;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::value_path_context::{guard_value_is_truthy, predicate_any};
use crate::{Guard, ValueKind};
use helm_schema_core::{GuardValue, Predicate};

use super::domain::{AbstractFragment, PathCondition, Splice, SpliceMeta, and_conditions};
use super::eval::{Adopted, ArmSpec, Contributions, Interpreter, NodeView};

/// Exact range sequences resolved from a statically known list iterable.
/// Each alternative preserves one list's item order and bindings.
pub(super) struct RangeIterations {
    pub(super) alternatives: Vec<Vec<RangeIterationBinding>>,
    /// A statically nonempty iterable promotes the body outcome at the
    /// join: bindings set in every iteration survive the region.
    pub(super) nonempty: bool,
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
    pub(super) fn eval_control(
        &mut self,
        region: &ControlRegion,
        adopted: &[Adopted<'_>],
        escaped: Vec<DeferredNodes<'_>>,
    ) -> Contributions {
        if matches!(region.kind, ControlKind::Define | ControlKind::Block) {
            // Define/block bodies render nothing at document scope (they are
            // evaluated when included); escaped siblings adopted into the
            // region are part of that suppressed body.
            return Contributions::default();
        }
        let branch_nodes = branch_node_lists(region, adopted);
        let (escaped_per_branch, escaped_after) = split_escaped(region, escaped);

        let entry_locals = self.locals.clone();
        let entry_predicates = self.active_predicates.len();
        let entry_dots = self.dot_stack.len();
        let entry_ranged = self.active_direct_ranged_paths.len();
        // Root-context `set` state joins across if/else arms like locals:
        // each arm evaluates from the entry state (arms are mutually
        // exclusive at runtime, so one arm's mutation must not leak into a
        // sibling's evaluation), and the outcomes join after the region —
        // complete literal-assignment chains into an exact value dispatch
        // (vault's five-arm `vault.mode`). Non-If regions keep the
        // sequential accumulation.
        let entry_root = (region.kind == ControlKind::If).then(|| self.capture_root_set_state());
        let mut root_arm_states: Vec<(Predicate, bool, RootSetState)> = Vec::new();

        let mut out = Contributions::default();
        let mut outcomes = Vec::new();
        let mut arm_header_exprs: Vec<Option<TemplateExpr>> = Vec::new();
        let mut prior_conditions: Vec<PathCondition> = Vec::new();
        let mut has_unconditional_else = false;
        let mut promote_body_outcome = false;

        for (index, _branch) in region.branches.iter().enumerate() {
            self.locals = entry_locals.clone();
            self.active_predicates.truncate(entry_predicates);
            self.dot_stack.truncate(entry_dots);
            self.active_direct_ranged_paths.truncate(entry_ranged);
            if let Some(entry_root) = &entry_root {
                self.restore_root_set_state(entry_root);
            }

            let mut arm_condition = Predicate::True;
            // Later arms run under the negations of every earlier decoded
            // condition (range alternatives decode no condition, so they
            // stay unguarded like the current pipeline's range joins).
            for prior in &prior_conditions {
                let negated = prior.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions(arm_condition, negated);
            }

            let arm = self.classify_branch(region, index);
            arm_header_exprs.push(match &arm {
                ArmSpec::If(Some(header)) => Some(header.expr().clone()),
                _ => None,
            });
            if matches!(arm, ArmSpec::Else) && index > 0 {
                has_unconditional_else = true;
            }
            let nodes = branch_nodes.get(index).map_or(&[][..], Vec::as_slice);
            // Header reads carry the region's site: the unique resource the
            // region intersects (none when it spans several documents).
            let region_site = self.region_site(region.span);
            let previous_site = std::mem::replace(&mut self.current_site, region_site);
            let (own_condition, extra, iterations) =
                self.activate_arm(&arm, nodes, region.span.start, index);
            self.current_site = previous_site;
            // The value-dispatch join needs mutually exclusive, total arm
            // conditions: an If arm whose header failed to decode (or a
            // with/range arm inside the chain) leaves later negations
            // incomplete.
            let arm_decoded = match &arm {
                ArmSpec::Else => true,
                ArmSpec::If(_) => own_condition.is_some(),
                _ => false,
            };
            if let Some(own) = own_condition {
                arm_condition = and_conditions(arm_condition, own.clone());
                if !matches!(arm, ArmSpec::Range { .. }) {
                    prior_conditions.push(own);
                }
            }
            if index == 0 && iterations.as_ref().is_some_and(|plan| plan.nonempty) {
                promote_body_outcome = true;
            }

            self.locals.enter_local_scope();
            if matches!(arm, ArmSpec::Range { .. }) {
                self.loop_depth += 1;
            }
            let mut contributions = match &iterations {
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
                            .map_or((&[][..], &[][..]), |(first, rest)| (first.as_slice(), rest));
                        (0..first.len())
                            .take_while(|&index| {
                                rest.iter()
                                    .all(|alternative| alternative.get(index) == first.get(index))
                            })
                            .count()
                    } else {
                        usize::MAX
                    };
                    for alternative in &plan.alternatives {
                        self.locals = alternative_entry.clone();
                        let mut remaining = Predicate::True;
                        for (item_index, item) in alternative.iter().enumerate() {
                            if remaining == Predicate::False {
                                break;
                            }
                            if let Some((variable, binding)) = &item.variable {
                                self.locals
                                    .fragment_values
                                    .insert(variable.clone(), binding.clone());
                            }
                            if let Some((variable, ordinal)) = &item.key {
                                self.locals
                                    .fragment_values
                                    .insert(variable.clone(), ordinal.clone());
                            }
                            let entry_predicates = self.active_predicates.len();
                            let entry_capture_approximates =
                                self.alternative_capture_approximates.len();
                            if item_index >= shared_items {
                                self.alternative_capture_approximates
                                    .push(Predicate::approximate(
                                        format!(
                                            "{}:{}:range alternative",
                                            self.source_offset, region.span.start
                                        ),
                                        std::collections::BTreeSet::new(),
                                    ));
                            }
                            self.push_predicate(remaining.clone());
                            self.dot_stack.push(Some(item.dot.clone()));
                            let mut iteration = self.eval_node_list(nodes);
                            self.dot_stack.pop();
                            self.active_predicates.truncate(entry_predicates);
                            self.alternative_capture_approximates
                                .truncate(entry_capture_approximates);
                            let break_condition = iteration.loop_control.break_condition();
                            iteration.take_loop_control();
                            iteration.guard_all(&remaining);
                            all.extend(iteration);
                            remaining = match break_condition {
                                Predicate::False => remaining,
                                Predicate::True => Predicate::False,
                                condition => and_conditions(remaining, condition.negated()),
                            };
                        }
                        alternative_outcomes.push(self.locals.clone());
                    }
                    self.locals = alternative_entry.clone();
                    self.locals
                        .join_branch_outcomes(&alternative_entry, alternative_outcomes);
                    all
                }
                None => self.eval_node_list(nodes),
            };
            // Escaped descendants of earlier siblings whose spans fall in
            // this branch's window evaluate here, re-attached under their
            // parent entry chain, so they carry this arm's condition.
            for spec in escaped_per_branch.get(index).into_iter().flatten() {
                self.eval_deferred(spec.clone(), &mut contributions);
            }
            if matches!(arm, ArmSpec::Range { .. }) {
                self.loop_depth -= 1;
                contributions.take_loop_control();
            }
            self.locals.exit_local_scope();
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
            self.locals
                .conjoin_changed_truthy_reductions(&entry_locals, &stamped_condition);
            outcomes.push(self.locals.clone());
            if entry_root.is_some() {
                root_arm_states.push((
                    arm_condition.clone(),
                    arm_decoded,
                    self.capture_root_set_state(),
                ));
            }

            contributions.extend(extra);
            contributions.guard_all(&arm_condition);
            out.extend(contributions);
        }

        self.locals = entry_locals.clone();
        self.active_predicates.truncate(entry_predicates);
        self.dot_stack.truncate(entry_dots);
        self.active_direct_ranged_paths.truncate(entry_ranged);
        if let Some(entry_root) = &entry_root {
            self.restore_root_set_state(entry_root);
            self.join_root_set_arms(entry_root, root_arm_states, has_unconditional_else);
        }
        if promote_body_outcome {
            // A statically nonempty exact range definitely ran its body:
            // bindings written there survive without an entry-state merge.
            outcomes.truncate(1);
        } else if !has_unconditional_else {
            outcomes.push(entry_locals.clone());
        }
        if region.kind == ControlKind::If {
            self.apply_reassignment_exclusions(
                &entry_locals,
                &mut outcomes,
                &arm_header_exprs,
                region.span.start,
            );
            self.apply_omission_exclusions(&entry_locals, &mut outcomes, &arm_header_exprs);
        }
        self.locals.join_branch_outcomes(&entry_locals, outcomes);

        // Descendants of adopted or escaped nodes that start after the
        // region end evaluate here, outside the branch scope, re-attached
        // under their parent entry chain (source order: they follow
        // `{{ end }}`).
        let mut deferred_specs = escaped_after;
        for entry in adopted {
            let mut chain = Vec::new();
            collect_deferred(
                entry.view.node,
                region.span.end,
                entry.defer_upper,
                &mut chain,
                &mut deferred_specs,
            );
        }
        for spec in deferred_specs {
            self.eval_deferred(spec, &mut out);
        }
        out
    }

    /// Evaluate one deferred descendant batch and re-attach it under its
    /// parent entry chain, letting explicitly-indented output keep floating
    /// past entries it does not render inside.
    fn eval_deferred(&mut self, spec: DeferredNodes<'_>, out: &mut Contributions) {
        let views: Vec<NodeView<'_>> = spec
            .nodes
            .iter()
            .map(|node| NodeView::plain(node))
            .collect();
        // A mapping entry can only contain strictly deeper content: chain
        // entries at or below the deferred nodes' own content indent are
        // structural-recovery artifacts (a trailing region hung under an
        // escaped open entry), not real parents. Control nodes measure by
        // their branch bodies (headers are conventionally unindented).
        let node_indent = spec
            .nodes
            .iter()
            .filter_map(|node| self.structural_content_indent(node))
            .min();
        let chain_entries: Vec<&helm_schema_syntax::MappingEntry> = match node_indent {
            Some(node_indent) => spec
                .chain
                .iter()
                .copied()
                .filter(|entry| entry.indent < node_indent)
                .collect(),
            None => spec.chain.clone(),
        };
        let mut contributions = self.eval_node_list(&views);
        let loop_control = contributions.take_loop_control();
        let mut chain = chain_entries.iter().rev();
        let Some(innermost) = chain.next() else {
            out.extend(contributions);
            return;
        };
        let opened_empty = innermost.value.is_none() && innermost.block.is_none();
        let mut value = contributions.take_floating_below(innermost.indent, opened_empty, None);
        let mut floating = std::mem::take(&mut contributions.floating);
        value.extend(contributions.assemble());
        let mut key = self.entry_key(&innermost.key);
        for parent in chain {
            let mut wrapper = Contributions::default();
            wrapper.merge_entry(key, value);
            wrapper.floating = floating;
            let opened_empty = parent.value.is_none() && parent.block.is_none();
            let mut parent_value = wrapper.take_floating_below(parent.indent, opened_empty, None);
            floating = std::mem::take(&mut wrapper.floating);
            parent_value.extend(wrapper.assemble());
            value = parent_value;
            key = self.entry_key(&parent.key);
        }
        let mut re_attached = Contributions::default();
        re_attached.merge_entry(key, value);
        re_attached.floating = floating;
        re_attached.loop_control = loop_control;
        out.extend(re_attached);
    }

    fn classify_branch(&self, region: &ControlRegion, index: usize) -> ArmSpec {
        if index == 0 {
            let facts = self.body_facts.control_facts.get(&region.span.start);
            return match region.kind {
                ControlKind::If => ArmSpec::If(facts.and_then(|facts| facts.header.clone())),
                ControlKind::With => ArmSpec::With(facts.and_then(|facts| facts.header.clone())),
                ControlKind::Range => ArmSpec::Range {
                    header: facts.and_then(|facts| facts.header.clone()),
                    destructured: facts.is_some_and(|facts| facts.range_destructured),
                    value_variable: facts.and_then(|facts| facts.range_value_variable.clone()),
                    key_variable: facts.and_then(|facts| facts.range_key_variable.clone()),
                },
                ControlKind::Define | ControlKind::Block => ArmSpec::Else,
            };
        }
        let header_text = region
            .branches
            .get(index)
            .map_or("", |branch| self.text(branch.header));
        parse_else_header(header_text)
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
    ) {
        match arm {
            ArmSpec::Else => (None, Contributions::default(), None),
            ArmSpec::If(header) => (
                self.activate_if(header.as_ref(), region_start, branch_index),
                Contributions::default(),
                None,
            ),
            ArmSpec::With(header) => (
                self.activate_with(header.as_ref(), region_start, branch_index),
                Contributions::default(),
                None,
            ),
            ArmSpec::Range {
                header,
                destructured,
                value_variable,
                key_variable,
            } => self.activate_range(
                header.as_ref(),
                *destructured,
                value_variable.as_deref(),
                key_variable.as_deref(),
                nodes,
                region_start,
            ),
        }
    }

    /// A member condition re-decoded with each range variable bound to a
    /// DEFINITELY-ITERATED entry of its overlay iterable (the `set
    /// $services "default" (omit …)` member): that entry iterates on every
    /// render, so a faithful decode under the binding is a sound SUBSET of
    /// "the condition holds for some iteration" — usable only where firing
    /// less often is safe (fail terminals, positive-polarity captures).
    fn definite_member_condition_sound_subset(&mut self, expr: &TemplateExpr) -> Vec<Guard> {
        let definite: Vec<(String, AbstractValue)> = self
            .locals
            .definite_range_member_values
            .iter()
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
                context.condition_lowering_is_faithful(expr),
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
        if !faithful || !predicate.contract_guards_are_exact() {
            return Vec::new();
        }
        predicate.contract_guards()
    }

    /// Absorb a truthy⇒string fail capture for each path: a condition's
    /// string consumer fails template evaluation when the raw value is
    /// present (truthy) but not a string. Ambient guards join through the
    /// same absorption the helper-body `fail` lane uses.
    pub(super) fn absorb_condition_string_captures(&mut self, paths: &BTreeSet<String>) {
        let captures: Vec<crate::eval_effect::FailCapture> = paths
            .iter()
            .map(|path| {
                let mut conjunction = Vec::new();
                if !helm_schema_core::split_value_path(path)
                    .iter()
                    .any(|segment| segment == "*")
                {
                    conjunction.insert(0, Predicate::truthy_path(path.clone()));
                }
                // An approximately-lowered enclosing condition gates when
                // this consumer runs at all: the ambient predicates the
                // absorption prepends carry it as an `Approximate` conjunct,
                // so the implication abstains instead of binding a branch
                // whose real guard the encoding cannot represent.
                crate::eval_effect::FailCapture {
                    conjunction,
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::ValueType {
                        path: path.clone(),
                        schema_type: "string".to_string(),
                    },
                }
            })
            .collect();
        self.absorb_helper_fails(&captures);
    }

    fn activate_if(
        &mut self,
        header: Option<&TemplateHeader>,
        region_start: usize,
        branch_index: usize,
    ) -> Option<PathCondition> {
        let header = header?;
        let (mut predicate, faithful, bound_values) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.condition_lowering_is_faithful(header.expr()),
                context.bound_output_paths_expr(header.expr()),
            )
        };
        let helper_paths = self.absorb_header_execution_effects(header.expr());
        if !faithful {
            let marker = format!("{}:{region_start}:{branch_index}", self.source_offset);
            let mut sound_subset = self.first_iteration_dedup_sound_subset(header.expr());
            if sound_subset.is_empty() {
                sound_subset = self.definite_member_condition_sound_subset(header.expr());
            }
            predicate = if sound_subset.is_empty() {
                self.value_path_context()
                    .approximate_condition_predicate_expr(header.expr(), &marker)
            } else {
                let paths = self
                    .value_path_context()
                    .resolved_values_paths_from_expr(header.expr());
                Predicate::approximate_with_sound_subset(marker, paths, sound_subset)
            };
        }
        for path in &bound_values {
            self.push_read(path, &[]);
        }
        // Helper-body conditions over bound helper calls resolve through the
        // call's summary: its claim paths become guard reads, and when the
        // condition itself decodes nothing they stand in as the arm's truthy
        // conditions (the summary lane's rule for `if include …` headers).
        for path in &helper_paths {
            self.push_read(path, &[]);
        }
        if predicate.is_trivial() && !helper_paths.is_empty() {
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
        let conjuncts: Vec<Predicate> = match &predicate {
            Predicate::And(items) => items.clone(),
            other => vec![other.clone()],
        };
        for conjunct in conjuncts {
            if conjunct.contract_guards_are_exact() {
                for guard in &conjunct.contract_guards() {
                    for path in guard.value_paths() {
                        self.push_read(path, std::slice::from_ref(guard));
                    }
                    self.push_predicate(Predicate::from(guard.clone()));
                }
            } else if !matches!(conjunct, Predicate::True) {
                for path in conjunct.value_paths() {
                    self.push_read(&path, &[]);
                }
                self.push_predicate(conjunct);
            }
        }
        Some(predicate)
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
        if function != "not" || args.len() != 1 {
            return Vec::new();
        }
        let TemplateExpr::Call {
            function: test,
            args: test_args,
        } = args[0].deparen()
        else {
            return Vec::new();
        };
        if test != "hasKey" || test_args.len() != 2 {
            return Vec::new();
        }
        let TemplateExpr::Variable(name) = test_args[0].deparen() else {
            return Vec::new();
        };
        let accumulator_is_empty = self
            .locals
            .fragment_values
            .get(name.trim_start_matches('$'))
            .is_some_and(|value| {
                matches!(
                    value,
                    crate::abstract_value::AbstractValue::Dict(entries) if entries.is_empty()
                )
            });
        if !accumulator_is_empty || self.loop_depth != 1 {
            return Vec::new();
        }
        let ranged_paths: BTreeSet<&String> = self
            .active_predicates
            .iter()
            .filter_map(|predicate| match predicate {
                Predicate::Guard(Guard::Range { path }) => Some(path),
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
    ) -> Option<PathCondition> {
        let Some(header) = header else {
            self.dot_stack.push(None);
            return None;
        };
        let (mut predicate, faithful, bound_values, dot) = {
            let context = self.value_path_context();
            // Helper bodies decode `with` like `if` (truthy conditions), the
            // shape the summary lane always produced: a helper row's
            // self-condition must stay a positive header for the signal
            // builder, where document rows carry the with-marker flavor.
            let predicate = if self.helper_scope {
                context.condition_predicate_expr(header.expr())
            } else {
                context.with_condition_predicate_expr(header.expr())
            };
            let faithful = context.condition_lowering_is_faithful(header.expr());
            (
                predicate,
                faithful,
                context.bound_output_paths_expr(header.expr()),
                context.with_body_fragment_value_expr(header.expr()),
            )
        };
        let helper_paths = self.absorb_header_execution_effects(header.expr());
        if predicate.is_trivial() && !helper_paths.is_empty() {
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
            predicate = self
                .value_path_context()
                .approximate_condition_predicate_expr(header.expr(), &marker);
        }
        // The with-predicate is pushed before its reads so the reads carry
        // the `Guard::With` markers, mirroring the current walker. Inexact
        // conjuncts stay raw, the same rule as `if` headers.
        let conjuncts: Vec<Predicate> = match &predicate {
            Predicate::And(items) => items.clone(),
            other => vec![other.clone()],
        };
        for conjunct in conjuncts {
            if conjunct.contract_guards_are_exact() {
                for guard in &conjunct.contract_guards() {
                    self.push_predicate(Predicate::from(guard.clone()));
                }
            } else if !matches!(conjunct, Predicate::True) {
                self.push_predicate(conjunct);
            }
        }
        for path in &bound_values {
            self.push_read(path, &[]);
        }
        for guard in &predicate.contract_guards() {
            for path in guard.value_paths() {
                self.push_read(path, &[]);
            }
        }
        if let TemplateExpr::VariableDefinition { name, .. } = header.expr()
            && let Some(binding) = dot.as_ref()
        {
            self.locals
                .fragment_values
                .insert(name.trim_start_matches('$').to_string(), binding.clone());
        }
        self.dot_stack.push(dot);
        Some(predicate)
    }

    fn activate_range(
        &mut self,
        header: Option<&TemplateHeader>,
        destructured: bool,
        value_variable: Option<&str>,
        key_variable: Option<&str>,
        nodes: &[NodeView<'_>],
        region_start: usize,
    ) -> (
        Option<PathCondition>,
        Contributions,
        Option<RangeIterations>,
    ) {
        let Some(header) = header else {
            self.dot_stack.push(None);
            return (None, Contributions::default(), None);
        };
        if let Some((variable, literals)) = parse_literal_list_range_expr(header.expr()) {
            self.locals.insert_range_domain(variable, literals);
        } else if let Some(variable) = key_variable
            && let Some(keys) = literal_dict_range_keys(header.expr())
        {
            // `range $k, $v := dict "a" … "b" …` iterates exactly the
            // literal keys: `$k`'s domain makes `get map $k` reads decode
            // to the finite member set.
            self.locals.insert_range_domain(variable.to_string(), keys);
        }
        self.absorb_header_execution_effects(header.expr());
        let iterable_value = self.range_iterable_fragment_value(header);
        let range_is_statically_nonempty = iterable_value
            .as_ref()
            .is_some_and(AbstractValue::definitely_nonempty_iterable);
        let range_source = header_range_source(header.expr());
        let derived_range_condition = match range_source.deparen() {
            TemplateExpr::Variable(name)
                if self
                    .locals
                    .fragment_values
                    .get(name.trim_start_matches('$'))
                    .is_some_and(|value| {
                        !matches!(
                            value,
                            AbstractValue::ValuesPath(_)
                                | AbstractValue::JsonDecodedPath(_)
                                | AbstractValue::OutputPath(_, _)
                        )
                    }) =>
            {
                self.locals
                    .truthy_reductions
                    .get(name.trim_start_matches('$'))
                    .cloned()
            }
            _ => None,
        };
        let (
            source_paths,
            direct_path,
            direct_variable_path,
            identity_variable_path,
            json_decoded_path,
        ) = {
            let context = self.value_path_context();
            let direct_path = context.single_direct_iterable_range_path_expr(range_source);
            let json_decoded_direct_path =
                context.single_direct_json_decoded_range_path_expr(range_source);
            // A range over a VARIABLE holding a single member identity
            // (`range $values` where `$values` is one member of an outer
            // ranged map) iterates that member directly; the fn above
            // only sees literal selectors. The binding must be the path's
            // IDENTITY: a variable holding derived data (`$namespaces :=
            // splitList "," .Values.x`) iterates the derivation, and
            // stamping the iterable domain on the influencing path would
            // reject the string the split actually consumes.
            let direct_variable_path = match (&direct_path, range_source) {
                (None, TemplateExpr::Variable(name)) => context
                    .template_bindings
                    .get(name)
                    .cloned()
                    .and_then(AbstractValue::without_widened)
                    .map(|value| value.paths())
                    .filter(|paths| paths.len() == 1)
                    .and_then(|paths| paths.into_iter().next()),
                _ => None,
            };
            // The read-guard lane below needs the strictly-IDENTITY form: a
            // fallback-selected binding (`$crs := .Values.x | default
            // list`) also exposes exactly one path, but the range iterates
            // the FALLBACK on every Helm-falsy input, so the path itself
            // owes no iterable shape (datadog's orchestrator
            // custom-resources list). Member-identity marking above stays
            // on the permissive form — fail captures keyed on the ranged
            // member must survive the fallback wrapper (nats' jsonpatch).
            let identity_variable_path = match (&direct_path, range_source) {
                (None, TemplateExpr::Variable(name)) => context
                    .template_bindings
                    .get(name)
                    .cloned()
                    .and_then(AbstractValue::without_widened)
                    .and_then(|value| match value {
                        AbstractValue::ValuesPath(path)
                        | AbstractValue::JsonDecodedPath(path)
                        | AbstractValue::OutputPath(path, _) => Some(path),
                        _ => None,
                    }),
                _ => None,
            };
            let json_decoded_variable_path = match (&direct_path, range_source) {
                (None, TemplateExpr::Variable(name)) => context
                    .template_bindings
                    .get(name)
                    .and_then(AbstractValue::unique_json_decoded_path),
                _ => None,
            };
            (
                context
                    .resolved_values_paths_from_expr(header.expr())
                    .into_iter()
                    .collect::<Vec<_>>(),
                direct_path,
                direct_variable_path,
                identity_variable_path,
                json_decoded_direct_path.or(json_decoded_variable_path),
            )
        };
        // A bare-dot range (`range .`) resolves through the dot VALUE,
        // which may be a derived collection merely INFLUENCED by one path
        // (kyverno's labels-merge list of rendered fragments): only an
        // identity dot ranges the path itself, so only that form may carry
        // the read guard below.
        let bare_dot_range =
            matches!(range_source.deparen(), TemplateExpr::Field(fields) if fields.is_empty());
        let dot_is_identity_of = |path: &str| {
            matches!(
                &iterable_value,
                Some(
                    AbstractValue::ValuesPath(value_path)
                        | AbstractValue::JsonDecodedPath(value_path)
                        | AbstractValue::OutputPath(value_path, _)
                ) if value_path == path
            )
        };
        let shape = self.range_body_shape(nodes);
        let renders_scalar_items =
            shape.emits_sequence_items && shape.items_all_scalar && direct_path.is_some();
        let emit_header_read = destructured || !shape.emits_sequence_items || renders_scalar_items;
        let renders_mapping_entries =
            destructured && !shape.emits_sequence_items && shape.has_dynamic_entries;
        // Structural claims about the ranged path hold only when the range
        // iterates the path ITSELF: `range until (int .Values.n)` iterates a
        // DERIVED list, so it says nothing about the path's own shape. The
        // same discipline gates the bare dot — `range .` over a derived
        // collection merely influenced by one path (kyverno's labels-merge
        // list of rendered fragments) must not mark that path ranged.
        if let Some(path) = &direct_path
            && (!bare_dot_range || dot_is_identity_of(path))
        {
            self.range_modes.mark_direct(path);
            if destructured {
                self.range_modes.mark_destructured(path);
            }
        }
        if let Some(path) = &direct_variable_path {
            self.range_modes.mark_direct(path);
            if destructured {
                self.range_modes.mark_destructured(path);
            }
        }
        if let Some(path) = &json_decoded_path {
            self.range_modes.mark_json_decoded(path);
        }
        // A range over a first-truthy selection chain (`with A | default B`
        // dots, `range (A | default B)`) iterates the SELECTED candidate,
        // so each identity candidate owes an iterable shape exactly on its
        // selected states: `truthy(A) ⇒ iterable(A)`, and `¬truthy(A) ∧
        // truthy(B) ⇒ iterable(B)`. The own-truthiness conjunct keeps the
        // last candidate sound too (`default` selects a falsy fallback
        // verbatim, but a falsy scalar there is the accepted-widening
        // direction, never a false rejection), and the prior negations keep
        // a truthy scalar beside a selected collection accepted (kyverno's
        // per-controller `imagePullSecrets | default global`). The claim
        // rides the fail-capture lane: it is header-abort evidence, and a
        // read row would be absorbed into the wider co-sited with-header
        // read at canonicalization.
        let selection_chain = iterable_value
            .as_ref()
            .and_then(AbstractValue::selection_chain_identity_paths);
        if let Some(chain) = &selection_chain {
            let mut prior_falsy: Vec<Predicate> = Vec::new();
            for path in chain {
                let mut tail = prior_falsy.clone();
                tail.push(Predicate::truthy_path(path.clone()));
                let capture = crate::eval_effect::FailCapture {
                    conjunction: self.fail_capture_conjunction(tail),
                    ranged: self.capture_ranged_modes(),
                    kind: crate::eval_effect::CaptureKind::RangeSelection {
                        path: path.clone(),
                        chain: chain.clone(),
                        allow_integer: !destructured,
                    },
                };
                if !capture
                    .conjunction
                    .iter()
                    .any(|p| matches!(p, Predicate::False))
                    && !self.fail_conditions.contains(&capture)
                {
                    self.fail_conditions.push(capture);
                }
                prior_falsy.push(Predicate::truthy_path(path.clone()).negated());
            }
        }
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
                let identity_range_of_path = direct_path.as_deref() == Some(path.as_str())
                    && (!bare_dot_range || dot_is_identity_of(path));
                let direct_range_of_path = destructured
                    || identity_range_of_path
                    || identity_variable_path.as_deref() == Some(path.as_str());
                if !self.helper_scope || direct_range_of_path {
                    let guard = Guard::Range { path: path.clone() };
                    self.push_read(path, std::slice::from_ref(&guard));
                } else {
                    self.push_read(path, &[]);
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
                extra.push_value_arm(splice_arm(path, ValueKind::Scalar, &self.current_site));
            }
        }
        if renders_mapping_entries {
            // Templated-key entries render at the body's own entry indent:
            // the fragment attaches to the container that indent opens (the
            // CST can nest a shallow-marker region under a preceding open
            // entry), the same float rule as explicitly-indented output.
            for path in &source_paths {
                let (condition, node) = splice_arm(path, ValueKind::Fragment, &self.current_site);
                let mut value = super::domain::Guarded::empty();
                value.arms.push((condition, node));
                match shape.dynamic_entry_indent {
                    Some(width) => extra.floating.push(super::eval::FloatingOutput {
                        width,
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
        let iterations = self.exact_range_iterations(header, value_variable, key_variable);
        if let Some(iterations) = iterations {
            return (Some(Predicate::all(own)), extra, Some(iterations));
        }
        if let Some(path) = direct_path.as_ref().or(direct_variable_path.as_ref()) {
            self.active_direct_ranged_paths.push(path.clone());
        }
        let range_binding_path = direct_path
            .as_ref()
            .or(direct_variable_path.as_ref())
            .cloned();
        let mut dot = direct_path
            .as_ref()
            .or(direct_variable_path.as_ref())
            .map(|path| {
                let member_path = helm_schema_core::append_value_path(path, "*");
                if json_decoded_path.as_ref() == Some(path) {
                    AbstractValue::JsonDecodedPath(member_path)
                } else {
                    AbstractValue::ValuesPath(member_path)
                }
            });
        if self.helper_scope {
            let item_dot = iterable_value
                .as_ref()
                .and_then(AbstractValue::fragment_range_item)
                .map(|binding| binding.to_context_value());
            if item_dot.is_some() {
                dot = item_dot;
            }
            if let Some((variable, binding)) =
                helm_schema_ast::range_variable_name_expr(header.expr()).zip(dot.clone())
            {
                self.locals.fragment_values.insert(variable, binding);
            }
        }
        // Ranging a structured iterable outside helper scope binds the item
        // dot to the iterable's member domain: a derived keys list yields
        // its collection key (`range keys m` at a template site keeps a
        // same-map `pluck` member read exact), and a constructed or joined
        // list yields the union of its item alternatives (airflow's
        // `range $workerSet := $workerSets` over a conditional
        // default-set concat). An item that still carries the ITERABLE's
        // own identity (`fragment_range_item` keeps a non-decoded
        // OutputPath whole for influence) projects to its member here, so
        // the binding never claims the collection renders where its
        // members do.
        if dot.is_none() {
            fn item_member_identity(item: AbstractValue) -> AbstractValue {
                match item {
                    AbstractValue::OutputPath(path, meta) if !meta.json_decoded => {
                        AbstractValue::OutputPath(
                            helm_schema_core::append_value_path(&path, "*"),
                            meta,
                        )
                    }
                    AbstractValue::Choice(choices) => AbstractValue::Choice(
                        choices.into_iter().map(item_member_identity).collect(),
                    ),
                    other => other,
                }
            }
            dot = iterable_value
                .as_ref()
                .and_then(AbstractValue::fragment_range_item)
                .map(|item| item_member_identity(item).to_context_value());
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
        self.dot_stack.push(dot);
        (Some(Predicate::all(own)), extra, None)
    }

    // (header_range_source lives at module scope below.)

    /// The iterable's fragment value, for the helper-scope range model.
    fn range_iterable_fragment_value(&mut self, header: &TemplateHeader) -> Option<AbstractValue> {
        let value_expr = match header.expr().deparen() {
            helm_schema_ast::TemplateExpr::VariableDefinition { value, .. }
            | helm_schema_ast::TemplateExpr::Assignment { value, .. } => value.as_ref(),
            expr => expr,
        };
        let mut seen = self.helper_seen.clone();
        let current_dot = self.current_dot_fragment();
        FragmentEvalContext::new(self.db).fragment_value_from_expr(
            value_expr,
            &self.locals.fragment_values,
            current_dot.as_ref(),
            &mut seen,
        )
    }

    fn exact_range_iterations(
        &mut self,
        header: &TemplateHeader,
        value_variable: Option<&str>,
        key_variable: Option<&str>,
    ) -> Option<RangeIterations> {
        let iterable = self.range_iterable_fragment_value(header)?;
        let alternatives = match &iterable {
            AbstractValue::List(items) => vec![
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
            ],
            AbstractValue::Dict(entries) => vec![
                entries
                    .iter()
                    .map(|(key, value)| {
                        (
                            AbstractValue::StringSet(BTreeSet::from([key.clone()])),
                            value.clone(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ],
            AbstractValue::Choice(choices) => exact_iteration_alternatives(choices.iter())?,
            // The selected candidate is one of the statically-known
            // alternatives, so per-alternative exact iteration is the same
            // over-approximation the unordered choice gets.
            AbstractValue::FirstTruthy(candidates) => {
                exact_iteration_alternatives(candidates.iter())?
            }
            _ => return None,
        };
        // A destructured header binds its declared value variable; a plain
        // `range $x := …` binds `$x` to the successive elements.
        let variable = value_variable
            .map(str::to_string)
            .or_else(|| range_variable_name_expr(header.expr()));
        let alternatives = alternatives
            .into_iter()
            .map(|items| {
                items
                    .into_iter()
                    .map(|(key, item)| RangeIterationBinding {
                        dot: item.clone(),
                        variable: variable.as_ref().map(|variable| (variable.clone(), item)),
                        key: key_variable.map(|variable| (variable.to_string(), key)),
                    })
                    .collect()
            })
            .collect();
        Some(RangeIterations {
            alternatives,
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
    site: &Option<std::rc::Rc<super::domain::SiteFacts>>,
) -> (PathCondition, AbstractFragment) {
    (
        Predicate::True,
        AbstractFragment::Splice(Splice {
            values_path: path.to_string(),
            kind,
            meta: SpliceMeta {
                site: site.clone(),
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
        let mut target = 0;
        for (index, branch) in region.branches.iter().enumerate() {
            if start >= branch.header.end {
                target = index;
            }
        }
        if let Some(list) = lists.get_mut(target) {
            list.push(entry.view);
        }
    }
    for list in &mut lists {
        list.sort_by_key(|view| view.node.span_start());
    }
    lists
}

/// Classify an `{{ else … }}` branch header. The header span is a single
/// isolated action token, so keyword classification here is a narrow local
/// check; condition text still goes through the typed header parser.
fn parse_else_header(text: &str) -> ArmSpec {
    let mut inner = text.trim();
    if let Some(rest) = inner.strip_prefix("{{") {
        inner = rest.trim_start_matches('-').trim();
    }
    if let Some(rest) = inner.strip_suffix("}}") {
        inner = rest.trim_end_matches('-').trim();
    }
    if inner == "else" {
        return ArmSpec::Else;
    }
    let Some(rest) = inner.strip_prefix("else") else {
        return ArmSpec::Else;
    };
    let rest = rest.trim_start();
    if let Some(condition) = rest.strip_prefix("if ") {
        return ArmSpec::If(Some(TemplateHeader::parse_control(format!(
            "if {condition}"
        ))));
    }
    if let Some(condition) = rest.strip_prefix("with ") {
        return ArmSpec::With(Some(TemplateHeader::parse_control(format!(
            "with {condition}"
        ))));
    }
    ArmSpec::Else
}

/// One batch of deferred descendants: the entry chain they nest under (in
/// document order, outermost first) and the deferred nodes themselves.
#[derive(Clone)]
pub(super) struct DeferredNodes<'n> {
    chain: Vec<&'n helm_schema_syntax::MappingEntry>,
    nodes: Vec<&'n Node>,
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
            if !nodes.is_empty()
                && let Some(branch) = per_branch.get_mut(index)
            {
                branch.push(DeferredNodes {
                    chain: spec.chain.clone(),
                    nodes,
                });
            }
        }
        if !past.is_empty() {
            after.push(DeferredNodes {
                chain: spec.chain,
                nodes: past,
            });
        }
    }
    (per_branch, after)
}

/// Collect descendants of an adopted node whose spans start at or beyond the
/// adopting region's end (and below the enclosing bound, which the enclosing
/// region's own deferral handles), with the mapping-entry chain above them.
pub(super) fn collect_deferred<'n>(
    node: &'n Node,
    limit: usize,
    upper: Option<usize>,
    chain: &mut Vec<&'n helm_schema_syntax::MappingEntry>,
    out: &mut Vec<DeferredNodes<'n>>,
) {
    let in_window = |child: &Node| {
        let start = child.span_start();
        start >= limit && upper.is_none_or(|upper| start < upper)
    };
    match node {
        Node::Mapping(entry) => {
            chain.push(entry);
            let beyond: Vec<&Node> = entry.children.iter().filter(|c| in_window(c)).collect();
            if !beyond.is_empty() {
                out.push(DeferredNodes {
                    chain: chain.clone(),
                    nodes: beyond,
                });
            }
            for child in &entry.children {
                if child.span_start() < limit {
                    collect_deferred(child, limit, upper, chain, out);
                }
            }
            chain.pop();
        }
        Node::Sequence(item) => {
            let beyond: Vec<&Node> = item.children.iter().filter(|c| in_window(c)).collect();
            if !beyond.is_empty() {
                out.push(DeferredNodes {
                    chain: chain.clone(),
                    nodes: beyond,
                });
            }
            for child in &item.children {
                if child.span_start() < limit {
                    collect_deferred(child, limit, upper, chain, out);
                }
            }
        }
        Node::Control(region) => {
            for branch in &region.branches {
                for child in &branch.body {
                    collect_deferred(child, limit, upper, chain, out);
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

/// Per-alternative exact (key, item) iteration entries for alternatives
/// that are ALL statically-known lists or dicts; any other alternative
/// shape abstains.
fn exact_iteration_alternatives<'v>(
    alternatives: impl Iterator<Item = &'v AbstractValue>,
) -> Option<Vec<Vec<(AbstractValue, AbstractValue)>>> {
    let mut out = Vec::new();
    for alternative in alternatives {
        match alternative {
            AbstractValue::List(items) => out.push(
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
            AbstractValue::Dict(entries) => out.push(
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
            _ => return None,
        }
    }
    Some(out)
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
        let mut entry_identities: Vec<(&String, BTreeSet<String>)> = entry
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
                let advanced_into_member = paths.iter().any(|path| {
                    entry_paths
                        .iter()
                        .any(|entry| helm_schema_core::values_path_is_descendant(path, entry))
                });
                if paths.is_empty() || (paths.is_disjoint(&entry_paths) && !advanced_into_member) {
                    let marker = format!("reassign:{}:{region_start}:{index}", self.source_offset);
                    let header = header_exprs.get(index).and_then(Option::as_ref);
                    exclusions.push(self.reassignment_exclusion(header, marker));
                    fold_spellings = match (
                        fold_spellings,
                        self.empty_fold_spellings(header, name, value, &entry_paths),
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
                if let Some(value) = outcomes[index].fragment_values.get(name) {
                    let mut excluded = attach_reassignment_exclusion(value, &exclusion);
                    if let Some(spellings) = &fold_spellings {
                        excluded = attach_empty_fold_spellings(excluded, spellings);
                    }
                    outcomes[index]
                        .fragment_values
                        .insert(name.clone(), excluded);
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
        entry_paths: &BTreeSet<String>,
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
        if !((is_local(&args[0]) && is_string_literal(&args[1]))
            || (is_string_literal(&args[0]) && is_local(&args[1])))
        {
            return None;
        }
        let mut entry_paths = entry_paths.iter();
        let (Some(path), None) = (entry_paths.next(), entry_paths.next()) else {
            return None;
        };
        let predicate = self.value_path_context().condition_predicate_expr(header?);
        let disjuncts = match predicate {
            Predicate::Or(items) => items,
            other => vec![other],
        };
        let mut spellings = BTreeSet::new();
        for disjunct in disjuncts {
            let Predicate::Guard(Guard::Eq {
                path: guard_path,
                value,
            }) = disjunct
            else {
                return None;
            };
            if guard_path != *path {
                return None;
            }
            spellings.insert(value);
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
    /// …))` OpenShift gate). Empty means no sound negation was found.
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
            if !context.condition_lowering_is_faithful(expr) {
                continue;
            }
            let conjuncts = match context.condition_predicate_expr(expr) {
                Predicate::And(items) => items,
                other => vec![other],
            };
            for conjunct in conjuncts {
                if let Predicate::Guard(Guard::Eq { path, value }) = &conjunct
                    && !path.starts_with('$')
                    && !path.split('.').any(|part| part == "*")
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
                if let Predicate::Not(inner) = &conjunct
                    && let Predicate::Guard(Guard::Truthy { path }) = inner.as_ref()
                    && !path.starts_with('$')
                    && !path.split('.').any(|part| part == "*")
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
    metas: Option<&BTreeMap<String, crate::helper_meta::HelperOutputMeta>>,
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
            let mut meta = crate::helper_meta::HelperOutputMeta::default();
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
    dispatches: HashMap<String, RootValueDispatch>,
    mutations_observed: BTreeMap<String, AbstractValue>,
    predicates_observed: BTreeMap<String, Predicate>,
    dispatches_observed: BTreeMap<String, RootValueDispatch>,
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
    /// leaves a key holding one scalar literal, the key additionally joins
    /// into an exact [`RootValueDispatch`]: root-field equalities decode as
    /// the disjunction of the arms assigning the compared literal, and the
    /// key's truthiness becomes the disjunction of the arms assigning a
    /// truthy literal.
    fn join_root_set_arms(
        &mut self,
        entry: &RootSetState,
        arms: Vec<(Predicate, bool, RootSetState)>,
        has_unconditional_else: bool,
    ) {
        let mut keys: BTreeSet<String> = BTreeSet::new();
        for (_, _, state) in &arms {
            for (key, value) in &state.mutations_observed {
                if entry.mutations_observed.get(key) != Some(value) {
                    keys.insert(key.clone());
                }
            }
        }
        if keys.is_empty() {
            return;
        }
        for (_, _, state) in &arms {
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
            for (condition, _, state) in &arms {
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
                dispatch_arms.push((condition.clone(), literal));
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
            let dispatch = RootValueDispatch {
                arms: dispatch_arms,
            };
            self.root_value_dispatches
                .insert(key.clone(), dispatch.clone());
            self.root_value_dispatches_observed
                .insert(key.clone(), dispatch);
        }
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
