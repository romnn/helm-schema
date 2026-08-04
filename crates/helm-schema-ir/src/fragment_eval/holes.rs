//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use std::collections::BTreeSet;

use helm_schema_ast::{TemplateExpr, parse_expr_text};
use helm_schema_syntax::{BlockScalar, ScalarPart, ScalarParts, Span};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::Effects;
use crate::expr_eval::literal_helper_call_callee;
use crate::fragment_assignment::parse_helper_assignment_from_exprs;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::merge_rendered_row_meta;
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use crate::{Guard, ValueKind};
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, StringPart, and_conditions,
    stamp_fragment_sites, stamp_part_sites,
};
use super::eval::Interpreter;
use super::hole_effects::RenderedDemotion;
use super::lower::{
    LowerScope, MAX_SCALAR_ARM_FANOUT, MAX_SCALAR_ARMS, lower_scalar_dispatch,
    lower_scalar_dispatch_arms, lower_value, lower_value_scalar_arms,
};
use super::summary::splice_summary;

pub(super) struct HoleEval {
    pub(super) value: Option<AbstractValue>,
    pub(super) effects: Effects,
    pub(super) truth: TruthCondition,
    pub(super) json_payload_truth: TruthCondition,
    pub(super) scalar_dispatch: Option<ScalarValueDispatch>,
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
/// projection keeps that rule. Ancestor paths with a more specific path in
/// the same scalar hole are dropped. Fragment holes do the same when a helper
/// summary identifies the descendant as rendered output: the ancestor is
/// then an execution effect, not a second value reaching the sink.
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
    let drop: std::collections::BTreeSet<String> = all
        .iter()
        .filter(|path| {
            helm_schema_core::values_path_has_descendant(path, &all)
                && (scalar_site
                    || effects.helper_rendered.iter().any(|row| {
                        value_paths.contains(&row.path)
                            && helm_schema_core::values_path_is_descendant(&row.path, path)
                    }))
        })
        .cloned()
        .collect();
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
    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
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
        let document_root =
            !self.helper_scope && width.is_none() && self.line_indent(span.start) == 0;
        if let Some((spliced, root_render_indent)) = self.splice_helper_call_hole(&exprs) {
            let mut out = spliced;
            let width = match (width, root_render_indent) {
                (Some(width), Some(root_render_indent)) => {
                    Some(width.saturating_add(root_render_indent))
                }
                (None, _) => None,
                (width, None) => width,
            };
            // A helper spliced at column zero renders its own document, so the
            // rule below reaches whatever ranged member its body emits WHOLE
            // there, under that arm's own conditions (traefik and Sealed
            // Secrets route each `extraObjects`/`extraDeploy` member through a
            // `<chart>.render` helper whose `typeIs "string"` arms decide
            // between the member's raw text and its serialization).
            if document_root {
                for (condition, fragment) in &out.arms.clone() {
                    if let AbstractFragment::Splice(splice) = fragment
                        && splice.kind == ValueKind::Fragment
                        && splice.values_path.ends_with(".*")
                    {
                        let path = splice.values_path.clone();
                        self.record_document_root_mapping(&path, vec![condition.clone()]);
                    }
                }
            }
            out.extend(inlined);
            self.restore_site(previous_site);
            return (out, width);
        }
        let hole = self.eval_hole_exprs(&exprs);
        if self.helper_scope && hole.json_payload_truth != TruthCondition::Unknown {
            self.json_payload_truth_outputs.push((
                Predicate::all(self.active_predicates.clone()),
                hole.json_payload_truth.clone(),
            ));
        }
        self.absorb_hole_effects(&hole.effects, RenderedDemotion::None);
        let (value, extra_paths) =
            prepare_hole_value(hole.value, &hole.effects, kind == ValueKind::Scalar);
        if document_root
            && kind == ValueKind::Fragment
            && let Some(path) = value
                .as_ref()
                .and_then(AbstractValue::direct_values_identity)
            && path.ends_with(".*")
        {
            self.record_document_root_mapping(&path, Vec::new());
        }
        // The hole IS the whole scalar of a VALUE slot here (a partial scalar
        // routes through `eval_hole_parts`), so it renders an unquoted plain
        // token: no literal text — quotes included — sits beside the hole.
        // Document-level content is excluded: there a `: ` is the manifest's
        // own structure, which is what the ranged-document dispatch renders.
        if kind == ValueKind::Scalar && self.in_value_slot {
            self.record_plain_slot_text(value.as_ref(), &hole.effects);
        }
        let defaulted = hole.effects.default_paths_with_local();
        // Direct helper flows collapsed by transfer functions (printf over
        // include) keep their per-path branch meta: the summary's rendered
        // rows merge with the locals' binding-time meta for lowering.
        let mut hole_meta = hole.effects.local_output_meta.clone();
        merge_rendered_row_meta(&mut hole_meta, &hole.effects.helper_rendered);
        for (path, keys) in &hole.effects.omitted_map_keys {
            let meta = hole_meta.entry(path.clone()).or_default();
            for key in keys {
                meta.omitted_keys.insert(key.clone(), Vec::new());
            }
        }
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
        let mut out =
            if kind == ValueKind::Scalar && (self.scalar_output_projection || self.helper_scope) {
                hole.scalar_dispatch
                    .as_ref()
                    .and_then(|dispatch| lower_scalar_dispatch(dispatch, kind, &scope))
            } else {
                None
            }
            .unwrap_or_else(|| match &value {
                Some(value) => lower_value(value, kind, &scope),
                None => Guarded::empty(),
            });
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
        stamp_fragment_sites(&mut out, self.current_site.as_ref());
        out.extend(inlined);
        self.restore_site(previous_site);
        (out, width)
    }

    /// Claim the document-root mapping shape for a ranged member spliced as a
    /// whole fragment at COLUMN ZERO with no explicit indent: it renders as
    /// document-root content, and Helm decodes every manifest as a mapping, so
    /// a present non-null member must be an object (nats renders each
    /// `extraResources` item as its own document; null members decode to empty
    /// manifests and are skipped). Helper bodies render at their caller's
    /// position and abstain — the caller records the claim for them.
    fn record_document_root_mapping(&mut self, path: &str, condition: Vec<PathCondition>) {
        let capture = crate::eval_effect::FailCapture {
            conjunction: self.fail_capture_conjunction(condition),
            ranged: self.capture_ranged_modes(),
            kind: crate::eval_effect::CaptureKind::ComparableKind {
                path: path.to_string(),
                schema_type: "object".to_string(),
            },
        };
        if !self.fail_conditions.contains(&capture) {
            self.fail_conditions.push(capture);
        }
    }

    /// Claim the unquoted-slot lexical language for the text an UNQUOTED
    /// scalar slot renders. Two identities reach it, and both render the raw
    /// value's own characters, so text that ends the plain token there — `: `,
    /// ` #`, a line break, or a leading indicator — corrupts the document:
    ///
    /// - a directly ranged collection's KEY (crossplane's
    ///   `- name: {{ $key | replace "." "_" }}`), whose claim binds the
    ///   collection's keys; a lexical escape rides along only when its token
    ///   and replacement cannot change the token-ending characters;
    /// - a `tpl` operand that IS a values path (external-dns's
    ///   `mountPath: {{ tpl .Values….mountPath $ }}`), which `tpl` renders
    ///   verbatim unless the value carries a template action.
    ///
    /// Both claims describe the TEXT this source renders, so a helper body
    /// defers them to whatever sink its caller splices the body into.
    fn record_plain_slot_text(&mut self, value: Option<&AbstractValue>, effects: &Effects) {
        // A later stage that reshapes the text (`b64enc`, `quote`) is what the
        // slot renders, so the raw value's own characters no longer reach it
        // (external-dns's `{{ tpl $value $ | b64enc | quote }}`).
        let reaches_slot = |path: &String| {
            !effects.shape_erased_paths.contains(path)
                && !effects.encoded_paths.contains(path)
                && !effects.yaml_serialized_paths.contains(path)
        };
        let mut captures = Vec::new();
        // A key the hole already converted (`{{ $key | quote }}`) renders the
        // conversion's text, not the key's own characters — ingress-nginx
        // quotes each `sysctls` key into the `name:` slot. The identity-
        // preserving `replace` re-adds its keys through the channel below.
        let mut key_paths = value
            .map(AbstractValue::range_key_paths)
            .unwrap_or_default();
        key_paths.retain(|path| !effects.derived_range_key_paths.contains(path));
        key_paths.extend(effects.plain_text_range_key_paths.iter().cloned());
        key_paths.retain(reaches_slot);
        if !key_paths.is_empty() {
            captures.push(crate::eval_effect::CaptureKind::RangeKeyPlainSlot { paths: key_paths });
        }
        if let Some(AbstractValue::ValuesPath(path)) = value
            && effects.templated_text_identity_paths.contains(path)
            && reaches_slot(path)
        {
            captures.push(crate::eval_effect::CaptureKind::PlainSlotText {
                path: path.clone(),
                token_initial: true,
                templated: true,
            });
        }
        for path in &effects.plain_text_preserving_paths {
            if reaches_slot(path) {
                captures.push(crate::eval_effect::CaptureKind::PlainSlotText {
                    path: path.clone(),
                    token_initial: true,
                    templated: false,
                });
            }
        }
        let mut captures: Vec<crate::eval_effect::FailCapture> = captures
            .into_iter()
            .map(|kind| crate::eval_effect::FailCapture {
                conjunction: Vec::new(),
                ranged: crate::range_modes::RangeModes::default(),
                kind,
            })
            .collect();
        let mut formatted_paths = effects.plain_slot_string_format_paths.clone();
        let mut formatted_meta = value
            .map(AbstractValue::plain_slot_string_format_meta)
            .unwrap_or_default();
        for (path, meta) in &effects.local_output_meta {
            if meta.plain_slot_string_format {
                formatted_meta.entry(path.clone()).or_default().merge(meta);
            }
        }
        for (path, meta) in &formatted_meta {
            if meta.plain_slot_string_format && !meta.partial_text {
                formatted_paths.insert(path.clone());
            }
        }
        // `printf` itself erases the operand's structural shape, but a token-
        // opening `%s` still renders that operand's characters. Later encoders
        // and YAML serializers clear or block the formatter channel.
        formatted_paths.retain(|path| {
            !effects.encoded_paths.contains(path) && !effects.yaml_serialized_paths.contains(path)
        });
        for path in formatted_paths {
            let mut shared = BTreeSet::new();
            if formatted_meta
                .get(&path)
                .is_none_or(|meta| meta.predicates.is_empty())
                && (effects.defaults.contains(&path) || effects.local_default_paths.contains(&path))
            {
                shared.insert(Predicate::truthy_path(path.clone()));
            }
            let branches = formatted_meta
                .get(&path)
                .filter(|meta| !meta.predicates.is_empty())
                .map_or_else(
                    || vec![shared.clone()],
                    |meta| {
                        meta.predicates
                            .iter()
                            .map(|branch| {
                                let mut conjunction = shared.clone();
                                conjunction.extend(branch.iter().cloned());
                                conjunction
                            })
                            .collect()
                    },
                );
            for conjunction in branches {
                for kind in [
                    crate::eval_effect::CaptureKind::PrintfStringOperand { path: path.clone() },
                    crate::eval_effect::CaptureKind::AbsenceAborts { path: path.clone() },
                ] {
                    captures.push(crate::eval_effect::FailCapture {
                        conjunction: conjunction.iter().cloned().collect(),
                        ranged: crate::range_modes::RangeModes::default(),
                        kind,
                    });
                }
            }
        }
        if !captures.is_empty() {
            self.record_yaml_text_fails(&captures);
        }
    }

    fn absorb_helper_summary_type_hints(&mut self, summary: &super::summary::FragmentSummary) {
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
            if !path.trim().is_empty() {
                self.guarded_type_hints
                    .entry(path.clone())
                    .or_default()
                    .extend(hints.iter().cloned());
            }
        }
        for (path, hints) in &summary.fallback_type_hints {
            if path.trim().is_empty() {
                continue;
            }
            let sink = if self.hint_scope_is_unconditional(path) {
                &mut self.fallback_type_hints
            } else {
                // Branch-scoped fallback hints remain intent, not a consumer contract.
                &mut self.guarded_fallback_type_hints
            };
            sink.entry(path.clone())
                .or_default()
                .extend(hints.iter().cloned());
        }
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
    ) -> Option<(Guarded<AbstractFragment>, Option<usize>)> {
        let (name, arg) = splice_target_helper_call(exprs)?;
        if !self.db.has_helper(name) || self.helper_seen.contains(name) {
            return None;
        }
        let name = name.to_string();
        let current_dot = self.current_value_dot();
        let mut seen = self.helper_seen.clone();
        let env = self.hole_eval_env(current_dot.as_ref());
        let call = self.db.summarize_bound_helper_call(
            &name,
            arg,
            Some(&self.root_bindings),
            current_dot.as_ref(),
            &env,
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
        if self.in_value_slot {
            self.record_plain_slot_text(summary.value.as_ref(), &Effects::default());
        }
        // An indent-only structural splice renders the body as document
        // content, so its plain slots belong to this document. A value slot
        // instead renders the body as the caller's scalar token.
        if !self.in_value_slot {
            self.record_yaml_text_fails(&summary.text_fails);
        }
        self.absorb_member_host_conversions(&summary.member_host_conversions);
        self.absorb_helper_summary_type_hints(summary);
        self.shape_erased_paths
            .extend(summary.shape_erased_paths.iter().cloned());
        self.yaml_serialized_paths
            .extend(summary.yaml_serialized_paths.iter().cloned());
        let mut self_guarded_contracts = std::collections::BTreeSet::new();
        for path in &summary.string_contract_paths {
            let guarded_by_source = self.active_predicates.iter().any(|predicate| {
                matches!(
                    predicate,
                    Predicate::Guard(
                        Guard::Truthy { path: guarded }
                            | Guard::Range { path: guarded }
                            | Guard::With { path: guarded }
                    ) if guarded == path
                )
            });
            if guarded_by_source {
                self_guarded_contracts.insert(path.clone());
            } else {
                self.string_contract_paths.insert(path.clone());
            }
        }
        self.absorb_condition_string_captures(&self_guarded_contracts);
        self.range_modes.merge(&summary.range_modes);
        self.chart_defaults_observed
            .extend(summary.chart_defaults.iter().cloned());
        self.apply_root_set_mutations(
            &summary.root_set_mutations,
            &summary.root_set_predicates,
            &summary.root_set_value_dispatches,
        );
        self.values_default_sources_observed
            .extend(summary.values_default_sources.iter().cloned());
        self.values_root_overlay_prefixes_observed
            .extend(summary.values_root_overlay_prefixes.iter().cloned());
        // A wrapper snapshot stays verbatim because caller reads run afterward.
        self.pre_rewrite_strict_paths
            .extend(summary.pre_rewrite_strict_paths.iter().cloned());
        self.values_root_helper_includes_observed
            .extend(summary.values_root_helper_includes.iter().cloned());
        let mut chart_defaults = summary.chart_defaults.clone();
        self.locals.append_chart_value_defaults(&mut chart_defaults);
        Some((
            splice_summary(summary, self.current_site.as_ref(), self.in_value_slot),
            summary.root_render_indent,
        ))
    }

    /// Evaluate a hole rendered inside a partial scalar: guarded arms of
    /// string parts.
    #[expect(
        clippy::too_many_lines,
        reason = "hole evaluation must restore site state across every early return and keep lowering inputs synchronized"
    )]
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
        self.run_templated_text_paths
            .extend(hole.effects.templated_text_identity_paths.iter().cloned());
        let (value, extra_paths) =
            prepare_hole_value(hole.value, &hole.effects, kind != ValueKind::Fragment);
        let defaulted = hole.effects.default_paths_with_local();
        // Direct helper flows collapsed by transfer functions (printf over
        // include) keep their per-path branch meta: the summary's rendered
        // rows merge with the locals' binding-time meta for lowering.
        let mut hole_meta = hole.effects.local_output_meta.clone();
        merge_rendered_row_meta(&mut hole_meta, &hole.effects.helper_rendered);
        for (path, keys) in &hole.effects.omitted_map_keys {
            let meta = hole_meta.entry(path.clone()).or_default();
            for key in keys {
                meta.omitted_keys.insert(key.clone(), Vec::new());
            }
        }
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
        let mut arms = (self.scalar_output_projection || self.helper_scope)
            .then_some(hole.scalar_dispatch.as_ref())
            .flatten()
            .and_then(|dispatch| lower_scalar_dispatch_arms(dispatch, kind, &scope))
            .unwrap_or_else(|| match &value {
                Some(value) => lower_value_scalar_arms(value, kind, &scope),
                None => Vec::new(),
            });
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
        if !defaulted.is_empty() && !arms.is_empty() {
            let covered = Predicate::Or(
                arms.iter()
                    .map(|(condition, _)| condition.clone())
                    .collect(),
            )
            .normalize_boolean();
            if !covered.contains_approximation() {
                let fallback = covered.negated().normalize_boolean();
                if fallback != Predicate::False {
                    // An unattributed fallback still renders. Its empty part
                    // arm keeps neighboring scalar segments live when the
                    // selected value comes from chart context or a literal.
                    arms.push((fallback, Vec::new()));
                }
            }
        }
        for (_, parts) in &mut arms {
            stamp_part_sites(parts, self.current_site.as_ref());
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
        // A scalar the YAML grammar itself quoted carries its quotes outside
        // the templated parts, so neither the entire-hole shape nor the
        // completed-token scanner can see them: the slot is not a plain token
        // (jenkins' `jenkinsUrl: "{{ tpl .Values.agent.jenkinsUrl . }}"`).
        let quoted = self.text(parts.span).trim_start().starts_with(['"', '\'']);
        let previous_slot = self.in_value_slot;
        self.in_value_slot = previous_slot && !quoted;
        let out = self.eval_scalar_parts_inner(parts);
        self.in_value_slot = previous_slot;
        out
    }

    fn eval_scalar_parts_inner(&mut self, parts: &ScalarParts) -> Guarded<AbstractFragment> {
        let segments = self.scalar_segments(parts);
        if let Some(span) = entire_hole_span(&segments) {
            return self.eval_entire_hole(span);
        }
        // A `tpl` operand renders as DERIVED text, so its holes contribute
        // taint rather than a splice; the completed-token pass needs to know
        // which of those taints still carry the raw value's own characters,
        // and this run's holes are exactly the ones about to be evaluated.
        self.run_templated_text_paths.clear();
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
    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
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
            double_quoted_templated: std::collections::BTreeSet<String>,
            single_quoted_templated: std::collections::BTreeSet<String>,
            plain_templated: std::collections::BTreeSet<String>,
        }

        fn arm_claims(
            parts: &[StringPart],
            templated: &std::collections::BTreeSet<String>,
            value_slot: bool,
        ) -> ArmClaims {
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
                            && !splice.meta.range_key
                            && !splice.values_path.is_empty()
                            && !templated.contains(&splice.values_path);
                        // A `tpl` render is the raw value's own text whenever
                        // that value carries no template action, so an
                        // UNQUOTED position still binds the plain token's
                        // language even though `tpl` bound a string contract
                        // (cluster-autoscaler's
                        // `- --cluster-name={{ tpl .Values.magnumClusterName . }}`,
                        // where a `: ` turns the command item into a mapping).
                        if state == QuoteContext::None
                            && value_slot
                            && templated.contains(&splice.values_path)
                            && !splice.meta.encoded
                            && (!splice.meta.shape_erased || splice.meta.stringified)
                            && !splice.meta.yaml_serialized
                            && !splice.meta.json_serialized
                            && splice.meta.split_segment.is_none()
                        {
                            claims.plain_templated.insert(splice.values_path.clone());
                        }
                        if templated.contains(&splice.values_path)
                            && !splice.meta.encoded
                            && (!splice.meta.shape_erased || splice.meta.stringified)
                            && !splice.meta.yaml_serialized
                            && !splice.meta.json_serialized
                            && splice.meta.split_segment.is_none()
                        {
                            match state {
                                QuoteContext::Double => {
                                    claims
                                        .double_quoted_templated
                                        .insert(splice.values_path.clone());
                                }
                                QuoteContext::Single => {
                                    claims
                                        .single_quoted_templated
                                        .insert(splice.values_path.clone());
                                }
                                QuoteContext::None => {}
                            }
                        }
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

        let templated = self.run_templated_text_paths.clone();
        let value_slot = self.in_value_slot;
        let mut per_arm = arms
            .iter()
            .map(|(_, parts)| arm_claims(parts, &templated, value_slot));
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
            agreed
                .double_quoted_templated
                .retain(|path| arm.double_quoted_templated.contains(path));
            agreed
                .single_quoted_templated
                .retain(|path| arm.single_quoted_templated.contains(path));
            agreed
                .plain_templated
                .retain(|path| arm.plain_templated.contains(path));
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
        for (paths, style, templated) in [
            (
                agreed.double_quoted,
                helm_schema_core::QuotedScalarStyle::Double,
                false,
            ),
            (
                agreed.single_quoted,
                helm_schema_core::QuotedScalarStyle::Single,
                false,
            ),
            (
                agreed.double_quoted_templated,
                helm_schema_core::QuotedScalarStyle::Double,
                true,
            ),
            (
                agreed.single_quoted_templated,
                helm_schema_core::QuotedScalarStyle::Single,
                true,
            ),
        ] {
            for path in paths {
                captures.push(crate::eval_effect::FailCapture {
                    conjunction: Vec::new(),
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::QuotedSerialization {
                        path,
                        style,
                        templated,
                    },
                });
            }
        }
        // The slot language describes this source's own TEXT, so a helper
        // body defers it to the sink its caller splices the body into. The
        // claims above are about the value's SHAPE at the position and bind
        // wherever the position renders.
        let mut text_captures = Vec::new();
        for path in agreed.plain_templated {
            text_captures.push(crate::eval_effect::FailCapture {
                conjunction: Vec::new(),
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::PlainSlotText {
                    path,
                    // Literal text shares the token, so only its interior
                    // characters can end it; a leading indicator cannot.
                    token_initial: false,
                    templated: true,
                },
            });
        }
        if !captures.is_empty() {
            self.absorb_helper_fails(&captures);
        }
        if !text_captures.is_empty() {
            self.record_yaml_text_fails(&text_captures);
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
                    } else if facts.region_end > block.body.end {
                        cursor = block.body.end;
                        break;
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

    /// A shallow bare output hanging under a block-scalar entry or item
    /// (`key: |` followed by a column-0 `{{- include … }}`): the `|`/`>`
    /// header says the rendered text continues the block whenever it is
    /// deeper than the entry, so the hole gets the same treatment as holes
    /// inside the block body — fragment renders keep their semantic rows
    /// without minting structure, everything else contributes partial
    /// scalar text.
    pub(super) fn eval_block_adopted_output(&mut self, span: Span) -> Guarded<AbstractFragment> {
        if parse_expr_text(self.text(span))
            .iter()
            .any(TemplateExpr::renders_yaml_fragment)
        {
            self.eval_suppressed_fragment_hole(span);
            return Guarded::empty();
        }
        scalar_arms_to_fragment(self.eval_hole_parts(span), true)
    }

    /// A control region whose rendered body remains below a block-scalar
    /// header contributes guarded text, not structure beside that scalar.
    pub(super) fn eval_block_adopted_control(&mut self, span: Span) -> Guarded<AbstractFragment> {
        scalar_arms_to_fragment(self.eval_inline_region(span), true)
    }

    pub(super) fn eval_structural_control_output(
        &mut self,
        span: Span,
    ) -> Guarded<AbstractFragment> {
        scalar_arms_to_fragment(self.eval_inline_region(span), false)
    }

    /// A fragment-rendering hole inside a render-suppressed blob: rendered
    /// helper rows become pathless serialized reads, and direct values are
    /// serialized text rather than structural members of the enclosing
    /// document.
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
        self.absorb_hole_effects(&hole.effects, RenderedDemotion::Serialized);
        // The block's text is opaque unless its key names a YAML document,
        // and only a splice that reaches it through nothing but indent
        // shaping still renders the called body's own characters. Both hold
        // for jenkins' `jcasc-default-config.yaml: |-`, whose embedded
        // document stops parsing when a JCasC key breaks its plain token.
        if self.block_text_is_yaml && splice_target_helper_call(&exprs).is_some() {
            self.record_yaml_text_fails(&hole.effects.helper_text_fails);
        }
        self.push_effects_reads(&hole, ValueKind::Serialized);
        self.restore_site(previous_site);
    }
}
