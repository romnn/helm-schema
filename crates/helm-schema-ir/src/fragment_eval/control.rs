//! Control-region evaluation: each branch's contributions are evaluated
//! under the branch's decoded condition (plus the negations of prior arms)
//! and dissolve into the surrounding container as guarded arms. Local
//! bindings join across branches with the same rules as the symbolic
//! walker.

use std::collections::BTreeSet;

use helm_schema_ast::{TemplateExpr, TemplateHeader, range_variable_name_expr};
use helm_schema_syntax::{ControlKind, ControlRegion, Node, ScalarPart};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{literal_dict_range_keys, parse_literal_list_range_expr};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::{Guard, ValueKind};
use helm_schema_core::Predicate;

use super::domain::{AbstractFragment, PathCondition, Splice, SpliceMeta, and_conditions};
use super::eval::{Adopted, ArmSpec, Contributions, Interpreter, NodeView};

/// Exact range iterations resolved from a statically known list iterable
/// (helper scope): each item supplies the iteration's dot and, for
/// `range $item := …` headers, the item variable binding.
pub(super) struct RangeIterations {
    pub(super) items: Vec<RangeIterationBinding>,
    /// A statically nonempty iterable promotes the body outcome at the
    /// join: bindings set in every iteration survive the region.
    pub(super) nonempty: bool,
}

pub(super) struct RangeIterationBinding {
    pub(super) dot: AbstractValue,
    pub(super) variable: Option<(String, AbstractValue)>,
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
        let entry_approximate = self.approximate_condition_paths.len();
        let entry_ranged = self.active_direct_ranged_paths.len();

        let mut out = Contributions::default();
        let mut outcomes = Vec::new();
        let mut prior_conditions: Vec<PathCondition> = Vec::new();
        let mut has_unconditional_else = false;
        let mut promote_body_outcome = false;
        let mut prior_approximate_paths: Vec<String> = Vec::new();

