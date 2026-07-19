//! Absorption of hole-evaluation effects into interpreter state:
//! header-execution effects, required-call subjects, rendered-claim
//! demotion, root `set` mutations, and effect-derived reads.

//! Output-hole evaluation: expression holes evaluate through the existing
//! `AbstractValue` lattice (with bound-helper resolution) and lower into
//! fragment nodes; partial scalars combine per-segment arms with a bounded
//! cartesian product; inline `{{ if }}…{{ end }}` regions inside scalars
//! re-parse structurally and become guarded scalar arms.

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::bound_value_analysis::BoundValueContext;
use crate::eval_effect::Effects;
use crate::eval_env::EvalEnv;
use crate::fragment_expr_eval::{FragmentEvalContext, document_result_from_expr};
use crate::{Guard, ValueKind};
use helm_schema_core::Predicate;

use super::eval::Interpreter;
use super::holes::HoleEval;

/// How a no-render site demotes a called helper's rendered rows.
pub(super) enum RenderedDemotion {
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
pub(super) fn required_call_subjects(expr: &TemplateExpr) -> Vec<&TemplateExpr> {
    let mut subjects = Vec::new();
    collect_required_subjects(expr, &mut subjects);
    subjects
}

/// Whether a `required` subject is Helm-empty by construction: `nil`, an
/// empty string literal, an empty `dict`/`list`, or an `index`/`get` into
/// one of those (which yields nil for every key).
pub(super) fn subject_is_statically_helm_empty(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Literal(helm_schema_ast::Literal::Nil) => true,
        TemplateExpr::Literal(
            helm_schema_ast::Literal::String(text) | helm_schema_ast::Literal::RawString(text),
        ) => text.is_empty(),
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "dict" | "list") && args.is_empty() =>
        {
            true
        }
        TemplateExpr::Call { function, args }
            if matches!(function.as_str(), "index" | "get") && !args.is_empty() =>
        {
            subject_is_statically_helm_empty(&args[0])
        }
        _ => false,
    }
}

