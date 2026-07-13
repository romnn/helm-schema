//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use helm_schema_ast::{TemplateExpr, parse_action_expressions, parse_expr_text};
use helm_schema_syntax::{BlockScalar, ScalarPart, ScalarParts, Span, parse_go_template};

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::{
    BoundValueContext, parse_get_binding_from_exprs, parse_literal_list_range_expr,
};
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::expr_eval::literal_helper_call_callee;
use crate::fragment_assignment::parse_helper_assignment_from_exprs;
use crate::fragment_expr_eval::{FragmentEvalContext, document_result_from_expr};
use crate::helper_meta::merge_rendered_row_meta;
use crate::node_eval::{NodeAction, control_header, else_if_pairs, node_action};
use crate::{Guard, ValueKind};
use helm_schema_ast::children_with_field;
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, Guarded, PathCondition, StringPart, TaintPart,
    and_conditions, stamp_fragment_sites, stamp_part_sites,
};
use super::eval::Interpreter;
use super::lower::{
    LowerScope, MAX_SCALAR_ARM_FANOUT, MAX_SCALAR_ARMS, lower_value, lower_value_scalar_arms,
};
use super::summary::splice_summary;

pub(super) struct HoleEval {
    pub(super) value: Option<AbstractValue>,
    pub(super) effects: Effects,
}

/// How a no-render site demotes a called helper's rendered rows.
enum RenderedDemotion {
    /// Rendered rows stay tree evidence (ordinary output holes).
    None,
    /// Document-lane pathless claims keeping their kinds (document-scope
    /// assignments capture the rows as local evidence).
    Document,
    /// Dependency-lane scalar claims (helper-body captures and
    /// render-suppressed blobs: the caller sees summary facts, and
    /// dependency rows are scalar by construction).
    Dependency,
}

/// The subject expressions of every `required(message, subject)` call in
/// an expression, including the piped form (`subject | required "msg"`,
/// where the piped value arrives as the trailing argument).
fn required_call_subjects(expr: &TemplateExpr) -> Vec<&TemplateExpr> {
    let mut subjects = Vec::new();
    collect_required_subjects(expr, &mut subjects);
    subjects
}

fn collect_required_subjects<'e>(expr: &'e TemplateExpr, out: &mut Vec<&'e TemplateExpr>) {
    match expr {
        TemplateExpr::Call { function, args } => {
            if function == "required" && args.len() == 2 {
                out.push(&args[1]);
            }
            for arg in args {
                collect_required_subjects(arg, out);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            for (index, stage) in stages.iter().enumerate() {
                if let TemplateExpr::Call { function, args } = stage
                    && function == "required"
                    && args.len() == 1
                    && index > 0
                {
                    out.push(&stages[index - 1]);
                }
                collect_required_subjects(stage, out);
            }
        }
        TemplateExpr::Parenthesized(inner) => collect_required_subjects(inner, out),
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            collect_required_subjects(value, out)
        }
        _ => {}
    }
}

