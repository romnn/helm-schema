//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use helm_schema_ast::{TemplateExpr, parse_expr_text};
use helm_schema_syntax::{BlockScalar, ScalarPart, ScalarParts, Span};

use crate::ValueKind;
use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::expr_eval::literal_helper_call_callee;
use crate::fragment_assignment::parse_helper_assignment_from_exprs;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::merge_rendered_row_meta;
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, StringPart, and_conditions,
    stamp_fragment_sites, stamp_part_sites,
};
use super::eval::Interpreter;
use super::hole_effects::RenderedDemotion;
use super::lower::{
    LowerScope, MAX_SCALAR_ARM_FANOUT, MAX_SCALAR_ARMS, lower_value, lower_value_scalar_arms,
};
use super::summary::splice_summary;

pub(super) struct HoleEval {
    pub(super) value: Option<AbstractValue>,
    pub(super) effects: Effects,
}

/// Whether an expression invokes `fail` anywhere: evaluating it terminates
/// template rendering unconditionally.
pub(super) fn expr_contains_fail_call(expr: &TemplateExpr) -> bool {
    let mut found = false;
    expr.walk(|inner| {
        if let TemplateExpr::Call { function, .. } = inner
            && function == "fail"
        {
            found = true;
        }
    });
    found
}

/// One layout segment of a scalar run: literal text, a template hole, or a
/// whole inline control region (grouping the region's holes and texts).
enum Segment {
    Text(String),
    Hole(Span),
    Region(Span),
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

/// The single hole of a scalar run that covers the entire value, or `None`
/// when literal text makes the hole a partial scalar.
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
    (prefix.trim().is_empty() && suffix.trim().is_empty()).then_some(hole)
}

/// The static YAML key of a `printf "key: %s" …` hole (the format's leading
/// mapping key), when the hole is exactly one such printf.
fn static_printf_yaml_key(exprs: &[TemplateExpr]) -> Option<String> {
    fn printf_format(expr: &TemplateExpr) -> Option<&str> {
        match expr {
            TemplateExpr::Parenthesized(inner) => printf_format(inner),
            TemplateExpr::Call { function, args } if function == "printf" => match args.first()? {
                TemplateExpr::Literal(
                    helm_schema_ast::Literal::String(format)
                    | helm_schema_ast::Literal::RawString(format),
                ) => Some(format),
                _ => None,
            },
            TemplateExpr::Pipeline(stages) => stages.first().and_then(printf_format),
            _ => None,
        }
    }

    let [expr] = exprs else {
        return None;
    };
    let format = printf_format(expr)?;
    helm_schema_ast::parse_yaml_key(format.trim_start())
}

