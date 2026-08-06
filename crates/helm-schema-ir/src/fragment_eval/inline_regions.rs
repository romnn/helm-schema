//! Inline control regions inside scalars (`{{ if }}`/`{{ range }}` within
//! one flow scalar): arm activation, per-arm string parts, and the bounded
//! taint fallback for undecodable regions.

//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use helm_schema_ast::{TemplateExpr, parse_action_expressions};
use helm_schema_syntax::{Span, parse_go_template};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::parse_literal_list_range_expr;
use crate::eval_effect::{SelectionReachability, SelectionTruthReachability, SelectionTruthSource};
use crate::helper_meta::merge_rendered_row_meta;
use crate::node_eval::{NodeAction, control_header, else_if_pairs, node_action};
use crate::scalar_value::{TruthCondition, any_predicates, conjoin_predicates};
use crate::{Guard, ValueKind};
use helm_schema_ast::children_with_field;
use helm_schema_core::Predicate;

use super::domain::{PathCondition, StringPart, TaintPart, and_conditions, stamp_part_sites};
use super::eval::Interpreter;
use super::hole_effects::RenderedDemotion;
use super::holes::expr_contains_fail_call;
use super::lower::{
    LowerScope, MAX_SCALAR_ARM_FANOUT, lower_scalar_dispatch_arms, lower_value_scalar_arms,
};

