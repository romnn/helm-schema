//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use std::collections::HashSet;

use helm_schema_ast::{TemplateExpr, parse_action_expressions, parse_expr_text};
use helm_schema_syntax::{BlockScalar, ScalarPart, ScalarParts, Span, parse_go_template};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{
    BoundValueContext, parse_get_binding_from_exprs, parse_literal_list_range_expr,
};
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::fragment_assignment::parse_helper_assignment_from_exprs;
use crate::fragment_expr_eval::{
    FragmentEvalContext, document_result_from_expr, locals_with_roots,
};
use crate::helper_summary::merge_output_use_meta;
use crate::node_eval::{NodeAction, control_header, else_if_pairs, node_action};
use crate::{Guard, ValueKind};
use helm_schema_ast::children_with_field;
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, StringPart, and_conditions,
};
use super::eval::Interpreter;
use super::lower::{LowerScope, MAX_SCALAR_ARMS, lower_value, lower_value_scalar_arms};

pub(super) struct HoleEval {
    pub(super) value: Option<AbstractValue>,
    pub(super) effects: Effects,
}

/// One layout segment of a scalar run: literal text, a template hole, or a
/// whole inline control region (grouping the region's holes and texts).
enum Segment {
    Text(String),
    Hole(Span),
    Region(Span),
}