pub(super) fn collect_required_subjects<'e>(
    expr: &'e TemplateExpr,
    out: &mut Vec<&'e TemplateExpr>,
) {
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

/// The claim paths of the helper calls one hole resolved (read and rendered
/// sources), the sibling set for ambient-condition scoping.
pub(super) fn helper_claim_paths(effects: &Effects) -> std::collections::BTreeSet<String> {
    let mut claims: std::collections::BTreeSet<String> = effects
        .helper_reads
        .iter()
        .map(|read| read.values_path.clone())
        .collect();
    claims.extend(effects.helper_rendered.iter().map(|row| row.path.clone()));
    claims.extend(
        effects
            .helper_dependency_rendered
            .iter()
            .map(|row| row.path.clone()),
    );
    claims
}

/// Whether an ambient predicate belongs on one flowing path's assignment
/// meta: truthiness conditions about a different flowing path of the same
/// assignment describe that sibling's branch (unrelated paths keep the
/// condition).
pub(super) fn predicate_applies_to_flowing_path(
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

impl Interpreter<'_> {
    /// Evaluate one condition expression through the shared value lattice
    /// with bound-helper resolution (guard-read derivation for conditions
    /// over helper calls).
    pub(super) fn eval_hole_exprs_for_condition(&mut self, expr: &TemplateExpr) -> HoleEval {
        self.eval_hole_exprs(std::slice::from_ref(expr))
    }

    /// Absorbs every runtime effect produced while evaluating a control header.
    ///
    /// Header values are not rendered, so their ordinary output paths remain owned by the
    /// control evaluator. Strict call contracts, conversions, and called-helper effects still
    /// execute before the header decides whether its body runs.
    pub(super) fn absorb_header_execution_effects(
        &mut self,
        expr: &TemplateExpr,
    ) -> std::collections::BTreeSet<String> {
        let hole = self.eval_hole_exprs_for_condition(expr);
        let mut effects = hole.effects;
        effects.bound_output_paths.clear();
        let strict_paths: std::collections::BTreeSet<String> = effects
            .helper_fails
            .iter()
            .flat_map(runtime_requirement_paths)
            .collect();
        // Derived booleans and counts erase output identity, but their operands remain strict.
        // A header has no output slot to protect, so the hard operand contract wins over that
        // placement-only erasure. Truly total calls such as `join` have no failure capture.
        effects
            .shape_erased_paths
            .retain(|path| !strict_paths.contains(path));

        for path in &effects.string_contract_paths {
            let sink = if self.hint_scope_is_unconditional(path) {
                &mut self.type_hints
            } else {
                &mut self.guarded_type_hints
            };
            sink.entry(path.clone())
                .or_default()
                .insert("string".to_string());
        }

        let has_helper_claims = !effects.helper_reads.is_empty()
            || !effects.helper_rendered.is_empty()
            || !effects.helper_dependency_rendered.is_empty();
        let mut claims = if has_helper_claims {
            helper_claim_paths(&effects)
        } else {
            std::collections::BTreeSet::new()
        };
        if has_helper_claims {
            claims.extend(effects.type_hints.keys().cloned());
            claims.extend(effects.fallback_type_hints.keys().cloned());
        }
        self.absorb_hole_effects(&effects, RenderedDemotion::None);

        claims
            .iter()
            .filter(|path| !helm_schema_core::values_path_has_descendant(path, &claims))
            .cloned()
            .collect()
    }

    /// Record every `required(message, subject)` guardrail in the
    /// expressions: rendering fails under the ambient predicates whenever a
    /// subject resolving to exactly one values path is Helm-empty. Member
    /// bindings resolve here (the value-path context sees them), so ranged
    /// subjects attach per-member requirements.
    pub(super) fn record_required_subjects(&mut self, exprs: &[TemplateExpr]) {
        let mut subject_paths = Vec::new();
        let mut statically_empty = false;
        {
            let context = self.value_path_context();
            for expr in exprs {
                for subject in required_call_subjects(expr) {
                    // `required "msg" nil` (and its `index (dict) …`
                    // spellings) is a pure validator: whenever control
                    // reaches it, rendering terminates, so the ambient
                    // predicates form a terminal clause.
                    if subject_is_statically_helm_empty(subject) {
                        statically_empty = true;
                        continue;
                    }
                    let paths = context.paths_for_expr(subject);
                    if paths.len() == 1 {
                        subject_paths.extend(paths);
                    }
                }
            }
        }
        if statically_empty {
            self.record_fail_condition();
        }
        for path in subject_paths {
            self.record_required_condition(&path);
        }
    }

    /// Evaluate the expressions of one output hole through the shared value
    /// lattice, resolving bound helper calls via the memoized summaries.
    pub(super) fn eval_hole_exprs(&mut self, exprs: &[TemplateExpr]) -> HoleEval {
        let current_dot = self.current_value_dot();
        let mut env = EvalEnv::from_helper_context(Some(&self.root_bindings), current_dot.as_ref())
            .without_helper_call_args();
        // Locals (`$x`) and root bindings (`.x`) are distinct namespaces:
        // roots stay in `root_fields` so a helper-arg key never shadows a
        // same-named body local. Range VALUE variables resolve to member
        // identity (`$arg` in `range $arg := .Values.args` is `args.*`),
        // the same identity the range dot already carries, so member
        // consumers (`tpl $arg`) bind their contracts per member;
        // explicit fragment values shadow them where both exist.
        env.locals = self.locals.range_member_values.clone();
        env.locals.extend(
            self.locals
                .fragment_values
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        env.local_default_paths = self.locals.default_paths.clone();
        env.local_output_meta = self.locals.output_meta.clone();
        env.member_host_conversions = self.member_host_conversions.clone();
        env.active_predicates = self.active_predicates.clone();
        env.root_truthy_predicates = self.root_truthy_predicates.clone();
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
                &env.locals,
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
                Guard::RangeKeyPrefix { .. }
                | Guard::RangeKeyEquals { .. }
                | Guard::RangeKeyMatches { .. } => true,
                Guard::Truthy { path: guard_path }
                | Guard::Not { path: guard_path }
                | Guard::Absent { path: guard_path }
                | Guard::Eq {
                    path: guard_path, ..
                }
                | Guard::NotEq {
                    path: guard_path, ..
                }
                | Guard::MatchesPattern {
                    path: guard_path, ..
                }
                | Guard::IntGt {
                    path: guard_path, ..
                }
                | Guard::IntLt {
                    path: guard_path, ..
                }
                | Guard::AtMostOneMember { path: guard_path }
                | Guard::MinMembers {
                    path: guard_path, ..
                }
                | Guard::HasKey {
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
                Predicate::Approximate { .. } => true,
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

    pub(super) fn absorb_hole_effects(&mut self, effects: &Effects, demotion: RenderedDemotion) {
        self.absorb_member_host_conversions(&effects.member_host_conversions);
        self.apply_root_set_mutations(&effects.root_set_mutations, &effects.root_set_predicates);
        self.values_default_sources_observed
            .extend(effects.values_default_sources.iter().cloned());
        self.values_root_helper_includes_observed
            .extend(effects.values_root_helper_includes.iter().cloned());
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
        for (path, hints) in &effects.fallback_type_hints {
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
        self.parsed_yaml_input_paths
            .extend(effects.parsed_yaml_input_paths.iter().cloned());
        self.yaml_serialized_paths
            .extend(effects.yaml_serialized_paths.iter().cloned());
        self.shape_erased_paths
            .extend(effects.shape_erased_paths.iter().cloned());
        self.range_modes.merge(&effects.range_modes);
        // Only an unconditional consumer contributes a path-wide contract.
        // Conditional consumers travel through their placed rows and fail
        // captures; promoting them here would reject values in dead arms.
        if self.active_predicates.is_empty() {
            self.string_contract_paths
                .extend(effects.string_contract_paths.iter().cloned());
        }
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
            .chain(
                effects
                    .helper_dependency_rendered
                    .iter()
                    .flat_map(|row| row.meta.suppress_predicate_paths.iter()),
            )
            .chain(effects.helper_suppressed_paths.iter())
            .collect();
        let claims = helper_claim_paths(effects);
        self.absorb_helper_reads_with_suppression(&effects.helper_reads, &suppressed, &claims);
        for row in &effects.helper_dependency_rendered {
            let kind = if row.encoded {
                ValueKind::Scalar
            } else {
                row.kind
            };
            self.push_meta_reads(&row.path, kind, &row.meta, &claims, true);
        }
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

    pub(super) fn apply_root_set_mutations(
        &mut self,
        mutations: &std::collections::BTreeMap<String, AbstractValue>,
        predicates: &std::collections::BTreeMap<String, Predicate>,
    ) {
        for (key, value) in mutations {
            self.root_truthy_predicates.remove(key);
            self.root_set_predicates_observed.remove(key);
            self.root_bindings.insert(key.clone(), value.clone());
            self.root_set_mutations_observed
                .insert(key.clone(), value.clone());
            if let Some(predicate) = predicates.get(key) {
                self.root_truthy_predicates
                    .insert(key.clone(), predicate.clone());
                self.root_set_predicates_observed
                    .insert(key.clone(), predicate.clone());
            }
        }
    }

    /// Pathless reads for every values path the hole's effects attribute
    /// (used where the current pipeline suppresses rendered placement:
    /// assignment right-hand sides and render-suppressed fragment holes).
    /// Ancestor paths with a more specific path in the same hole are dropped
    /// (the most-specific-path rule for scalar sites), and paths already
    /// covered by rendered helper rows read through those rows instead.
    pub(super) fn push_effects_reads(&mut self, hole: &HoleEval, kind: ValueKind) {
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
}

fn runtime_requirement_paths(
    capture: &crate::eval_effect::FailCapture,
) -> std::collections::BTreeSet<String> {
    use crate::eval_effect::CaptureKind;

    match &capture.kind {
        CaptureKind::RangeKeyStrings { paths } | CaptureKind::CollectionItems { paths, .. } => {
            paths.clone()
        }
        CaptureKind::IndexAccess { path, .. } => [path.clone()].into_iter().collect(),
        CaptureKind::SplitIndexAccess { paths, .. } => paths.clone(),
        CaptureKind::ValueType { path, .. }
        | CaptureKind::ComparableKind { path, .. }
        | CaptureKind::ValuePattern { path, .. }
        | CaptureKind::QuotedSerialization { path, .. } => [path.clone()].into_iter().collect(),
        CaptureKind::Fail | CaptureKind::MemberAccess { .. } => capture
            .conjunction
            .last()
            .filter(|predicate| predicate_is_runtime_kind_requirement(predicate))
            .map(Predicate::value_paths)
            .unwrap_or_default()
            .into_iter()
            .collect(),
    }
}

fn predicate_is_runtime_kind_requirement(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Guard(
            Guard::TypeIs { .. } | Guard::NotTypeIs { .. } | Guard::MatchesPattern { .. },
        ) => true,
        Predicate::Not(inner) => predicate_is_runtime_kind_requirement(inner),
        Predicate::True
        | Predicate::False
        | Predicate::Approximate { .. }
        | Predicate::Guard(_)
        | Predicate::And(_)
        | Predicate::Or(_) => false,
    }
}