/// Whether an expression invokes `fail` anywhere: evaluating it terminates
/// template rendering unconditionally.
fn expr_contains_fail_call(expr: &TemplateExpr) -> bool {
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

/// The values path whose TYPE an expression describes: `typeOf <selector>`
/// or `kindOf <selector>` over a single resolvable values path.
fn type_descriptor_source(expr: &TemplateExpr, interpreter: &Interpreter<'_>) -> Option<String> {
    let TemplateExpr::Call { function, args } = expr.deparen() else {
        return None;
    };
    if !matches!(function.as_str(), "typeOf" | "kindOf") || args.len() != 1 {
        return None;
    }
    let subject = args[0].deparen();
    if !matches!(
        subject,
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
    ) {
        return None;
    }
    let paths = interpreter.value_path_context().paths_for_expr(subject);
    (paths.len() == 1).then(|| paths.into_iter().next().unwrap_or_default())
}

/// One layout segment of a scalar run: literal text, a template hole, or a
/// whole inline control region (grouping the region's holes and texts).
enum Segment {
    Text(String),
    Hole(Span),
    Region(Span),
}

impl Interpreter<'_> {
    /// Evaluate one condition expression through the shared value lattice
    /// with bound-helper resolution (guard-read derivation for conditions
    /// over helper calls).
    pub(super) fn eval_hole_exprs_for_condition(&mut self, expr: &TemplateExpr) -> HoleEval {
        self.eval_hole_exprs(std::slice::from_ref(expr))
    }

    /// Record every `required(message, subject)` guardrail in the
    /// expressions: rendering fails under the ambient predicates whenever a
    /// subject resolving to exactly one values path is Helm-empty. Member
    /// bindings resolve here (the value-path context sees them), so ranged
    /// subjects attach per-member requirements.
    pub(super) fn record_required_subjects(&mut self, exprs: &[TemplateExpr]) {
        let mut subject_paths = Vec::new();
        {
            let context = self.value_path_context();
            for expr in exprs {
                for subject in required_call_subjects(expr) {
                    let paths = context.paths_for_expr(subject);
                    if paths.len() == 1 {
                        subject_paths.extend(paths);
                    }
                }
            }
        }
        for path in subject_paths {
            self.record_required_condition(&path);
        }
    }

    /// Evaluate the expressions of one output hole through the shared value
    /// lattice, resolving bound helper calls via the memoized summaries.
    fn eval_hole_exprs(&mut self, exprs: &[TemplateExpr]) -> HoleEval {
        let current_dot = self.current_value_dot();
        let mut env = EvalEnv::from_helper_context(Some(&self.root_bindings), current_dot.as_ref())
            .without_helper_call_args();
        // Locals (`$x`) and root bindings (`.x`) are distinct namespaces:
        // roots stay in `root_fields` so a helper-arg key never shadows a
        // same-named body local.
        env.locals = self.locals.fragment_values.clone();
        env.local_default_paths = self.locals.default_paths.clone();
        env.local_output_meta = self.locals.output_meta.clone();
        env.bound_values =
            BoundValueContext::new(&self.locals.range_domains, &self.locals.get_bindings);
        let context = FragmentEvalContext::new(self.db);
        let mut seen = self.helper_seen.clone();
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
    /// list: chart-level default mutations (source order), declared type
    /// hints, bound-value reads, and helper-internal read facts. Rendered
    /// helper rows become reads only in no-render contexts, per the site's
    /// [`RenderedDemotion`] flavor.
    /// Whether a hint on `path` binds unconditionally. Guards about `path`
    /// itself (self-guards, `typeIs` type switches) partition its own
    /// domain; `range`/`with` headers and `default` fallbacks bind values
    /// without expressing configuration branches. Only a document-level
    /// boolean-style guard on some OTHER path scopes the hint to that
    /// branch — helper-internal branches are calling-convention dispatch,
    /// not chart configuration, so they never scope hints.
    pub(super) fn hint_scope_is_unconditional(&self, path: &str) -> bool {
        if self.helper_scope {
            return true;
        }
        fn guard_gates(guard: &Guard, path: &str) -> bool {
            let foreign = |guard_path: &str| {
                guard_path != path
                    && !helm_schema_core::values_path_is_descendant(guard_path, path)
                    && !helm_schema_core::values_path_is_descendant(path, guard_path)
            };
            match guard {
                Guard::Range { .. } | Guard::With { .. } | Guard::Default { .. } => false,
                Guard::Truthy { path: guard_path }
                | Guard::Not { path: guard_path }
                | Guard::Absent { path: guard_path }
                | Guard::Eq {
                    path: guard_path, ..
                }
                | Guard::NotEq {
                    path: guard_path, ..
                } => !guard_path.trim().is_empty() && foreign(guard_path),
                // A type test PARTITIONS its subject: hints observed under
                // it hold only for the tested types, even on the hinted
                // path itself (a self-truthy guard, by contrast, only
                // states nullability).
                Guard::TypeIs {
                    path: guard_path, ..
                }
                | Guard::NotTypeIs {
                    path: guard_path, ..
                } => !guard_path.trim().is_empty(),
                Guard::Or { paths } => paths
                    .iter()
                    .any(|guard_path| !guard_path.trim().is_empty() && foreign(guard_path)),
                Guard::AnyOf { alternatives } => alternatives
                    .iter()
                    .flatten()
                    .any(|guard| guard_gates(guard, path)),
            }
        }
        fn predicate_gates(predicate: &Predicate, path: &str) -> bool {
            match predicate {
                Predicate::True | Predicate::False => false,
                Predicate::Guard(guard) => guard_gates(guard, path),
                Predicate::Not(inner) => predicate_gates(inner, path),
                Predicate::And(predicates) | Predicate::Or(predicates) => {
                    predicates.iter().any(|inner| predicate_gates(inner, path))
                }
            }
        }
        self.active_predicates
            .iter()
            .all(|predicate| !predicate_gates(predicate, path))
    }

    fn absorb_hole_effects(&mut self, effects: &Effects, demotion: RenderedDemotion) {
        self.chart_defaults_observed
            .extend(effects.chart_default_paths.iter().cloned());
        let mut chart_defaults = effects.chart_default_paths.clone();
        self.locals.append_chart_value_defaults(&mut chart_defaults);

        // Type hints surface from every hole, including assignment
        // right-hand sides. A hint observed under branch predicates about
        // OTHER paths holds only where those branches render: it may type
        // conditional overlays but never the unconditional base. Predicates
        // about the hinted path itself (self-guards, `typeIs` type
        // switches) partition its own domain instead, so those hints stay
        // base evidence.
        for (path, hints) in &effects.type_hints {
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
        for (path, hints) in &effects.guarded_type_hints {
            if path.trim().is_empty() {
                continue;
            }
            self.guarded_type_hints
                .entry(path.clone())
                .or_default()
                .extend(hints.iter().cloned());
        }
        self.parsed_yaml_input_paths
            .extend(effects.parsed_yaml_input_paths.iter().cloned());
        self.yaml_serialized_paths
            .extend(effects.yaml_serialized_paths.iter().cloned());
        self.shape_erased_paths
            .extend(effects.shape_erased_paths.iter().cloned());
        self.string_contract_paths
            .extend(effects.string_contract_paths.iter().cloned());
        // Under ambient predicates the row lanes only hint (and hints
        // about a path under its OWN guard stay row-anchored); the
        // truthy⇒string capture carries the enforceable conditional arm
        // through the fail machinery (ambient guards join at absorption).
        // Predicate-free sites stay row-only: the unconditional row typing
        // already states the requirement. Only DIRECT consumer subjects
        // qualify — a called helper's contract flags lost their
        // body-internal guards (its own fail lane carries the captures).
        if !self.active_predicates.is_empty() {
            self.absorb_condition_string_captures(&effects.direct_string_consumer_paths.clone());
        }

        let bound_reads: Vec<String> = effects.bound_output_paths.iter().cloned().collect();
        for path in bound_reads {
            self.push_read(&path, &[]);
        }
        // Guard-path reads that are strict ancestors of a predicate path the
        // helper explicitly severed (index-call narrowing) are dropped, the
        // same way the summary lane always skipped them. Narrowings observed
        // in this source accumulate so a helper summary can apply them to
        // its own condition reads.
        for meta in effects.local_output_meta.values() {
            self.suppress_predicate_paths
                .extend(meta.suppress_predicate_paths.iter().cloned());
        }
        self.suppress_predicate_paths
            .extend(effects.helper_suppressed_paths.iter().cloned());
        let suppressed: std::collections::BTreeSet<&String> = effects
            .helper_rendered
            .iter()
            .flat_map(|row| row.meta.suppress_predicate_paths.iter())
            .chain(effects.helper_suppressed_paths.iter())
            .collect();
        let claims = helper_claim_paths(effects);
        self.absorb_helper_reads_with_suppression(&effects.helper_reads, &suppressed, &claims);
        self.absorb_helper_fails(&effects.helper_fails);
        match demotion {
            RenderedDemotion::None => {}
            RenderedDemotion::Document => {
                for row in &effects.helper_rendered {
                    // Encoded renders don't expose the value's shape; other
                    // demoted rows keep their summary kind (structured helper
                    // rows captured by assignments stay fragment evidence).
                    let kind = if row.encoded {
                        ValueKind::Scalar
                    } else {
                        row.kind
                    };
                    self.push_meta_reads(&row.path, kind, &row.meta, &claims, false);
                }
            }
            RenderedDemotion::Dependency => {
                for row in &effects.helper_rendered {
                    self.push_meta_reads(&row.path, ValueKind::Scalar, &row.meta, &claims, true);
                }
            }
        }
    }

    /// Pathless reads for every values path the hole's effects attribute
    /// (used where the current pipeline suppresses rendered placement:
    /// assignment right-hand sides and render-suppressed fragment holes).
    /// Ancestor paths with a more specific path in the same hole are dropped
    /// (the most-specific-path rule for scalar sites), and paths already
    /// covered by rendered helper rows read through those rows instead.
    fn push_effects_reads(&mut self, hole: &HoleEval, kind: ValueKind) {
        let row_sources: std::collections::BTreeSet<&String> = hole
            .effects
            .helper_rendered
            .iter()
            .map(|row| &row.path)
            .collect();
        let defaulted = hole.effects.default_paths_with_local();
        let all = hole.effects.output_value_paths();
        for path in &all {
            if helm_schema_core::values_path_has_descendant(path, &all)
                || row_sources.contains(path)
            {
                continue;
            }
            let mut extra = Vec::new();
            if defaulted.contains(path) {
                extra.push(Guard::Default { path: path.clone() });
            }
            let (resource, provenance) = match &self.current_site {
                Some(site) => (
                    site.resource.clone(),
                    site.provenance.iter().cloned().collect(),
                ),
                None => (None, Vec::new()),
            };
            self.push_read_row(path, kind, &extra, resource, provenance, false);
        }
    }

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
        let scope = LowerScope {
            defaulted_paths: &defaulted,
            encoded_paths: &hole.effects.encoded_paths,
            shape_erased_paths: &hole.effects.shape_erased_paths,
            string_contract_paths: &hole.effects.string_contract_paths,
            chart_value_defaults: &self.locals.chart_value_defaults,
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
        let mut seen = self.helper_seen.clone();
        let summary = self.db.summarize_bound_helper_call(
            &name,
            arg,
            Some(&self.root_bindings),
            current_dot.as_ref(),
            &self.locals.fragment_values,
            FragmentEvalContext::new(self.db),
            &mut seen,
        );
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
        self.shape_erased_paths
            .extend(summary.shape_erased_paths.iter().cloned());
        self.string_contract_paths
            .extend(summary.string_contract_paths.iter().cloned());
        self.chart_defaults_observed
            .extend(summary.chart_defaults.iter().cloned());
        let mut chart_defaults = summary.chart_defaults.clone();
        self.locals.append_chart_value_defaults(&mut chart_defaults);
        Some(splice_summary(&summary, &self.current_site))
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
        let scope = LowerScope {
            defaulted_paths: &defaulted,
            encoded_paths: &hole.effects.encoded_paths,
            shape_erased_paths: &hole.effects.shape_erased_paths,
            string_contract_paths: &hole.effects.string_contract_paths,
            chart_value_defaults: &self.locals.chart_value_defaults,
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
                Some(_) => {}
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
        let hole = self.eval_hole_exprs(&exprs);
        self.absorb_hole_effects(&hole.effects, RenderedDemotion::Document);
        self.push_effects_reads(&hole, ValueKind::Fragment);
        self.restore_site(previous_site);
    }

    /// Evaluate an inline `{{ if }}…{{ end }}` or `{{ range }}…{{ end }}`
    /// region inside a scalar by re-parsing the region text with the
    /// Go-template grammar and turning its branches into guarded scalar
    /// arms. Other inline regions (`with`) and nested regions degrade to
    /// conservative taint. The whole region evaluates under the region's
    /// site facts (its holes share the region's line).
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

    fn eval_inline_region_arms(&mut self, span: Span) -> Vec<(PathCondition, Vec<StringPart>)> {
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
        let entry_approximate = self.approximate_condition_paths.len();
        let mut prior_approximate_paths: Vec<String> = Vec::new();
        let mut prior: Vec<PathCondition> = Vec::new();
        let mut arms = Vec::new();
        for (header, children) in arm_specs {
            self.active_predicates.truncate(entry_predicates);
            self.approximate_condition_paths.truncate(entry_approximate);
            // An arm under the negation of an approximately-lowered prior
            // is approximate on the same paths.
            self.approximate_condition_paths
                .extend(prior_approximate_paths.iter().cloned());
            let mut arm_condition = Predicate::True;
            for predicate in &prior {
                let negated = predicate.negated();
                self.push_predicate(negated.clone());
                arm_condition = and_conditions(arm_condition, negated);
            }
            let arm_entry_approximate = self.approximate_condition_paths.len();
            if let Some(own) = self.activate_inline_if(header.as_ref()) {
                arm_condition = and_conditions(arm_condition, own.clone());
                prior.push(own);
            }
            prior_approximate_paths.extend(
                self.approximate_condition_paths[arm_entry_approximate..]
                    .iter()
                    .cloned(),
            );
            for (sub_condition, parts) in self.inline_body_arms(&children, text) {
                arms.push((and_conditions(arm_condition.clone(), sub_condition), parts));
            }
        }
        self.active_predicates.truncate(entry_predicates);
        self.approximate_condition_paths.truncate(entry_approximate);
        if arms.len() > MAX_SCALAR_ARM_FANOUT {
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
        let entry_ranged = self.active_direct_ranged_paths.len();
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
        if let Some(path) = &direct_path {
            self.active_direct_ranged_paths.push(path.clone());
        }
        let dot = direct_path
            .map(|path| AbstractValue::ValuesPath(helm_schema_core::append_value_path(&path, "*")));
        self.dot_stack.push(dot);
        let mut arms = Vec::new();
        for (sub_condition, parts) in
            self.inline_body_arms(&children_with_field(node, "body"), text)
        {
            arms.push((and_conditions(condition.clone(), sub_condition), parts));
        }
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
        let (predicate, faithful) = {
            let context = self.value_path_context();
            (
                context.condition_predicate_expr(header.expr()),
                context.condition_lowering_is_faithful(header.expr()),
            )
        };
        if !faithful {
            let paths = self
                .value_path_context()
                .resolved_values_paths_from_expr(header.expr());
            if paths.is_empty() {
                self.approximate_condition_paths.push(String::new());
            }
            self.approximate_condition_paths.extend(paths);
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
                // A `fail` output terminates rendering: no valid values
                // document may satisfy the guards active here, and the
                // action renders nothing.
                if exprs.iter().any(expr_contains_fail_call) {
                    self.record_fail_condition();
                    return Vec::new();
                }
                self.record_required_subjects(&exprs);
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
                let scope = LowerScope {
                    defaulted_paths: &defaulted,
                    encoded_paths: &hole.effects.encoded_paths,
                    shape_erased_paths: &hole.effects.shape_erased_paths,
                    string_contract_paths: &hole.effects.string_contract_paths,
                    chart_value_defaults: &self.locals.chart_value_defaults,
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
            NodeAction::If(_) | NodeAction::With(_) | NodeAction::Range(_) => {
                // Nested inline control: keep the influence, drop the
                // structure (bounded conservative fallback).
                let content = node.utf8_text(text.as_bytes()).unwrap_or("");
                let taint = self.resolved_paths_of_action_text(content);
                if taint.is_empty() {
                    Vec::new()
                } else {
                    vec![(
                        Predicate::True,
                        vec![StringPart::Taint(TaintPart::new(taint))],
                    )]
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
        vec![(
            Predicate::True,
            vec![StringPart::Taint(TaintPart::new(taint))],
        )]
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
    pub(super) fn eval_assignment_span(&mut self, span: Span) {
        let exprs = parse_expr_text(self.text(span));
        if exprs.is_empty() {
            return;
        }
        let previous_site = self.enter_hole_site(span);
        self.eval_assignment_exprs(&exprs);
        self.restore_site(previous_site);
    }

    /// Structural `set` mutations on local dict bindings (`set $ctx "k" v`,
    /// bare or assigned to `$_`) mutate the target local instead of binding
    /// output. Helper bodies rely on this for config-normalization chains;
    /// only the chart-default effects surface (the summary lane never
    /// claimed set-call operand reads).
    pub(super) fn apply_helper_scope_set_mutations(&mut self, exprs: &[TemplateExpr]) -> bool {
        if !self.helper_scope {
            return false;
        }
        let current_dot = self.current_dot_fragment();
        let mut seen = self.helper_seen.clone();
        if !crate::fragment_assignment::apply_local_set_mutations_from_exprs(
            exprs,
            &mut self.locals.fragment_values,
            current_dot.as_ref(),
            FragmentEvalContext::new(self.db),
            &mut seen,
        ) {
            return false;
        }
        let effects = crate::expr_eval::eval_helper_exprs_direct_effects(
            exprs,
            &self.root_bindings,
            self.current_value_dot().as_ref(),
        );
        self.chart_defaults_observed
            .extend(effects.chart_default_paths.iter().cloned());
        let mut chart_defaults = effects.chart_default_paths;
        self.locals.append_chart_value_defaults(&mut chart_defaults);
        true
    }

    pub(super) fn eval_assignment_exprs(&mut self, exprs: &[TemplateExpr]) {
        if self.apply_helper_scope_set_mutations(exprs) {
            return;
        }
        if let Some(assignment) = parse_helper_assignment_from_exprs(exprs) {
            let rhs = std::slice::from_ref(&assignment.rhs_expr);
            self.record_required_subjects(rhs);
            let output_effects = self.value_path_context().expression_output_effects(rhs);
            let hole = self.eval_hole_exprs(rhs);
            // The binding is the hole value without widened members (an
            // unknown call result is influence, not a values-backed
            // fragment).
            let fragment_value = hole.value.clone().and_then(AbstractValue::without_widened);
            // Helper bodies keep the prior binding when the right-hand side
            // resolves to nothing (the summary lane's rule): an unresolvable
            // re-assignment in one branch must not erase the other branches'
            // value at the join.
            if fragment_value.is_some() || !self.helper_scope {
                self.locals.bind_fragment_value(
                    assignment.kind,
                    assignment.variable.clone(),
                    fragment_value.clone(),
                );
            }
            // `$tp := typeOf .Values.x` binds a TYPE DESCRIPTOR of the path:
            // later `eq $tp "string"` comparisons are type tests, never value
            // equalities, so remember the described path. Recorded after the
            // value binding, whose displacement clears every other domain.
            if let Some(source) = type_descriptor_source(&assignment.rhs_expr, self) {
                self.locals
                    .typeof_sources
                    .insert(assignment.variable.clone(), source);
            }
            let mut output_meta = output_effects.local_output_meta.clone();
            merge_rendered_row_meta(&mut output_meta, &hole.effects.helper_rendered);
            // A shape-erasing RHS (`$tag := … | toString`) rides the binding:
            // wherever the local renders, the splice exposes no input shape.
            for path in &hole.effects.shape_erased_paths {
                output_meta.entry(path.clone()).or_default().shape_erased = true;
            }
            // Likewise a derived-text RHS (`$port := include … .`): a later
            // consuming transform on the local operates on rendered text and
            // claims nothing about the underlying paths.
            for path in &hole.effects.derived_text_paths {
                output_meta.entry(path.clone()).or_default().derived_text = true;
            }
            // A string-contracting RHS (`$name := .Values.x | trunc 63`)
            // also rides the binding: wherever the local renders, that row
            // requires a string input.
            for path in &hole.effects.string_contract_paths {
                output_meta.entry(path.clone()).or_default().string_contract = true;
            }
            // A helper-body `=` re-assignment under branch predicates keeps
            // those predicates on each flowing path's meta: the write-through
            // survives the branch join in the locals, so the conditions must
            // ride the meta to the render site (the summary lane's rule). A
            // truthiness condition about a *different* flowing path describes
            // a sibling's branch and stays off this path's meta.
            if self.helper_scope
                && assignment.kind == crate::fragment_assignment::AssignmentKind::Assignment
                && !self.active_predicates.is_empty()
            {
                if let Some(binding) = &fragment_value {
                    for path in binding.fragment_rendered_paths() {
                        output_meta.entry(path).or_default();
                    }
                }
                let flowing: std::collections::BTreeSet<String> =
                    output_meta.keys().cloned().collect();
                for (path, meta) in &mut output_meta {
                    let site: std::collections::BTreeSet<Predicate> = self
                        .active_predicates
                        .iter()
                        .filter(|predicate| {
                            predicate_applies_to_flowing_path(predicate, path, &flowing)
                        })
                        .cloned()
                        .collect();
                    meta.conjoin_branches(&site);
                }
            }
            if self.helper_scope {
                // Keep the (possibly empty) default and meta entries: the
                // branch join unions per-variable facts only for variables
                // every outcome still tracks, and a pre-branch binding
                // without facts must not erase a branch's recorded ones.
                self.locals
                    .default_paths
                    .insert(assignment.variable.clone(), output_effects.defaults.clone());
                self.locals
                    .output_meta
                    .insert(assignment.variable.clone(), output_meta);
            } else {
                self.locals
                    .set_default_paths(&assignment.variable, output_effects.defaults.clone());
                self.locals
                    .set_output_meta(assignment.variable.clone(), output_meta);
            }
            let demotion = if self.helper_scope {
                RenderedDemotion::Dependency
            } else {
                RenderedDemotion::Document
            };
            self.absorb_hole_effects(&hole.effects, demotion);
            // Inside helper bodies, direct expression paths ride the binding
            // and surface where the local renders; the summary lane never
            // claimed them at the assignment itself.
            if !self.helper_scope {
                let kind = if rhs.iter().any(TemplateExpr::renders_yaml_fragment) {
                    ValueKind::Fragment
                } else {
                    ValueKind::Scalar
                };
                self.push_effects_reads(&hole, kind);
            }
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

/// The claim paths of the helper calls one hole resolved (read and rendered
/// sources), the sibling set for ambient-condition scoping.
fn helper_claim_paths(effects: &Effects) -> std::collections::BTreeSet<String> {
    let mut claims: std::collections::BTreeSet<String> = effects
        .helper_reads
        .iter()
        .map(|read| read.values_path.clone())
        .collect();
    claims.extend(effects.helper_rendered.iter().map(|row| row.path.clone()));
    claims
}

/// Whether an ambient predicate belongs on one flowing path's assignment
/// meta: truthiness conditions about a different flowing path of the same
/// assignment describe that sibling's branch (unrelated paths keep the
/// condition).
fn predicate_applies_to_flowing_path(
    predicate: &Predicate,
    path: &str,
    flowing: &std::collections::BTreeSet<String>,
) -> bool {
    let predicate_path = match predicate {
        Predicate::Guard(Guard::Truthy { path } | Guard::Not { path }) => path,
        Predicate::Not(inner) => {
            return predicate_applies_to_flowing_path(inner, path, flowing);
        }
        _ => return true,
    };
    predicate_path == path
        || !flowing.contains(predicate_path)
        || crate::helper_meta::values_paths_are_related(predicate_path, path)
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