        for (index, _branch) in region.branches.iter().enumerate() {
            self.locals = entry_locals.clone();
            self.active_predicates.truncate(entry_predicates);
            self.dot_stack.truncate(entry_dots);
            self.approximate_condition_paths.truncate(entry_approximate);
            self.active_direct_ranged_paths.truncate(entry_ranged);
            // An arm under the negation of an approximately-lowered prior
            // is approximate on the same paths.
            self.approximate_condition_paths
                .extend(prior_approximate_paths.iter().cloned());

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
            if matches!(arm, ArmSpec::Else) && index > 0 {
                has_unconditional_else = true;
            }
            let nodes = branch_nodes.get(index).map_or(&[][..], Vec::as_slice);
            // Header reads carry the region's site: the unique resource the
            // region intersects (none when it spans several documents).
            let region_site = self.region_site(region.span);
            let previous_site = std::mem::replace(&mut self.current_site, region_site);
            let arm_entry_approximate = self.approximate_condition_paths.len();
            let (own_condition, extra, iterations) =
                self.activate_arm(&arm, nodes, region.span.start);
            prior_approximate_paths.extend(
                self.approximate_condition_paths[arm_entry_approximate..]
                    .iter()
                    .cloned(),
            );
            self.current_site = previous_site;
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
            let mut contributions = match &iterations {
                Some(plan) => {
                    // Exact iterations share one local scope (bindings
                    // accumulate across items, like the sequential loop the
                    // template runs); each item installs its own dot.
                    let mut all = Contributions::default();
                    for item in &plan.items {
                        if let Some((variable, binding)) = &item.variable {
                            self.locals
                                .fragment_values
                                .insert(variable.clone(), binding.clone());
                        }
                        self.dot_stack.push(Some(item.dot.clone()));
                        all.extend(self.eval_node_list(nodes));
                        self.dot_stack.pop();
                    }
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
            self.locals.exit_local_scope();
            outcomes.push(self.locals.clone());

            contributions.extend(extra);
            contributions.guard_all(&arm_condition);
            out.extend(contributions);
        }

        self.locals = entry_locals.clone();
        self.active_predicates.truncate(entry_predicates);
        self.dot_stack.truncate(entry_dots);
        self.approximate_condition_paths.truncate(entry_approximate);
        self.active_direct_ranged_paths.truncate(entry_ranged);
        if promote_body_outcome {
            // A statically nonempty exact range definitely ran its body:
            // bindings written there survive without an entry-state merge.
            outcomes.truncate(1);
        } else if !has_unconditional_else {
            outcomes.push(entry_locals.clone());
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
    ) -> (
        Option<PathCondition>,
        Contributions,
        Option<RangeIterations>,
    ) {
        match arm {
            ArmSpec::Else => (None, Contributions::default(), None),
            ArmSpec::If(header) => (
                self.activate_if(header.as_ref()),
                Contributions::default(),
                None,
            ),
            ArmSpec::With(header) => (
                self.activate_with(header.as_ref()),
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

    /// Absorb a truthy⇒string fail capture for each path: a condition's
    /// string consumer fails template evaluation when the raw value is
    /// present (truthy) but not a string. Ambient guards join through the
    /// same absorption the helper-body `fail` lane uses.
    pub(super) fn absorb_condition_string_captures(&mut self, paths: &BTreeSet<String>) {
        let captures: Vec<crate::eval_effect::FailCapture> = paths
            .iter()
            .map(|path| crate::eval_effect::FailCapture {
                conjunction: vec![
                    Predicate::truthy_path(path.clone()),
                    Predicate::from(crate::Guard::TypeIs {
                        path: path.clone(),
                        schema_type: "string".to_string(),
                    })
                    .negated(),
                ],
                // An approximately-lowered enclosing condition gates when
                // this consumer runs at all: the capture carries it so the
                // implication abstains instead of binding a branch whose
                // real guard the encoding cannot represent (F64).
                approximate_condition_paths: self
                    .approximate_condition_paths
                    .iter()
                    .cloned()
                    .collect(),
                direct_ranged_paths: BTreeSet::new(),
                member_access: false,
            })
            .collect();
        self.absorb_helper_fails(&captures);
    }

    fn activate_if(&mut self, header: Option<&TemplateHeader>) -> Option<PathCondition> {
        let header = header?;
        let (mut predicate, faithful, bound_values, transform_facts) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.condition_lowering_is_faithful(header.expr()),
                context.bound_output_paths_expr(header.expr()),
                context.condition_transform_facts(header.expr()),
            )
        };
        // A string consumer whose subject passes through `default`
        // (`semverCompare ">=1.19" (.Values.kubeVersion | default …)`)
        // sees the raw value only when it is truthy: a conditional
        // contract that binds at condition-EVALUATION time, so it is
        // absorbed before this header's own fidelity sentinel — only
        // ENCLOSING approximations can gate whether evaluation happens.
        self.absorb_condition_string_captures(&transform_facts.defaulted_string_contracts);
        // Structural accessor contracts the header expression records on
        // its own (`dig`'s intermediate-map requirement) bind the same
        // way: the expression evaluates whenever control reaches the
        // header.
        let header_captures = self
            .value_path_context()
            .expression_fail_captures(header.expr());
        self.absorb_helper_fails(&header_captures);
        // A string-consuming call in the condition (`regexMatch`, `replace`,
        // …) fails template evaluation for non-string subjects: that is a
        // runtime string contract, exactly like a rendered `trunc`. Under
        // ambient predicates the row lanes only hint; the truthy⇒string
        // capture carries the enforceable arm through the same fail
        // machinery the defaulted form uses.
        if !self.active_predicates.is_empty() {
            self.absorb_condition_string_captures(&transform_facts.string_contracts.clone());
        }
        if !faithful {
            let paths = self
                .value_path_context()
                .resolved_values_paths_from_expr(header.expr());
            if paths.is_empty() {
                // An undecodable condition with no resolvable paths could
                // gate ANYTHING; the empty marker poisons fail negation
                // globally under it.
                self.approximate_condition_paths.push(String::new());
            }
            self.approximate_condition_paths.extend(paths);
        }
        for path in &bound_values {
            self.push_read(path, &[]);
        }
        // A total conversion in the condition
        // (`eq (.Values.x | toString) "true"`) renders any input, exactly
        // like the same conversion in a `set` expression or render hole.
        self.shape_erased_paths.extend(transform_facts.shape_erased);
        for path in transform_facts.string_contracts {
            let sink = if self.hint_scope_is_unconditional(&path) {
                &mut self.type_hints
            } else {
                &mut self.guarded_type_hints
            };
            sink.entry(path.clone())
                .or_default()
                .insert("string".to_string());
            if self.approximate_condition_paths.is_empty() {
                self.string_contract_paths.insert(path);
            }
        }
        // Helper-body conditions over bound helper calls resolve through the
        // call's summary: its claim paths become guard reads, and when the
        // condition itself decodes nothing they stand in as the arm's truthy
        // conditions (the summary lane's rule for `if include …` headers).
        let helper_paths = self.helper_condition_claim_paths(header.expr());
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
            } else if !conjunct.is_trivial() {
                for path in conjunct.value_paths() {
                    self.push_read(&path, &[]);
                }
                self.push_predicate(conjunct);
            }
        }
        Some(predicate)
    }

    /// The most-specific claim paths of bound helper calls inside a
    /// condition (empty for conditions without resolvable calls).
    fn helper_condition_claim_paths(
        &mut self,
        expr: &helm_schema_ast::TemplateExpr,
    ) -> std::collections::BTreeSet<String> {
        if !expr_contains_bound_helper_call(expr, self.db) {
            return std::collections::BTreeSet::new();
        }
        let hole = self.eval_hole_exprs_for_condition(expr);
        for path in &hole.effects.parsed_yaml_input_paths {
            self.type_hints
                .entry(path.clone())
                .or_default()
                .insert("string".to_string());
        }
        // Total conversions and string contracts observed inside the called
        // helper hold regardless of where the call sits.
        self.shape_erased_paths
            .extend(hole.effects.shape_erased_paths.iter().cloned());
        if self.approximate_condition_paths.is_empty() {
            self.string_contract_paths
                .extend(hole.effects.string_contract_paths.iter().cloned());
        }
        // The helper's own guarded reads carry its type-dispatch facts
        // (`kindIs "string" .Values.x` arms prove the chart handles that
        // kind), and its `fail` captures its rejected complement: both
        // hold wherever the condition is EVALUATED, so they absorb here
        // exactly like at a value-position call.
        let suppressed: std::collections::BTreeSet<&String> = hole
            .effects
            .helper_rendered
            .iter()
            .flat_map(|row| row.meta.suppress_predicate_paths.iter())
            .chain(hole.effects.helper_suppressed_paths.iter())
            .collect();
        let claims: std::collections::BTreeSet<String> = hole
            .effects
            .helper_reads
            .iter()
            .map(|read| read.values_path.clone())
            .collect();
        self.absorb_helper_reads_with_suppression(&hole.effects.helper_reads, &suppressed, &claims);
        self.absorb_helper_fails(&hole.effects.helper_fails);
        let mut paths: std::collections::BTreeSet<String> = hole
            .effects
            .helper_reads
            .iter()
            .map(|read| read.values_path.clone())
            .collect();
        paths.extend(
            hole.effects
                .helper_rendered
                .iter()
                .map(|row| row.path.clone()),
        );
        paths.extend(hole.effects.type_hints.keys().cloned());
        paths
            .iter()
            .filter(|path| !helm_schema_core::values_path_has_descendant(path, &paths))
            .cloned()
            .collect()
    }

    fn activate_with(&mut self, header: Option<&TemplateHeader>) -> Option<PathCondition> {
        let Some(header) = header else {
            self.dot_stack.push(None);
            return None;
        };
        let (predicate, faithful, bound_values, dot) = {
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
        // Structural accessor contracts recorded by evaluating the header
        // expression (`with dig … .Values.x`) bind whenever control
        // reaches the header, before this header's own fidelity sentinel.
        let header_captures = self
            .value_path_context()
            .expression_fail_captures(header.expr());
        self.absorb_helper_fails(&header_captures);
        if !faithful {
            let paths = self
                .value_path_context()
                .resolved_values_paths_from_expr(header.expr());
            if paths.is_empty() {
                self.approximate_condition_paths.push(String::new());
            }
            self.approximate_condition_paths.extend(paths);
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
            } else if !conjunct.is_trivial() {
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
        // A range over a helper's output (`range (include … . | fromJson)`)
        // evaluates the helper whenever control reaches the header: its
        // guarded reads carry the body's type-dispatch facts and its `fail`
        // captures its rejected complement, absorbed like any call site.
        self.helper_condition_claim_paths(header.expr());
        let range_source = header_range_source(header.expr());
        let (source_paths, direct_path, direct_variable_path) = {
            let context = self.value_path_context();
            let direct_path = context.single_direct_iterable_range_path_expr(range_source);
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
            (
                context
                    .resolved_values_paths_from_expr(header.expr())
                    .into_iter()
                    .collect::<Vec<_>>(),
                direct_path,
                direct_variable_path,
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
        // DERIVED list, so it says nothing about the path's own shape.
        if let Some(path) = &direct_path {
            self.direct_range_source_paths.insert(path.clone());
            if destructured {
                self.destructured_range_source_paths.insert(path.clone());
                // Under a gating predicate (a `kindIs "map"` partition arm)
                // the map hint holds only where that branch renders.
                let sink = if self.hint_scope_is_unconditional(path) {
                    &mut self.type_hints
                } else {
                    &mut self.guarded_type_hints
                };
                sink.entry(path.clone())
                    .or_default()
                    .insert("object".to_string());
            }
        }
        if let Some(path) = &direct_variable_path {
            self.direct_range_source_paths.insert(path.clone());
            if destructured {
                self.destructured_range_source_paths.insert(path.clone());
            }
        }

        let mut own = Vec::new();
        let mut extra = Contributions::default();
        for path in &source_paths {
            // Helper bodies mark range membership with truthy conditions
            // (the summary lane's flavor: range guards are a document-lane
            // shape the signal builder scopes to rendered documents).
            let predicate = if self.helper_scope {
                Predicate::truthy_path(path.clone())
            } else {
                Predicate::from(Guard::Range { path: path.clone() })
            };
            if emit_header_read && !renders_scalar_items {
                if self.helper_scope {
                    if destructured {
                        let guard = Guard::Range { path: path.clone() };
                        self.push_read(path, std::slice::from_ref(&guard));
                    } else {
                        self.push_read(path, &[]);
                    }
                } else {
                    let guard = Guard::Range { path: path.clone() };
                    self.push_read(path, std::slice::from_ref(&guard));
                }
            }
            own.push(predicate.clone());
            self.push_predicate(predicate);
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
        let iterations = self.exact_range_iterations(header);
        if let Some(iterations) = iterations {
            return (Some(Predicate::all(own)), extra, Some(iterations));
        }
        if let Some(path) = direct_path.as_ref().or(direct_variable_path.as_ref()) {
            self.active_direct_ranged_paths.push(path.clone());
        }
        let mut dot = direct_path
            .map(|path| AbstractValue::ValuesPath(helm_schema_core::append_value_path(&path, "*")));
        if self.helper_scope {
            let iterable = self.range_iterable_fragment_value(header);
            let item_dot = iterable
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
        // Bind the range's VALUE variable to the member identity: `$v` in
        // `range $k, $v := .Values.x` (and `$e` in `range $e := .Values.x`)
        // holds each member, so type tests and `fail` guards on it describe
        // `x.*`. The KEY variable of a destructured range has no member
        // identity, and hole rendering deliberately does not resolve these
        // (member reads must not manufacture placed rows).
        let member_variable = match value_variable {
            Some(variable) => Some(variable.to_string()),
            None if !destructured => helm_schema_ast::range_variable_name_expr(header.expr()),
            None => None,
        };
        if let Some((variable, binding)) = member_variable.zip(dot.clone()) {
            self.locals.range_member_values.insert(variable, binding);
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

    fn exact_range_iterations(&mut self, header: &TemplateHeader) -> Option<RangeIterations> {
        if !self.helper_scope {
            return None;
        }
        let iterable = self.range_iterable_fragment_value(header)?;
        let AbstractValue::List(items) = &iterable else {
            return None;
        };
        let variable = range_variable_name_expr(header.expr());
        Some(RangeIterations {
            items: items
                .iter()
                .map(|item| RangeIterationBinding {
                    dot: item.clone(),
                    variable: variable
                        .as_ref()
                        .map(|variable| (variable.clone(), item.clone())),
                })
                .collect(),
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

/// Whether an expression contains an `include`/`template` call naming a
/// helper the define index can resolve.
fn expr_contains_bound_helper_call(
    expr: &helm_schema_ast::TemplateExpr,
    db: &crate::analysis_db::IrAnalysisDb,
) -> bool {
    let mut found = false;
    expr.walk(|node| {
        if let helm_schema_ast::TemplateExpr::Call { function, args } = node
            && let Some(name) = crate::expr_eval::literal_helper_call_callee(function, args)
            && db.has_helper(name)
        {
            found = true;
        }
    });
    found
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