/// The literal helper call a hole splices whole: exactly one expression
/// that is an `include`/`template` call with a literal name, either bare or
/// piped only through indent shaping (`nindent`/`indent`), which relocates
/// the fragment without transforming it.
fn splice_target_helper_call(exprs: &[TemplateExpr]) -> Option<(&str, Option<&TemplateExpr>)> {
    let [expr] = exprs else {
        return None;
    };
    let call = match expr.deparen() {
        TemplateExpr::Pipeline(stages) => {
            let (first, rest) = stages.split_first()?;
            if !rest.iter().all(|stage| {
                matches!(
                    stage.deparen(),
                    TemplateExpr::Call { function, .. }
                        if matches!(function.as_str(), "nindent" | "indent")
                )
            }) {
                return None;
            }
            first.deparen()
        }
        other => other,
    };
    let TemplateExpr::Call { function, args } = call else {
        return None;
    };
    let name = literal_helper_call_callee(function, args)?;
    Some((name, args.get(1)))
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
        // Bounded fallback: drop the cross-segment correlation but keep
        // every contribution under its own conditions (projection reads
        // per-part attribution, not reconstructed text).
        let mut arms = base;
        arms.extend(segment);
        if arms.len() > MAX_SCALAR_ARM_FANOUT {
            let parts = arms.into_iter().flat_map(|(_, parts)| parts).collect();
            return vec![(Predicate::True, parts)];
        }
        return arms;
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

impl Interpreter<'_> {
    /// Evaluate a hole standing as an entire fragment position.
    pub(super) fn eval_entire_hole(&mut self, span: Span) -> Guarded<AbstractFragment> {
        self.eval_output_action(span).0
    }

    /// Evaluate a standalone output action: the lowered fragment plus the
    /// action's explicit rendered indent (`… | nindent N`), which decides
    /// which enclosing container the output attaches to.
    pub(super) fn eval_output_action(
        &mut self,
        span: Span,
    ) -> (Guarded<AbstractFragment>, Option<usize>) {
        let text = self.text(span);
        if hole_is_control_fragment(text) {
            return (Guarded::empty(), None);
        }
        let exprs = parse_expr_text(text);
        if exprs.is_empty() {
            return (Guarded::empty(), None);
        }
        let previous_site = self.enter_hole_site(span);
        if parse_helper_assignment_from_exprs(&exprs).is_some() {
            self.eval_assignment_exprs(&exprs);
            self.restore_site(previous_site);
            return (Guarded::empty(), None);
        }
        if self.apply_helper_scope_set_mutations(&exprs) {
            self.restore_site(previous_site);
            return (Guarded::empty(), None);
        }
        // A `fail` hole terminates rendering: no valid values document may
        // satisfy the guards active here, and the action renders nothing.
        if exprs.iter().any(expr_contains_fail_call) {
            self.record_fail_condition();
            self.restore_site(previous_site);
            return (Guarded::empty(), None);
        }
        self.record_required_subjects(&exprs);
        let inlined = self.inline_static_file_fragments(&exprs);
        let width = exprs
            .iter()
            .rev()
            .find_map(TemplateExpr::fragment_indent_width);
        let kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
            ValueKind::Fragment
        } else {
            ValueKind::Scalar
        };
        if let Some(spliced) = self.splice_helper_call_hole(&exprs) {
            let mut out = spliced;
            out.extend(inlined);
            self.restore_site(previous_site);
            return (out, width);
        }
        let hole = self.eval_hole_exprs(&exprs);
        self.absorb_hole_effects(&hole.effects, RenderedDemotion::None);
        let (value, extra_paths) =
            prepare_hole_value(hole.value, &hole.effects, kind == ValueKind::Scalar);
        let defaulted = hole.effects.default_paths_with_local();
        // Direct helper flows collapsed by transfer functions (printf over
        // include) keep their per-path branch meta: the summary's rendered
        // rows merge with the locals' binding-time meta for lowering.
        let mut hole_meta = hole.effects.local_output_meta.clone();
        merge_rendered_row_meta(&mut hole_meta, &hole.effects.helper_rendered);
        // An APPROXIMATELY-lowered enclosing condition gates this hole:
        // its rows' branch keys stand in for a guard the encoding cannot
        // represent, so a string contract riding them would narrow states
        // the real branch never reaches.
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
        let mut out = match &value {
            Some(value) => lower_value(value, kind, &scope),
            None => Guarded::empty(),
        };
        for path in extra_paths {
            for (condition, splice) in scope.path_splice_arms(&path, kind) {
                out.arms.push((condition, AbstractFragment::Splice(splice)));
            }
        }
        // A `printf "key: %s" …` hole renders a mapping entry as text: the
        // rendered content belongs under the format's static key (the
        // summary lane's static-key rule for helper bodies).
        if self.helper_scope
            && let Some(key) = static_printf_yaml_key(&exprs)
            && !out.is_empty()
        {
            out = Guarded::unconditional(AbstractFragment::Mapping(super::domain::Mapping {
                entries: vec![super::domain::MappingEntry {
                    key: super::domain::EntryKey::Literal(key),
                    value: out,
                }],
            }));
        }
        stamp_fragment_sites(&mut out, &self.current_site);
        out.extend(inlined);
        self.restore_site(previous_site);
        (out, width)
    }

    /// Splice a bound helper call's summary fragment at an entire-hole
    /// position. Fires for the plain call shape (`include`/`template` with a
    /// literal name, alone or piped only through indent shaping): the
    /// summary's fragment lands under the hole's slot, its body sites rebase
    /// onto the call site, and its reads/hints absorb here. Other shapes
    /// (encodings, transfer functions, dynamic names, unresolved helpers)
    /// keep evaluating through the value lattice.
    fn splice_helper_call_hole(
        &mut self,
        exprs: &[TemplateExpr],
    ) -> Option<Guarded<AbstractFragment>> {
        let (name, arg) = splice_target_helper_call(exprs)?;
        if !self.db.has_helper(name) || self.helper_seen.contains(name) {
            return None;
        }
        let name = name.to_string();
        let current_dot = self.current_value_dot();
        let mut helper_locals = self.locals.range_member_values.clone();
        helper_locals.extend(
            self.locals
                .fragment_values
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        let mut seen = self.helper_seen.clone();
        let call = self.db.summarize_bound_helper_call(
            &name,
            arg,
            Some(&self.root_bindings),
            current_dot.as_ref(),
            &helper_locals,
            FragmentEvalContext::new(self.db),
            &mut seen,
        );
        self.absorb_hole_effects(&call.argument_effects, RenderedDemotion::Dependency);
        let summary = &call.summary;
        let suppressed: std::collections::BTreeSet<&String> = summary
            .rendered
            .iter()
            .flat_map(|row| row.meta.suppress_predicate_paths.iter())
            .chain(summary.suppress_predicate_paths.iter())
            .collect();
        let mut claims: std::collections::BTreeSet<String> = summary
            .reads
            .iter()
            .map(|read| read.values_path.clone())
            .collect();
        claims.extend(summary.rendered.iter().map(|row| row.path.clone()));
        self.absorb_helper_reads_with_suppression(&summary.reads, &suppressed, &claims);
        self.absorb_helper_fails(&summary.fail_conditions);
        self.absorb_member_host_conversions(&summary.member_host_conversions);
        for (path, hints) in &summary.type_hints {
            if path.trim().is_empty() {
                continue;
            }
            let sink = if self.hint_scope_is_unconditional(path) {
                &mut self.type_hints
            } else {
                &mut self.guarded_type_hints
            };
            sink.entry(path.clone())
                .or_default()
                .extend(hints.iter().cloned());
        }
        for (path, hints) in &summary.guarded_type_hints {
            if path.trim().is_empty() {
                continue;
            }
            self.guarded_type_hints
                .entry(path.clone())
                .or_default()
                .extend(hints.iter().cloned());
        }
        for (path, hints) in &summary.fallback_type_hints {
            if path.trim().is_empty() {
                continue;
            }
            let sink = if self.hint_scope_is_unconditional(path) {
                &mut self.fallback_type_hints
            } else {
                // Branch-scoped fallback hints keep their fallback identity
                //: overlay lowering must know they are intent, not a
                // consumer contract.
                &mut self.guarded_fallback_type_hints
            };
            sink.entry(path.clone())
                .or_default()
                .extend(hints.iter().cloned());
        }
        self.shape_erased_paths
            .extend(summary.shape_erased_paths.iter().cloned());
        self.yaml_serialized_paths
            .extend(summary.yaml_serialized_paths.iter().cloned());
        self.string_contract_paths
            .extend(summary.string_contract_paths.iter().cloned());
        self.range_modes.merge(&summary.range_modes);
        self.chart_defaults_observed
            .extend(summary.chart_defaults.iter().cloned());
        self.apply_root_set_mutations(&summary.root_set_mutations, &summary.root_set_predicates);
        self.values_default_sources_observed
            .extend(summary.values_default_sources.iter().cloned());
        self.values_root_helper_includes_observed
            .extend(summary.values_root_helper_includes.iter().cloned());
        let mut chart_defaults = summary.chart_defaults.clone();
        self.locals.append_chart_value_defaults(&mut chart_defaults);
        Some(splice_summary(summary, &self.current_site))
    }

    /// Evaluate a hole rendered inside a partial scalar: guarded arms of
    /// string parts.
    pub(super) fn eval_hole_parts(&mut self, span: Span) -> Vec<(PathCondition, Vec<StringPart>)> {
        let text = self.text(span);
        if hole_is_control_fragment(text) {
            return Vec::new();
        }
        let exprs = parse_expr_text(text);
        if exprs.is_empty() {
            return Vec::new();
        }
        let previous_site = self.enter_hole_site(span);
        if parse_helper_assignment_from_exprs(&exprs).is_some() {
            self.eval_assignment_exprs(&exprs);
            self.restore_site(previous_site);
            return Vec::new();
        }
        if self.apply_helper_scope_set_mutations(&exprs) {
            self.restore_site(previous_site);
            return Vec::new();
        }
        if exprs.iter().any(expr_contains_fail_call) {
            self.record_fail_condition();
            self.restore_site(previous_site);
            return Vec::new();
        }
        self.record_required_subjects(&exprs);
        let _ = self.inline_static_file_fragments(&exprs);
        // Fragment-rendering holes (`toYaml … | nindent`) keep fragment
        // evidence even inside scalar text; everything else is a partial
        // scalar contribution.
        let kind = if exprs.iter().any(TemplateExpr::renders_yaml_fragment) {
            ValueKind::Fragment
        } else {
            ValueKind::PartialScalar
        };
        let hole = self.eval_hole_exprs(&exprs);
        self.absorb_hole_effects(&hole.effects, RenderedDemotion::None);
        let (value, extra_paths) =
            prepare_hole_value(hole.value, &hole.effects, kind != ValueKind::Fragment);
        let defaulted = hole.effects.default_paths_with_local();
        // Direct helper flows collapsed by transfer functions (printf over
        // include) keep their per-path branch meta: the summary's rendered
        // rows merge with the locals' binding-time meta for lowering.
        let mut hole_meta = hole.effects.local_output_meta.clone();
        merge_rendered_row_meta(&mut hole_meta, &hole.effects.helper_rendered);
        // An APPROXIMATELY-lowered enclosing condition gates this hole:
        // its rows' branch keys stand in for a guard the encoding cannot
        // represent, so a string contract riding them would narrow states
        // the real branch never reaches.
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
        let mut arms = match &value {
            Some(value) => lower_value_scalar_arms(value, kind, &scope),
            None => Vec::new(),
        };
        let mut plain_parts: Vec<StringPart> = Vec::new();
        for path in extra_paths {
            for (condition, splice) in scope.path_splice_arms(&path, kind) {
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
        for (_, parts) in &mut arms {
            stamp_part_sites(parts, &self.current_site);
        }
        self.restore_site(previous_site);
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
            return self.eval_entire_hole(span);
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
                Segment::Hole(span) => self.eval_hole_parts(span),
                Segment::Region(span) => self.eval_inline_region(span),
            };
            arms = combine_scalar_arms(arms, segment_arms);
        }
        self.record_completed_token_contracts(&arms);
        scalar_arms_to_fragment(arms, false)
    }

    /// Completed-token contracts of a partial scalar: raw inputs that
    /// corrupt the ASSEMBLED YAML token abort rendering, so they become fail
    /// captures under the ambient conditions (the absorb site prepends
    /// them):
    /// - a raw splice OPENING an unquoted token (`image: {{ x }}/…`) breaks
    ///   on a list value, whose rendering opens a flow sequence there
    ///   (tempo's assembled image scalar);
    /// - a raw splice inside MANUAL double quotes (`image: "{{ x }}/…"`,
    ///   also inside flow content) breaks on strings whose text is not
    ///   valid double-quoted YAML content — an unescaped `"`, or a `\` that
    ///   does not begin a YAML escape sequence. Raw `\"`/`\\` sequences are
    ///   valid escapes and render (zalando's manually quoted image scalar);
    /// - a raw splice inside MANUAL single quotes breaks on strings whose
    ///   every `'` is not doubled (`''` is the only escape in single-quoted
    ///   YAML);
    /// - in both quoted contexts a COLLECTION value renders through Go's
    ///   fmt (`map[k:v]` / `[a b]`) with its nested strings and mapping
    ///   keys embedded raw, so those must satisfy the same content grammar
    ///   (zalando's map-valued registry inside manual quotes).
    ///
    /// The quote context comes from a scanner over the PRECEDING literal
    /// text (a state machine over `"`/`'`/escapes), so flow-style content
    /// (`[ "prefix{{ x }}" ]`) claims the same contract as a whole quoted
    /// token; a quote-safe splice value cannot close the context it sits in.
    /// Only splices whose rendered text IS the raw value claim (transforms
    /// like `quote`, `b64enc`, or `trunc` reshape the text), and a path
    /// claims only when EVERY scalar arm agrees — the arms partition what
    /// the token renders, so a path present at the position in all of them
    /// provably reaches it.
    fn record_completed_token_contracts(&mut self, arms: &[(PathCondition, Vec<StringPart>)]) {
        #[derive(Clone, Copy, PartialEq)]
        enum QuoteContext {
            None,
            Double,
            Single,
        }

        fn advance_quote_context(mut state: QuoteContext, text: &str) -> QuoteContext {
            let mut chars = text.chars().peekable();
            while let Some(character) = chars.next() {
                state = match (state, character) {
                    (QuoteContext::None, '"') => QuoteContext::Double,
                    (QuoteContext::None, '\'') => QuoteContext::Single,
                    (QuoteContext::Double, '"') => QuoteContext::None,
                    (QuoteContext::Double, '\\') => {
                        chars.next();
                        QuoteContext::Double
                    }
                    (QuoteContext::Single, '\'') => {
                        if chars.peek() == Some(&'\'') {
                            chars.next();
                            QuoteContext::Single
                        } else {
                            QuoteContext::None
                        }
                    }
                    (state, _) => state,
                };
            }
            state
        }

        #[derive(Default)]
        struct ArmClaims {
            token_initial: std::collections::BTreeSet<String>,
            double_quoted: std::collections::BTreeSet<String>,
            single_quoted: std::collections::BTreeSet<String>,
        }

        fn arm_claims(parts: &[StringPart]) -> ArmClaims {
            let mut claims = ArmClaims::default();
            let mut state = QuoteContext::None;
            let mut preceding_text = false;
            for (index, part) in parts.iter().enumerate() {
                match part {
                    StringPart::Text(alternatives) => {
                        preceding_text |= alternatives.iter().any(|text| !text.is_empty());
                        // Alternative texts must agree on the context they
                        // leave behind, or the position claims nothing.
                        let mut states = alternatives
                            .iter()
                            .map(|text| advance_quote_context(state, text));
                        let Some(first) = states.next() else {
                            continue;
                        };
                        state = if states.all(|next| next == first) {
                            first
                        } else {
                            return claims;
                        };
                    }
                    StringPart::Splice(splice) => {
                        let raw = splice.kind == ValueKind::PartialScalar
                            && !splice.meta.encoded
                            && !splice.meta.shape_erased
                            && !splice.meta.yaml_serialized
                            && !splice.meta.string_contract
                            && !splice.meta.json_serialized
                            && splice.meta.split_segment.is_none()
                            && !splice.values_path.is_empty();
                        if !raw {
                            continue;
                        }
                        match state {
                            QuoteContext::Double => {
                                claims.double_quoted.insert(splice.values_path.clone());
                            }
                            QuoteContext::Single => {
                                claims.single_quoted.insert(splice.values_path.clone());
                            }
                            QuoteContext::None
                                if index == 0 && !preceding_text && !splice.meta.defaulted =>
                            {
                                // A defaulted splice exempts itself: every
                                // Helm-falsy input (the empty list included)
                                // renders the fallback instead of the raw
                                // value.
                                claims.token_initial.insert(splice.values_path.clone());
                            }
                            QuoteContext::None => {}
                        }
                    }
                    StringPart::Taint(_) => {}
                }
            }
            claims
        }

        let mut per_arm = arms.iter().map(|(_, parts)| arm_claims(parts));
        let Some(mut agreed) = per_arm.next() else {
            return;
        };
        for arm in per_arm {
            agreed
                .token_initial
                .retain(|path| arm.token_initial.contains(path));
            agreed
                .double_quoted
                .retain(|path| arm.double_quoted.contains(path));
            agreed
                .single_quoted
                .retain(|path| arm.single_quoted.contains(path));
        }
        let mut captures = Vec::new();
        for path in agreed.token_initial {
            captures.push(crate::eval_effect::FailCapture {
                conjunction: vec![Predicate::from(crate::Guard::TypeIs {
                    path,
                    schema_type: "array".to_string(),
                })],
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::Fail,
            });
        }
        for (paths, style) in [
            (
                agreed.double_quoted,
                helm_schema_core::QuotedScalarStyle::Double,
            ),
            (
                agreed.single_quoted,
                helm_schema_core::QuotedScalarStyle::Single,
            ),
        ] {
            for path in paths {
                captures.push(crate::eval_effect::FailCapture {
                    conjunction: Vec::new(),
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::QuotedSerialization { path, style },
                });
            }
        }
        if !captures.is_empty() {
            self.absorb_helper_fails(&captures);
        }
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
    /// attributes at the block's own position). Region-opening holes whose
    /// region stays inside the block evaluate as inline regions (block
    /// content never becomes CST control structure); regions extending past
    /// the block are represented as CST children of the block's entry and
    /// contribute their condition reads there.
    pub(super) fn eval_block_scalar(&mut self, block: &BlockScalar) -> Guarded<AbstractFragment> {
        let mut arms: Vec<(PathCondition, Vec<StringPart>)> = vec![(Predicate::True, Vec::new())];
        let mut cursor = block.body.start;
        for hole in &block.holes {
            if hole.start < cursor {
                continue;
            }
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
            match self.body_facts.control_facts.get(&hole.start) {
                Some(facts) if facts.region_end <= block.body.end => {
                    let region = Span {
                        start: hole.start,
                        end: facts.region_end,
                    };
                    let region_arms = self.eval_inline_region(region);
                    arms = combine_scalar_arms(arms, region_arms);
                    cursor = region.end;
                    continue;
                }
                Some(facts) => {
                    if facts.is_range {
                        // YAML block spans can exclude trim-only closing
                        // actions even though the template range is wholly
                        // contained in the scalar. Evaluate the parsed range
                        // for its body contracts; the scalar text remains
                        // owned by the block lowering above.
                        let _ = self.eval_inline_region(Span {
                            start: hole.start,
                            end: facts.region_end,
                        });
                    }
                }
                None => {
                    if parse_expr_text(self.text(*hole))
                        .iter()
                        .any(TemplateExpr::renders_yaml_fragment)
                    {
                        // A fragment render suppressed into block text: the
                        // helper rows and value paths are the semantic facts
                        // (with their own kinds); the text stays opaque.
                        self.eval_suppressed_fragment_hole(*hole);
                    } else {
                        let hole_arms = self.eval_hole_parts(*hole);
                        arms = combine_scalar_arms(arms, hole_arms);
                    }
                }
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

    /// A fragment-rendering hole inside a render-suppressed blob: rendered
    /// helper rows become pathless reads that keep their kinds, and direct
    /// value paths read with the hole's fragment kind.
    fn eval_suppressed_fragment_hole(&mut self, span: Span) {
        let exprs = parse_expr_text(self.text(span));
        if exprs.is_empty() {
            return;
        }
        let previous_site = self.enter_hole_site(span);
        if exprs.iter().any(expr_contains_fail_call) {
            self.record_fail_condition();
            self.restore_site(previous_site);
            return;
        }
        self.record_required_subjects(&exprs);
        let _ = self.inline_static_file_fragments(&exprs);
        let hole = self.eval_hole_exprs(&exprs);
        self.absorb_hole_effects(&hole.effects, RenderedDemotion::Document);
        self.push_effects_reads(&hole, ValueKind::Fragment);
        self.restore_site(previous_site);
    }
}