impl Interpreter<'_> {
    /// Evaluate the expressions of one output hole through the shared value
    /// lattice, resolving bound helper calls via the memoized summaries.
    fn eval_hole_exprs(&mut self, exprs: &[TemplateExpr]) -> HoleEval {
        let current_dot = self
            .current_dot_fragment()
            .map(|value| value.to_context_value())
            .or_else(|| self.current_dot_binding());
        let mut env = EvalEnv::from_helper_context(Some(&self.root_bindings), current_dot.as_ref())
            .without_helper_call_args();
        env.locals = locals_with_roots(&self.locals.fragment_values, &self.root_bindings);
        env.local_default_paths = self.locals.default_paths.clone();
        env.local_output_meta = self.locals.output_meta.clone();
        env.bound_values =
            BoundValueContext::new(&self.locals.range_domains, &self.locals.get_bindings);
        let context = FragmentEvalContext::new(self.db);
        let mut seen = HashSet::new();
        let mut values = Vec::new();
        let mut effects = Effects::default();
        for expr in exprs {
            let result = document_result_from_expr(
                expr,
                &env,
                &self.locals.fragment_values,
                Some(&self.root_bindings),
                current_dot.as_ref(),
                context,
                &mut seen,
            );
            values.extend(result.value);
            effects.merge(result.effects);
        }
        HoleEval {
            value: AbstractValue::choice(values).map(|value| value.to_context_value()),
            effects,
        }
    }

    /// Absorb a hole's effect stream into interpreter state and the read
    /// list: chart-level default mutations (source order), bound-value
    /// reads, and helper-internal read facts. Rendered helper rows become
    /// reads only in no-render contexts (assignments), where the current
    /// pipeline also demotes them to pathless claims.
    fn absorb_hole_effects(&mut self, effects: &Effects, include_rendered_reads: bool) {
        let mut chart_defaults = effects.chart_default_paths.clone();
        chart_defaults.extend(effects.helper_summary.chart_defaults.iter().cloned());
        self.locals.append_chart_value_defaults(&mut chart_defaults);

        let bound_reads: Vec<String> = effects.bound_output_paths.iter().cloned().collect();
        for path in bound_reads {
            self.push_read(&path, &[]);
        }
        let summary = effects.helper_summary.clone();
        // Guard-path reads that are strict ancestors of a predicate path the
        // helper explicitly severed (index-call narrowing) are dropped, the
        // same way the current emission skips them.
        let suppressed: std::collections::BTreeSet<&String> = summary
            .output_uses
            .iter()
            .flat_map(|output| output.meta.suppress_predicate_paths.iter())
            .collect();
        for (path, meta) in &summary.guard_path_meta {
            if !suppressed.contains(path)
                && suppressed
                    .iter()
                    .any(|narrowed| helm_schema_core::values_path_is_descendant(narrowed, path))
            {
                continue;
            }
            self.push_meta_reads(path, meta);
        }
        for output in &summary.output_uses {
            if output.is_dependency() || include_rendered_reads {
                self.push_meta_reads(&output.source_expr, &output.meta);
            }
        }
    }

    /// Pathless reads for every values path the hole's effects attribute
    /// (used where the current pipeline suppresses rendered placement, i.e.
    /// assignment right-hand sides).
    fn push_effects_reads(&mut self, hole: &HoleEval) {
        let defaulted = hole.effects.default_paths_with_local();
        for path in hole.effects.output_value_paths() {
            if defaulted.contains(&path) {
                let default_guard = Guard::Default { path: path.clone() };
                self.push_read(&path, std::slice::from_ref(&default_guard));
            } else {
                self.push_read(&path, &[]);
            }
        }
    }

    /// Evaluate a hole standing as an entire fragment position.
    pub(super) fn eval_entire_hole(&mut self, text: &str) -> Guarded<AbstractFragment> {
        self.eval_output_action(text).0
    }

    /// Evaluate a standalone output action: the lowered fragment plus the
    /// action's explicit rendered indent (`… | nindent N`), which decides
    /// which enclosing container the output attaches to.
    pub(super) fn eval_output_action(
        &mut self,
        text: &str,
    ) -> (Guarded<AbstractFragment>, Option<usize>) {
        if hole_is_control_fragment(text) {
            return (Guarded::empty(), None);
        }
        let exprs = parse_expr_text(text);
        if exprs.is_empty() {
            return (Guarded::empty(), None);
        }
        if parse_helper_assignment_from_exprs(&exprs).is_some() {
            self.eval_assignment_exprs(&exprs);
            return (Guarded::empty(), None);
        }
        let inlined = self.inline_static_file_fragments(&exprs);
        let width = exprs
            .iter()
            .rev()
            .find_map(TemplateExpr::fragment_indent_width);
        let hole = self.eval_hole_exprs(&exprs);
        self.absorb_hole_effects(&hole.effects, false);
        let kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };
        let (value, extra_paths) =
            prepare_hole_value(hole.value, &hole.effects, kind == ValueKind::Scalar);
        let defaulted = hole.effects.default_paths_with_local();
        let scope = LowerScope {
            defaulted_paths: &defaulted,
            encoded_paths: &hole.effects.encoded_paths,
            chart_value_defaults: &self.locals.chart_value_defaults,
            local_output_meta: &hole.effects.local_output_meta,
        };
        let mut out = match &value {
            Some(value) => lower_value(value, kind, &scope),
            None => Guarded::empty(),
        };
        for path in extra_paths {
            for (condition, splice) in scope.path_splice_arms(&path, kind) {
                out.arms.push((condition, AbstractFragment::Splice(splice)));
            }
        }
        out.extend(inlined);
        (out, width)
    }

    /// Evaluate a hole rendered inside a partial scalar: guarded arms of
    /// string parts.
    pub(super) fn eval_hole_parts(&mut self, text: &str) -> Vec<(PathCondition, Vec<StringPart>)> {
        if hole_is_control_fragment(text) {
            return Vec::new();
        }
        let exprs = parse_expr_text(text);
        if exprs.is_empty() {
            return Vec::new();
        }
        if parse_helper_assignment_from_exprs(&exprs).is_some() {
            self.eval_assignment_exprs(&exprs);
            return Vec::new();
        }
        let hole = self.eval_hole_exprs(&exprs);
        self.absorb_hole_effects(&hole.effects, false);
        let (value, extra_paths) = prepare_hole_value(hole.value, &hole.effects, true);
        let defaulted = hole.effects.default_paths_with_local();
        let scope = LowerScope {
            defaulted_paths: &defaulted,
            encoded_paths: &hole.effects.encoded_paths,
            chart_value_defaults: &self.locals.chart_value_defaults,
            local_output_meta: &hole.effects.local_output_meta,
        };
        let mut arms = match &value {
            Some(value) => lower_value_scalar_arms(value, &scope),
            None => Vec::new(),
        };
        let mut plain_parts: Vec<StringPart> = Vec::new();
        for path in extra_paths {
            for (condition, splice) in scope.path_splice_arms(&path, ValueKind::PartialScalar) {
                if condition == Predicate::True {
                    plain_parts.push(StringPart::Splice(splice));
                } else {
                    arms.push((condition, vec![StringPart::Splice(splice)]));
                }
            }
        }
        if !plain_parts.is_empty() {
            arms.push((Predicate::True, plain_parts));
        }
        arms
    }

    /// Whether any hole of a scalar run renders a YAML fragment (used for
    /// range body-shape classification).
    pub(super) fn scalar_parts_render_fragment(&self, parts: &ScalarParts) -> bool {
        parts.parts.iter().any(|part| match part {
            ScalarPart::Hole(span) => parse_expr_text(self.text(*span))
                .iter()
                .any(TemplateExpr::renders_yaml_fragment),
            ScalarPart::Text(_) => false,
        })
    }

    /// Evaluate a scalar run (an entry value, item value, or scalar line).
    pub(super) fn eval_scalar_parts(&mut self, parts: &ScalarParts) -> Guarded<AbstractFragment> {
        let segments = self.scalar_segments(parts);
        if let Some(span) = entire_hole_span(&segments) {
            return self.eval_entire_hole(self.text(span));
        }
        let mut arms: Vec<(PathCondition, Vec<StringPart>)> = vec![(Predicate::True, Vec::new())];
        for segment in segments {
            let segment_arms = match segment {
                Segment::Text(text) => {
                    if text.is_empty() {
                        continue;
                    }
                    vec![(
                        Predicate::True,
                        vec![StringPart::Text([text].into_iter().collect())],
                    )]
                }
                Segment::Hole(span) => self.eval_hole_parts(self.text(span)),
                Segment::Region(span) => self.eval_inline_region(span),
            };
            arms = combine_scalar_arms(arms, segment_arms);
        }
        scalar_arms_to_fragment(arms, false)
    }

    /// Group a scalar run's parts into segments, folding parts covered by an
    /// inline control region into one region segment.
    fn scalar_segments(&self, parts: &ScalarParts) -> Vec<Segment> {
        let mut segments: Vec<Segment> = Vec::new();
        for part in &parts.parts {
            let span = match part {
                ScalarPart::Text(span) | ScalarPart::Hole(span) => *span,
            };
            if let Some(region) = self
                .inline_regions
                .iter()
                .find(|region| region.start <= span.start && span.start < region.end)
            {
                let already_grouped = matches!(
                    segments.last(),
                    Some(Segment::Region(last)) if last.start == region.start
                );
                if !already_grouped {
                    segments.push(Segment::Region(*region));
                }
                continue;
            }
            match part {
                ScalarPart::Text(span) => {
                    segments.push(Segment::Text(self.text(*span).to_string()));
                }
                ScalarPart::Hole(span) => segments.push(Segment::Hole(*span)),
            }
        }
        segments
    }

    /// Evaluate a block scalar: the body text with holes evaluated in place
    /// (holes are render-suppressed into the block text, so everything
    /// attributes at the block's own position). Region-opening holes are
    /// skipped here; the region itself is a CST child of the block's entry
    /// and contributes its condition reads there.
    pub(super) fn eval_block_scalar(&mut self, block: &BlockScalar) -> Guarded<AbstractFragment> {
        let mut arms: Vec<(PathCondition, Vec<StringPart>)> = vec![(Predicate::True, Vec::new())];
        let mut cursor = block.body.start;
        for hole in &block.holes {
            if hole.start > cursor
                && let Some(text) = self.source.get(cursor..hole.start)
                && !text.is_empty()
            {
                let text_arm = vec![(
                    Predicate::True,
                    vec![StringPart::Text([text.to_string()].into_iter().collect())],
                )];
                arms = combine_scalar_arms(arms, text_arm);
            }
            if !self.control_facts.contains_key(&hole.start) {
                let hole_arms = self.eval_hole_parts(self.text(*hole));
                arms = combine_scalar_arms(arms, hole_arms);
            }
            cursor = hole.end.max(cursor);
        }
        if block.body.end > cursor
            && let Some(text) = self.source.get(cursor..block.body.end)
            && !text.is_empty()
        {
            let text_arm = vec![(
                Predicate::True,
                vec![StringPart::Text([text.to_string()].into_iter().collect())],
            )];
            arms = combine_scalar_arms(arms, text_arm);
        }
        scalar_arms_to_fragment(arms, true)
    }

    /// Evaluate an inline `{{ if }}…{{ end }}` or `{{ range }}…{{ end }}`
    /// region inside a scalar by re-parsing the region text with the
    /// Go-template grammar and turning its branches into guarded scalar
    /// arms. Other inline regions (`with`) and nested regions degrade to
    /// conservative taint.
    pub(super) fn eval_inline_region(
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
            .find(|child| matches!(child.kind(), "if_action" | "range_action"))
        else {
            return self.inline_region_taint(text);
        };
        if action.kind() == "range_action" {
            return self.eval_inline_range(action, text);
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
        for (header, children) in arm_specs {
            self.active_predicates.truncate(entry_predicates);
            let mut arm_condition = Predicate::True;
            for predicate in &prior {
                let negated = predicate.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions(arm_condition, negated);
            }
            if let Some(own) = self.activate_inline_if(header.as_ref()) {
                arm_condition = and_conditions(arm_condition, own.clone());
                prior.push(own);
            }
            for (sub_condition, parts) in self.inline_body_arms(&children, text) {
                arms.push((and_conditions(arm_condition.clone(), sub_condition), parts));
            }
        }
        self.active_predicates.truncate(entry_predicates);
        if arms.len() > MAX_SCALAR_ARMS {
            let parts = arms.into_iter().flat_map(|(_, parts)| parts).collect();
            return vec![(Predicate::True, parts)];
        }
        arms
    }

    /// Evaluate an inline `{{ range }}…{{ end }}` region inside a scalar
    /// with the structural range activation: literal-list domains, the
    /// direct-path item dot, and the header read under `Guard::Range`; body
    /// contributions carry the range condition. Body-local bindings stay
    /// region-local (entry locals are restored, the same boundary as a
    /// structural branch scope).
    fn eval_inline_range(
        &mut self,
        node: tree_sitter::Node<'_>,
        text: &str,
    ) -> Vec<(PathCondition, Vec<StringPart>)> {
        let Some(header) = helm_schema_ast::range_header_from_source(node, text) else {
            return self.inline_region_taint(text);
        };
        let entry_predicates = self.active_predicates.len();
        let entry_dots = self.dot_stack.len();
        let entry_locals = self.locals.clone();
        if let Some((variable, literals)) = parse_literal_list_range_expr(header.expr()) {
            self.locals.insert_range_domain(variable, literals);
        }
        let (source_paths, direct_path) = {
            let context = self.value_path_context();
            (
                context
                    .resolved_values_paths_from_expr(header.expr())
                    .into_iter()
                    .collect::<Vec<_>>(),
                context.single_direct_iterable_range_path_expr(header.expr()),
            )
        };
        let mut own = Vec::new();
        for path in &source_paths {
            let guard = Guard::Range { path: path.clone() };
            self.push_read(path, std::slice::from_ref(&guard));
            own.push(Predicate::from(guard.clone()));
            self.push_predicate(Predicate::from(guard));
        }
        let condition = Predicate::all(own);
        let dot = direct_path.map(|path| AbstractValue::ValuesPath(format!("{path}.*")));
        self.dot_stack.push(dot);
        let mut arms = Vec::new();
        for (sub_condition, parts) in
            self.inline_body_arms(&children_with_field(node, "body"), text)
        {
            arms.push((and_conditions(condition.clone(), sub_condition), parts));
        }
        self.dot_stack.truncate(entry_dots);
        self.active_predicates.truncate(entry_predicates);
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
    fn inline_body_arms(
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

    fn activate_inline_if(
        &mut self,
        header: Option<&helm_schema_ast::TemplateHeader>,
    ) -> Option<PathCondition> {
        let header = header?;
        let predicate = self
            .value_path_context()
            .condition_predicate_expr(header.expr());
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
    fn inline_child_arms(
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
                let hole = self.eval_hole_exprs(&exprs);
                self.absorb_hole_effects(&hole.effects, false);
                let defaulted = hole.effects.default_paths_with_local();
                let scope = LowerScope {
                    defaulted_paths: &defaulted,
                    encoded_paths: &hole.effects.encoded_paths,
                    chart_value_defaults: &self.locals.chart_value_defaults,
                    local_output_meta: &hole.effects.local_output_meta,
                };
                match &hole.value {
                    Some(value) => lower_value_scalar_arms(value, &scope),
                    None => Vec::new(),
                }
            }
            NodeAction::Assignment(Some(exprs)) => {
                self.eval_assignment_exprs(&exprs);
                Vec::new()
            }
            NodeAction::If(_) | NodeAction::With(_) | NodeAction::Range(_) => {
                // Nested inline control: keep the influence, drop the
                // structure (bounded conservative fallback).
                let content = node.utf8_text(text.as_bytes()).unwrap_or("");
                let taint = self.resolved_paths_of_action_text(content);
                if taint.is_empty() {
                    Vec::new()
                } else {
                    vec![(Predicate::True, vec![StringPart::Taint(taint)])]
                }
            }
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

    fn inline_region_taint(&mut self, text: &str) -> Vec<(PathCondition, Vec<StringPart>)> {
        let taint = self.resolved_paths_of_action_text(text);
        if taint.is_empty() {
            return Vec::new();
        }
        vec![(Predicate::True, vec![StringPart::Taint(taint)])]
    }

    fn resolved_paths_of_action_text(&mut self, text: &str) -> std::collections::BTreeSet<String> {
        let mut paths = std::collections::BTreeSet::new();
        for expr in parse_action_expressions(text) {
            paths.extend(
                self.value_path_context()
                    .resolved_values_paths_from_expr(&expr),
            );
        }
        paths
    }

    /// Assignment actions: bind the local (fragment semantics), refresh its
    /// default/meta facts, and record the right-hand side's reads — the
    /// current pipeline walks assignment bodies in a no-render scope, so all
    /// of its claims are pathless.
    pub(super) fn eval_assignment_text(&mut self, text: &str) {
        let exprs = parse_expr_text(text);
        if !exprs.is_empty() {
            self.eval_assignment_exprs(&exprs);
        }
    }

    pub(super) fn eval_assignment_exprs(&mut self, exprs: &[TemplateExpr]) {
        if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
            let locals = locals_with_roots(&self.locals.fragment_values, &self.root_bindings);
            let current_dot = self
                .current_dot_binding()
                .map(|value| value.to_context_value());
            let mut seen = HashSet::new();
            let fragment_value = FragmentEvalContext::new(self.db).fragment_value_from_expr(
                &assignment.rhs_expr,
                &locals,
                current_dot.as_ref(),
                &mut seen,
            );
            self.locals.bind_fragment_value(
                assignment.kind,
                assignment.variable.clone(),
                fragment_value,
            );

            let rhs = std::slice::from_ref(&assignment.rhs_expr);
            let output_effects = self.value_path_context().expression_output_effects(rhs);
            let hole = self.eval_hole_exprs(rhs);
            let mut output_meta = output_effects.local_output_meta.clone();
            merge_output_use_meta(&mut output_meta, &hole.effects.helper_summary.output_uses);
            self.locals
                .set_default_paths(&assignment.variable, output_effects.defaults.clone());
            self.locals
                .set_output_meta(assignment.variable.clone(), output_meta);
            self.absorb_hole_effects(&hole.effects, true);
            self.push_effects_reads(&hole);
        }
        if let Some(get_binding) = parse_get_binding_from_exprs(exprs) {
            self.locals.apply_get_binding(get_binding);
        }
    }
}

/// Split a hole's evaluation into the value to lower and the extra effect
/// paths that attribute at the hole beyond the value's own paths (condition
/// operands of `ternary`/`and`/`or`, shallow local sources, …) — the
/// current pipeline emits every expression output path at the slot, so the
/// projection keeps that rule. At scalar sites, ancestor paths with a more
/// specific path in the same hole are dropped (the pipeline's
/// most-specific-path retain rule for scalar slots).
fn prepare_hole_value(
    value: Option<AbstractValue>,
    effects: &Effects,
    scalar_site: bool,
) -> (Option<AbstractValue>, Vec<String>) {
    let value_paths = value.as_ref().map(AbstractValue::paths).unwrap_or_default();
    let effect_paths = effects.output_value_paths();
    let all: std::collections::BTreeSet<String> = value_paths
        .iter()
        .chain(effect_paths.iter())
        .filter(|path| !path.is_empty())
        .cloned()
        .collect();
    let drop: std::collections::BTreeSet<String> = if scalar_site {
        all.iter()
            .filter(|path| helm_schema_core::values_path_has_descendant(path, &all))
            .cloned()
            .collect()
    } else {
        std::collections::BTreeSet::new()
    };
    let value = value.and_then(|value| value.remove_fragment_paths(&drop));
    let extras = effect_paths
        .into_iter()
        .filter(|path| !path.is_empty() && !value_paths.contains(path) && !drop.contains(path))
        .collect();
    (value, extras)
}

/// The single hole of a scalar run that covers the entire value (allowing a
/// wrapping quote pair), or `None` for genuinely partial scalars.
fn entire_hole_span(segments: &[Segment]) -> Option<Span> {
    let mut hole = None;
    let mut prefix = String::new();
    let mut suffix = String::new();
    for segment in segments {
        match segment {
            Segment::Region(_) => return None,
            Segment::Hole(span) => {
                if hole.is_some() {
                    return None;
                }
                hole = Some(*span);
            }
            Segment::Text(text) => {
                if hole.is_none() {
                    prefix.push_str(text);
                } else {
                    suffix.push_str(text);
                }
            }
        }
    }
    let hole = hole?;
    matches!(
        (prefix.trim(), suffix.trim()),
        ("", "") | ("\"", "\"") | ("'", "'")
    )
    .then_some(hole)
}

/// Whether an action hole is a control-flow fragment (`{{ if … }}`,
/// `{{ else }}`, `{{ end }}`, …) rather than an output expression. These
/// appear as bare holes inside block-scalar bodies where the region
/// structure itself is represented separately.
fn hole_is_control_fragment(text: &str) -> bool {
    let mut inner = text.trim();
    if let Some(rest) = inner.strip_prefix("{{") {
        inner = rest.trim_start_matches('-').trim_start();
    }
    matches!(
        inner.split_whitespace().next(),
        Some("if" | "else" | "end" | "range" | "with" | "define" | "block")
    )
}

fn combine_scalar_arms(
    base: Vec<(PathCondition, Vec<StringPart>)>,
    segment: Vec<(PathCondition, Vec<StringPart>)>,
) -> Vec<(PathCondition, Vec<StringPart>)> {
    if segment.is_empty() {
        return base;
    }
    if base.len().saturating_mul(segment.len()) > MAX_SCALAR_ARMS {
        // Bounded fallback: fold the segment's alternatives into one
        // contribution set and drop their conditions.
        let union: Vec<StringPart> = segment.into_iter().flat_map(|(_, parts)| parts).collect();
        return base
            .into_iter()
            .map(|(condition, mut parts)| {
                parts.extend(union.iter().cloned());
                (condition, parts)
            })
            .collect();
    }
    let mut out = Vec::new();
    for (base_condition, base_parts) in &base {
        for (segment_condition, segment_parts) in &segment {
            let mut parts = base_parts.clone();
            parts.extend(segment_parts.iter().cloned());
            out.push((
                and_conditions(base_condition.clone(), segment_condition.clone()),
                parts,
            ));
        }
    }
    out
}

fn scalar_arms_to_fragment(
    arms: Vec<(PathCondition, Vec<StringPart>)>,
    suppressed: bool,
) -> Guarded<AbstractFragment> {
    let mut out = Guarded::empty();
    for (condition, parts) in arms {
        out.arms.push((
            condition,
            AbstractFragment::Scalar(AbstractString { parts, suppressed }),
        ));
    }
    out
}
