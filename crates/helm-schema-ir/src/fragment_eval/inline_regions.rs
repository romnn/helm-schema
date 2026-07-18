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
use crate::helper_meta::merge_rendered_row_meta;
use crate::node_eval::{NodeAction, control_header, else_if_pairs, node_action};
use crate::{Guard, ValueKind};
use helm_schema_ast::children_with_field;
use helm_schema_core::Predicate;

use super::domain::{PathCondition, StringPart, TaintPart, and_conditions, stamp_part_sites};
use super::eval::Interpreter;
use super::hole_effects::RenderedDemotion;
use super::holes::expr_contains_fail_call;
use super::lower::{LowerScope, MAX_SCALAR_ARM_FANOUT, lower_value_scalar_arms};

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
            stamp_part_sites(parts, &self.current_site);
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
        let mut prior: Vec<PathCondition> = Vec::new();
        let mut arms = Vec::new();
        for (branch_index, (header, children)) in arm_specs.into_iter().enumerate() {
            self.active_predicates.truncate(entry_predicates);
            let mut arm_condition = Predicate::True;
            for predicate in &prior {
                let negated = predicate.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions(arm_condition, negated);
            }
            if let Some(own) =
                self.activate_inline_if(header.as_ref(), action.start_byte(), branch_index)
            {
                arm_condition = and_conditions(arm_condition, own.clone());
                prior.push(own);
            }
            for (sub_condition, parts) in self.inline_body_arms(&children, text) {
                arms.push((and_conditions(arm_condition.clone(), sub_condition), parts));
            }
        }
        self.active_predicates.truncate(entry_predicates);
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
        let own = self.activate_with(
            control_header(text, action).as_ref(),
            action.start_byte(),
            0,
        );
        let body_condition = own.clone().unwrap_or(Predicate::True);
        let mut arms = self
            .inline_body_arms(&children_with_field(action, "consequence"), text)
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
        for (condition, parts) in
            self.inline_body_arms(&children_with_field(action, "alternative"), text)
        {
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
    /// direct-path item dot, and the header read under `Guard::Range`; body
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
        let entry_ranged = self.active_direct_ranged_paths.len();
        let entry_locals = self.locals.clone();
        if let Some((variable, literals)) = parse_literal_list_range_expr(header.expr()) {
            self.locals.insert_range_domain(variable, literals);
        }
        self.absorb_header_execution_effects(header.expr());
        let range_source = match header.expr().deparen() {
            TemplateExpr::VariableDefinition { value, .. }
            | TemplateExpr::Assignment { value, .. } => value.as_ref(),
            expr => expr,
        };
        let (source_paths, direct_path, json_decoded_path) = {
            let context = self.value_path_context();
            (
                context
                    .resolved_values_paths_from_expr(header.expr())
                    .into_iter()
                    .collect::<Vec<_>>(),
                context.single_direct_iterable_range_path_expr(range_source),
                context.single_direct_json_decoded_range_path_expr(range_source),
            )
        };
        let destructured = helm_schema_ast::range_has_destructured_variable_definition(node);
        let mut own = Vec::new();
        for path in &source_paths {
            let guard = Guard::Range { path: path.clone() };
            self.push_read(path, std::slice::from_ref(&guard));
            own.push(Predicate::from(guard.clone()));
            self.push_predicate(Predicate::from(guard));
        }
        let condition = Predicate::all(own);
        if let Some(path) = &direct_path {
            self.range_modes.mark_direct(path);
            if destructured {
                self.range_modes.mark_destructured(path);
            }
            self.active_direct_ranged_paths.push(path.clone());
        }
        if let Some(path) = &json_decoded_path {
            self.range_modes.mark_json_decoded(path);
        }
        let dot = direct_path.as_ref().map(|path| {
            let member_path = helm_schema_core::append_value_path(path, "*");
            if json_decoded_path.as_ref() == Some(path) {
                AbstractValue::JsonDecodedPath(member_path)
            } else {
                AbstractValue::ValuesPath(member_path)
            }
        });
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
        for (sub_condition, parts) in
            self.inline_body_arms(&children_with_field(node, "body"), text)
        {
            arms.push((and_conditions(condition.clone(), sub_condition), parts));
        }
        self.loop_depth -= 1;
        self.dot_stack.truncate(entry_dots);
        self.active_predicates.truncate(entry_predicates);
        self.active_direct_ranged_paths.truncate(entry_ranged);
        self.locals = entry_locals;
        // A `{{ range }}…{{ else }}…{{ end }}` alternative renders when the
        // iterable is empty; like the structural range arms it decodes no
        // negated condition.
        for (sub_condition, parts) in
            self.inline_body_arms(&children_with_field(node, "alternative"), text)
        {
            arms.push((sub_condition, parts));
        }
        arms
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

    pub(super) fn activate_inline_if(
        &mut self,
        header: Option<&helm_schema_ast::TemplateHeader>,
        region_start: usize,
        branch_index: usize,
    ) -> Option<PathCondition> {
        let header = header?;
        let (mut predicate, faithful) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.condition_lowering_is_faithful(header.expr()),
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
            let paths = self
                .value_path_context()
                .resolved_values_paths_from_expr(header.expr());
            predicate = Predicate::approximate_with_sound_subset(
                format!("{}:{region_start}:{branch_index}", self.source_offset),
                paths,
                self.first_iteration_dedup_sound_subset(header.expr()),
            );
        }
        let guards = predicate.contract_guards();
        for guard in &guards {
            for path in guard.value_paths() {
                self.push_read(path, std::slice::from_ref(guard));
            }
            self.push_predicate(Predicate::from(guard.clone()));
        }
        if guards.is_empty() {
            self.push_predicate(predicate.clone());
        }
        Some(predicate)
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
                let content = node.utf8_text(text.as_bytes()).unwrap_or("");
                if content.is_empty() {
                    Vec::new()
                } else {
                    vec![(
                        Predicate::True,
                        vec![StringPart::Text(
                            [content.to_string()].into_iter().collect(),
                        )],
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
                    yaml_serialized_paths: &hole.effects.yaml_serialized_paths,
                    shape_erased_paths: &hole.effects.shape_erased_paths,
                    string_contract_paths: row_string_contract_paths,
                    json_serialized_paths: &hole.effects.json_serialized_paths,
                    chart_value_defaults: &self.locals.chart_value_defaults,
                    local_source_paths: &hole.effects.local_source_paths,
                    local_output_meta: &hole_meta,
                };
                match &hole.value {
                    Some(value) => lower_value_scalar_arms(value, kind, &scope),
                    None => Vec::new(),
                }
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
                self.inline_body_arms(&children, text)
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