impl Interpreter<'_> {
    /// Evaluate an inline `{{ if }}`, `{{ with }}`, or `{{ range }}`
    /// region inside a scalar by re-parsing the region text with the
    /// Go-template grammar and turning its branches into guarded scalar
    /// arms. The whole region evaluates under the region's site facts (its
    /// holes share the region's line).
    pub(super) fn eval_inline_region(
        &mut self,
        span: Span,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let region_site = self.region_site(span);
        let previous_site = std::mem::replace(&mut self.current_site, region_site);
        let mut arms = self.eval_inline_region_arms(span);
        for (_, parts) in &mut arms {
            stamp_part_sites(parts, self.current_site.as_ref());
        }
        self.restore_site(previous_site);
        arms
    }

    pub(super) fn eval_inline_region_arms(
        &mut self,
        span: Span,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let text = self.text(span);
        let Some(tree) = parse_go_template(text) else {
            return self.inline_region_taint(text);
        };
        let root = tree.root_node();
        let mut cursor = root.walk();
        let Some(action) = root
            .named_children(&mut cursor)
            .find(|child| matches!(child.kind(), "if_action" | "with_action" | "range_action"))
        else {
            return self.inline_region_taint(text);
        };
        self.eval_inline_control_action(action, text)
    }

    pub(super) fn eval_inline_control_action(
        &mut self,
        action: tree_sitter::Node<'_>,
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        if action.kind() == "range_action" {
            return self.eval_inline_range(action, text);
        }
        if action.kind() == "with_action" {
            return self.eval_inline_with(action, text);
        }

        let mut arm_specs = vec![(
            control_header(text, action),
            children_with_field(action, "consequence"),
        )];
        arm_specs.extend(else_if_pairs(action, text));
        arm_specs.push((None, children_with_field(action, "alternative")));

        let entry_predicates = self.active_predicates.len();
        let entry_locals = self.locals.clone();
        let mut prior_conditions: Vec<PathCondition> = Vec::new();
        let mut prior_reachability: Vec<SelectionTruthReachability> = Vec::new();
        let mut arms = Vec::new();
        let mut local_arm_states = Vec::new();
        for (branch_index, (header, children)) in arm_specs.into_iter().enumerate() {
            self.locals = entry_locals.clone();
            self.active_predicates.truncate(entry_predicates);
            let mut arm_condition = Predicate::True;
            for predicate in &prior_conditions {
                let negated = predicate.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions(arm_condition, negated);
            }
            let activated =
                self.activate_inline_if(header.as_ref(), action.start_byte(), branch_index);
            let (own, own_reachability) = activated.map_or_else(
                || {
                    (
                        None,
                        SelectionTruthReachability::exact(
                            Predicate::True,
                            SelectionTruthSource::RawInput,
                        ),
                    )
                },
                |(condition, truth)| (Some(condition), truth),
            );
            if let Some(own) = own {
                arm_condition = and_conditions(arm_condition, own.clone());
                prior_conditions.push(own);
            }
            let semantic_arm_truth = TruthCondition::all(
                prior_reachability
                    .iter()
                    .map(SelectionTruthReachability::truth_condition)
                    .map(|truth| truth.negated())
                    .chain(std::iter::once(own_reachability.truth_condition())),
            );
            if header.is_some() {
                prior_reachability.push(own_reachability);
            }
            if self.scalar_output_projection {
                self.active_predicates.truncate(entry_predicates);
                arm_condition = semantic_arm_truth.when_true();
                if arm_condition != Predicate::True {
                    self.push_predicate(arm_condition.clone());
                }
            }
            self.locals.enter_local_scope();
            let body_arms = if arm_condition == Predicate::False {
                Vec::new()
            } else if self.scalar_output_projection {
                self.scalar_body_arms(&children, text)
                    .unwrap_or_else(unknown_scalar_arms)
            } else {
                self.inline_body_arms(&children, text)
            };
            for (sub_condition, parts) in body_arms {
                arms.push((and_conditions(arm_condition.clone(), sub_condition), parts));
            }
            self.locals.exit_local_scope();
            local_arm_states.push((semantic_arm_truth, self.locals.clone()));
        }
        self.active_predicates.truncate(entry_predicates);
        self.locals = entry_locals.clone();
        let outcomes = local_arm_states
            .iter()
            .map(|(_, state)| state.clone())
            .collect::<Vec<_>>();
        self.locals.join_branch_outcomes(&entry_locals, &outcomes);
        self.locals
            .join_scalar_dispatch_arms(&entry_locals, &local_arm_states, true);
        if arms.len() > MAX_SCALAR_ARM_FANOUT {
            let parts = arms.into_iter().flat_map(|(_, parts)| parts).collect();
            return vec![(Predicate::True, parts)];
        }
        arms
    }

    pub(super) fn eval_inline_with(
        &mut self,
        action: tree_sitter::Node<'_>,
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let entry_predicates = self.active_predicates.len();
        let entry_dots = self.dot_stack.len();
        let entry_locals = self.locals.clone();
        let (own, _reachability) = self.activate_with(
            control_header(text, action).as_ref(),
            action.start_byte(),
            0,
        );
        let body_condition = own.clone().unwrap_or(Predicate::True);
        let consequence = children_with_field(action, "consequence");
        let body_arms = if body_condition == Predicate::False {
            Vec::new()
        } else if self.scalar_output_projection {
            self.scalar_body_arms(&consequence, text)
                .unwrap_or_else(unknown_scalar_arms)
        } else {
            self.inline_body_arms(&consequence, text)
        };
        let mut arms = body_arms
            .into_iter()
            .map(|(condition, parts)| (and_conditions(body_condition.clone(), condition), parts))
            .collect::<Vec<_>>();

        self.active_predicates.truncate(entry_predicates);
        self.dot_stack.truncate(entry_dots);
        self.locals = entry_locals.clone();
        let alternative_condition = own.as_ref().map_or(Predicate::True, Predicate::negated);
        if alternative_condition != Predicate::True {
            self.push_predicate(alternative_condition.clone());
        }
        let alternative = children_with_field(action, "alternative");
        let alternative_arms = if alternative_condition == Predicate::False {
            Vec::new()
        } else if self.scalar_output_projection {
            self.scalar_body_arms(&alternative, text)
                .unwrap_or_else(unknown_scalar_arms)
        } else {
            self.inline_body_arms(&alternative, text)
        };
        for (condition, parts) in alternative_arms {
            arms.push((
                and_conditions(alternative_condition.clone(), condition),
                parts,
            ));
        }

        self.active_predicates.truncate(entry_predicates);
        self.dot_stack.truncate(entry_dots);
        self.locals = entry_locals;
        arms
    }

    /// Evaluate an inline `{{ range }}…{{ end }}` region inside a scalar
    /// with the structural range activation: literal-list domains, the
    /// typed member value, and the header read under `Guard::Range`; body
    /// contributions carry the range condition. Body-local bindings stay
    /// region-local (entry locals are restored, the same boundary as a
    /// structural branch scope).
    pub(super) fn eval_inline_range(
        &mut self,
        node: tree_sitter::Node<'_>,
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let Some(header) = helm_schema_ast::range_header_from_source(node, text) else {
            return self.inline_region_taint(text);
        };
        let entry_predicates = self.active_predicates.len();
        let entry_dots = self.dot_stack.len();
        let entry_ranged = self.active_range_modes.len();
        let entry_locals = self.locals.clone();
        if let Some((variable, literals)) = parse_literal_list_range_expr(header.expr()) {
            self.locals.insert_range_domain(variable, literals);
        }
        let _ = self.absorb_header_execution_effects(header.expr());
        let range_source = match header.expr().deparen() {
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => value.as_ref(),
            expr => expr,
        };
        let range_subject = self.value_path_context().range_subject_expr(range_source);
        let source_paths = &range_subject.influence_paths;
        let member_identity = range_subject.member_identity.clone();
        let direct_path = member_identity
            .as_ref()
            .map(|identity| identity.path.clone());
        let input_identity = range_subject.input_identity.clone();
        let destructured = helm_schema_ast::range_has_destructured_variable_definition(node);
        self.record_range_identities(
            member_identity.as_ref(),
            input_identity.as_ref(),
            destructured,
        );
        self.record_selection_range_captures(range_subject.value.as_ref(), destructured);
        let mut own = Vec::new();
        for path in source_paths {
            let guard = Guard::Range { path: path.clone() };
            self.push_control_read(path, std::slice::from_ref(&guard));
            own.push(Predicate::from(guard.clone()));
            self.push_predicate(Predicate::from(guard));
        }
        let condition = Predicate::all(own);
        let body_condition =
            SelectionReachability::exact(condition.clone(), SelectionTruthSource::RawInput)
                .output_selection_predicate("inline range body", condition.value_paths());
        self.push_active_range_identity_modes(
            member_identity.as_ref(),
            input_identity.as_ref(),
            destructured,
        );
        let dot = range_subject
            .member_value
            .clone()
            .map(|value| value.to_context_value());
        let value_variable = if destructured {
            helm_schema_ast::range_destructured_value_variable(node, text)
        } else {
            helm_schema_ast::range_variable_name_expr(header.expr())
        };
        if let Some((variable, binding)) = value_variable.zip(dot.clone()) {
            self.locals.range_member_values.insert(variable, binding);
        }
        if destructured
            && let Some(variable) = helm_schema_ast::range_destructured_key_variable(node, text)
            && let Some(path) = direct_path
        {
            self.locals
                .range_member_values
                .insert(variable, AbstractValue::RangeKey(path));
        }
        self.dot_stack.push(dot);
        self.loop_depth += 1;
        let mut arms = Vec::new();
        let body = children_with_field(node, "body");
        let body_arms = if self.scalar_output_projection {
            self.scalar_body_arms(&body, text)
                .unwrap_or_else(unknown_scalar_arms)
        } else {
            self.inline_body_arms(&body, text)
        };
        for (sub_condition, parts) in body_arms {
            arms.push((and_conditions(body_condition.clone(), sub_condition), parts));
        }
        self.loop_depth -= 1;
        self.dot_stack.truncate(entry_dots);
        self.active_predicates.truncate(entry_predicates);
        self.active_range_modes.truncate(entry_ranged);
        self.locals = entry_locals;
        // A `{{ range }}…{{ else }}…{{ end }}` alternative renders when the
        // iterable is empty; like the structural range arms it decodes no
        // negated condition. The carrier records the positive arm, but this
        // legacy consumer cannot preserve its complement's branch scope.
        let alternative = children_with_field(node, "alternative");
        let alternative_arms = if self.scalar_output_projection {
            self.scalar_body_arms(&alternative, text)
                .unwrap_or_else(unknown_scalar_arms)
        } else {
            self.inline_body_arms(&alternative, text)
        };
        for (sub_condition, parts) in alternative_arms {
            arms.push((sub_condition, parts));
        }
        arms
    }

    /// Mark the resolved range identities on the shared range-mode registry
    /// and record the range-input contract capture for the iterable path.
    fn record_range_identities(
        &mut self,
        member_identity: Option<&crate::value_path_context::RangeSubjectIdentity>,
        input_identity: Option<&crate::value_path_context::RangeSubjectIdentity>,
        destructured: bool,
    ) {
        if let Some(identity) = member_identity {
            self.range_modes.mark_member_identity(&identity.path);
            if destructured {
                self.range_modes.mark_destructured(&identity.path);
            }
            if identity.json_decoded {
                self.range_modes.mark_json_decoded(&identity.path);
            }
        }
        if let Some(identity) = input_identity {
            self.range_modes.mark_input_identity(&identity.path);
            if destructured {
                self.range_modes.mark_destructured(&identity.path);
            }
            if identity.json_decoded {
                self.range_modes.mark_json_decoded(&identity.path);
            }
        }
        let input_contract_identity = input_identity.or_else(|| {
            member_identity.filter(|identity| {
                helm_schema_core::split_value_path(&identity.path)
                    .iter()
                    .any(|segment| segment == "*")
            })
        });
        if let Some(identity) = input_contract_identity {
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
                .any(|predicate| matches!(predicate, Predicate::False))
                && !self.fail_conditions.contains(&capture)
            {
                self.fail_conditions.push(capture);
            }
        }
    }

    /// Records the exact iterable obligation for each selected raw path in
    /// a first-truthy chain.
    ///
    /// The prior candidates' falsiness determines which path is selected;
    /// the selected path's own truthiness keeps a final falsy fallback open,
    /// because Helm accepts that state without iterating it.
    pub(super) fn record_selection_range_captures(
        &mut self,
        iterable_value: Option<&crate::abstract_value::AbstractValue>,
        destructured: bool,
    ) {
        let Some(chain) = iterable_value
            .and_then(crate::abstract_value::AbstractValue::selection_chain_identity_paths)
        else {
            return;
        };
        let mut prior_falsy = Vec::new();
        for path in &chain {
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
                .any(|predicate| matches!(predicate, Predicate::False))
                && !self.fail_conditions.contains(&capture)
            {
                self.fail_conditions.push(capture);
            }
            prior_falsy.push(Predicate::truthy_path(path.clone()).negated());
        }
    }

    /// Activate the resolved identities for the body evaluation; the caller
    /// truncates `active_range_modes` back to its entry length on exit.
    fn push_active_range_identity_modes(
        &mut self,
        member_identity: Option<&crate::value_path_context::RangeSubjectIdentity>,
        input_identity: Option<&crate::value_path_context::RangeSubjectIdentity>,
        destructured: bool,
    ) {
        if let Some(identity) = member_identity {
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
        if let Some(identity) = input_identity {
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
    }

    /// Fold one inline branch body into guarded part arms. Conditions
    /// arising inside the body (helper meta branches) stay on their own
    /// hole's arms — sibling holes of the same body are not correlated, so
    /// each part keeps exactly its own conditions (a cartesian product here
    /// would fabricate contradictory cross-hole combinations).
    pub(super) fn inline_body_arms(
        &mut self,
        children: &[tree_sitter::Node<'_>],
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let mut base: Vec<StringPart> = Vec::new();
        let mut conditional = Vec::new();
        for child in children {
            for (condition, parts) in self.inline_child_arms(*child, text) {
                if condition == Predicate::True {
                    base.extend(parts);
                } else {
                    conditional.push((condition, parts));
                }
            }
        }
        let mut arms = Vec::new();
        if !base.is_empty() || conditional.is_empty() {
            arms.push((Predicate::True, base));
        }
        arms.extend(conditional);
        arms
    }

    /// Compose the mutually exclusive alternatives of each scalar-producing
    /// child in source order. Keeping each child as one choice prevents a
    /// branch-selected local from being reinterpreted as several independent
    /// optional text fragments.
    pub(super) fn scalar_body_arms(
        &mut self,
        children: &[tree_sitter::Node<'_>],
        text: &str,
    ) -> Option<Vec<(PathCondition, Vec<StringPart>)>> {
        let mut states = vec![(Predicate::True, Vec::new())];
        for child in children {
            let action = node_action(text, *child);
            let alternatives = self.inline_child_arms(*child, text);
            if alternatives.is_empty() {
                if matches!(action, NodeAction::Output(Some(_))) {
                    return None;
                }
                continue;
            }
            // An exhaustive control whose every arm renders nothing has one
            // unconditional empty contribution. Retaining the arm predicates
            // would multiply opaque conditions even though they cannot
            // affect this body's rendered scalar; branch-local state changes
            // have already joined while evaluating the child.
            if alternatives.iter().all(|(_, parts)| parts.is_empty()) {
                continue;
            }
            let mut next = Vec::new();
            for (state_condition, state_parts) in &states {
                for (alternative_condition, alternative_parts) in &alternatives {
                    let Some(condition) =
                        conjoin_predicates(state_condition.clone(), alternative_condition.clone())
                    else {
                        continue;
                    };
                    let mut parts = state_parts.clone();
                    parts.extend(alternative_parts.iter().cloned());
                    next.push((condition, parts));
                    if next.len() > MAX_SCALAR_ARM_FANOUT {
                        return None;
                    }
                }
            }
            states = merge_scalar_part_arms(next);
            if states.is_empty() {
                return None;
            }
        }
        Some(states)
    }

    pub(super) fn activate_inline_if(
        &mut self,
        header: Option<&helm_schema_ast::TemplateHeader>,
        region_start: usize,
        branch_index: usize,
    ) -> Option<(PathCondition, SelectionTruthReachability)> {
        let header = header?;
        let (mut predicate, mut faithful) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.condition_lowering_is_faithful(header.expr()),
            )
        };
        let (helper_paths, evaluated_truth) = self.absorb_header_execution_effects(header.expr());
        let evaluated_truth_is_unknown = evaluated_truth.when_true().exact_predicate().is_none();
        if let Some(exact) = evaluated_truth.when_true().exact_predicate() {
            predicate = exact;
            faithful = true;
        }
        if evaluated_truth_is_unknown
            && matches!(predicate, Predicate::True)
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
            let mut paths = self
                .value_path_context()
                .resolved_values_paths_from_expr(header.expr());
            let evaluated_subset = evaluated_truth.when_true().proven_selected_subset();
            let evaluated_subset =
                (evaluated_subset != Predicate::False).then_some(evaluated_subset);
            let dedup_subset = self.first_iteration_dedup_sound_subset(header.expr());
            let dedup_subset = (!dedup_subset.is_empty())
                .then(|| Predicate::all(dedup_subset.into_iter().map(Predicate::from).collect()));
            let positive_subset =
                any_predicates(evaluated_subset.into_iter().chain(dedup_subset).collect());
            paths.extend(positive_subset.value_paths());
            let marker = format!("{}:{region_start}:{branch_index}", self.source_offset);
            predicate = if positive_subset == Predicate::False {
                Predicate::approximate(marker, paths)
            } else {
                Predicate::approximate_with_sound_predicate(marker, paths, positive_subset)
            };
        }
        let guards = predicate.contract_guards();
        for guard in &guards {
            for path in guard.value_paths() {
                self.push_control_read(path, std::slice::from_ref(guard));
            }
            self.push_predicate(Predicate::from(guard.clone()));
        }
        if guards.is_empty() {
            self.push_predicate(predicate.clone());
        }
        let semantic_truth = if evaluated_truth.when_true().exact_predicate().is_none() && faithful
        {
            SelectionTruthReachability::exact(predicate.clone(), SelectionTruthSource::RawInput)
        } else {
            evaluated_truth
        };
        Some((predicate, semantic_truth))
    }

    /// One inline body child as guarded part arms. An empty vec means "no
    /// contribution" (the fold skips it); nested inline control degrades to
    /// conservative taint.
    pub(super) fn inline_child_arms(
        &mut self,
        node: tree_sitter::Node<'_>,
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        match node_action(text, node) {
            NodeAction::Text => {
                let content = trimmed_template_text(node, text);
                if content.is_empty() {
                    Vec::new()
                } else {
                    vec![(
                        Predicate::True,
                        vec![StringPart::Text([content].into_iter().collect())],
                    )]
                }
            }
            NodeAction::Output(Some(exprs)) => {
                // A `fail` output terminates rendering: no valid values
                // document may satisfy the guards active here, and the
                // action renders nothing.
                if exprs.iter().any(expr_contains_fail_call) {
                    self.record_fail_condition();
                    return Vec::new();
                }
                self.record_required_subjects(&exprs);
                let _ = self.inline_static_file_fragments(&exprs);
                let hole = self.eval_hole_exprs(&exprs);
                self.absorb_hole_effects(&hole.effects, RenderedDemotion::None);
                let defaulted = hole.effects.default_paths_with_local();
                let kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
                    ValueKind::Fragment
                } else {
                    ValueKind::PartialScalar
                };
                let mut hole_meta = hole.effects.local_output_meta.clone();
                merge_rendered_row_meta(&mut hole_meta, &hole.effects.helper_rendered);
                for (path, keys) in &hole.effects.omitted_map_keys {
                    let meta = hole_meta.entry(path.clone()).or_default();
                    for key in keys {
                        meta.omitted_keys.insert(key.clone(), Vec::new());
                    }
                }
                // As at block-scalar sites, string-contract metadata must
                // abstain under approximately lowered conditions.
                let no_contracts = std::collections::BTreeSet::new();
                let row_string_contract_paths = if self.under_approximate_condition() {
                    &no_contracts
                } else {
                    &hole.effects.string_contract_paths
                };
                let scope = LowerScope {
                    defaulted_paths: &defaulted,
                    encoded_paths: &hole.effects.encoded_paths,
                    derived_text_paths: &hole.effects.derived_text_paths,
                    merge_operand_paths: &hole.effects.merge_operand_paths,
                    yaml_serialized_paths: &hole.effects.yaml_serialized_paths,
                    templated_yaml_paths: &hole.effects.templated_yaml_paths,
                    shape_erased_paths: &hole.effects.shape_erased_paths,
                    stringified_paths: &hole.effects.stringified_paths,
                    nil_omitting_paths: &hole.effects.nil_omitting_paths,
                    string_contract_paths: row_string_contract_paths,
                    plain_slot_string_format_paths: &hole.effects.plain_slot_string_format_paths,
                    json_serialized_paths: &hole.effects.json_serialized_paths,
                    chart_value_defaults: &self.locals.chart_value_defaults,
                    local_source_paths: &hole.effects.local_source_paths,
                    local_output_meta: &hole_meta,
                };
                self.scalar_output_projection
                    .then_some(hole.scalar_dispatch.as_ref())
                    .flatten()
                    .and_then(|dispatch| lower_scalar_dispatch_arms(dispatch, kind, &scope))
                    .unwrap_or_else(|| match &hole.value {
                        Some(value) => lower_value_scalar_arms(value, kind, &scope),
                        None => Vec::new(),
                    })
            }
            NodeAction::Assignment(Some(exprs)) => {
                self.eval_assignment_exprs(&exprs);
                Vec::new()
            }
            NodeAction::Range(_) => self.eval_inline_range(node, text),
            NodeAction::If(_) => self.eval_inline_control_action(node, text),
            NodeAction::With(_) => self.eval_inline_with(node, text),
            NodeAction::Output(None) | NodeAction::Assignment(None) | NodeAction::Suppressed => {
                Vec::new()
            }
            NodeAction::Descend => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                if self.scalar_output_projection {
                    self.scalar_body_arms(&children, text)
                        .unwrap_or_else(unknown_scalar_arms)
                } else {
                    self.inline_body_arms(&children, text)
                }
            }
        }
    }

    pub(super) fn inline_region_taint(
        &mut self,
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let taint = self.resolved_paths_of_action_text(text);
        if taint.is_empty() {
            return Vec::new();
        }
        vec![(
            Predicate::True,
            vec![StringPart::Taint(TaintPart::new(taint))],
        )]
    }

    pub(super) fn resolved_paths_of_action_text(
        &mut self,
        text: &str,
    ) -> std::collections::BTreeSet<String> {
        let mut paths = std::collections::BTreeSet::new();
        for expr in parse_action_expressions(text) {
            paths.extend(
                self.value_path_context()
                    .resolved_values_paths_from_expr(&expr),
            );
        }
        paths
    }
}

fn unknown_scalar_arms() -> Vec<(PathCondition, Vec<StringPart>)> {
    vec![(
        Predicate::True,
        vec![StringPart::Taint(TaintPart::new(
            std::collections::BTreeSet::new(),
        ))],
    )]
}

fn merge_scalar_part_arms(
    arms: Vec<(PathCondition, Vec<StringPart>)>,
) -> Vec<(PathCondition, Vec<StringPart>)> {
    let mut merged: Vec<(PathCondition, Vec<StringPart>)> = Vec::new();
    for (condition, parts) in arms {
        if let Some((existing, _)) = merged
            .iter_mut()
            .find(|(_, existing_parts)| *existing_parts == parts)
        {
            *existing = any_predicates(vec![existing.clone(), condition]);
        } else {
            merged.push((condition, parts));
        }
    }
    merged
}

fn trimmed_template_text(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut content = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
    if source
        .get(..node.start_byte())
        .is_some_and(|prefix| prefix.ends_with("-}}"))
    {
        content = content
            .trim_start_matches([' ', '\t', '\r', '\n'])
            .to_string();
    }
    if source
        .get(node.end_byte()..)
        .is_some_and(|suffix| suffix.starts_with("{{-"))
    {
        content = content
            .trim_end_matches([' ', '\t', '\r', '\n'])
            .to_string();
    }
    content
}
