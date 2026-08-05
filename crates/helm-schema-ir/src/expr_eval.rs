use std::collections::{BTreeMap, BTreeSet, HashMap};

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_call_eval::{eval_call_with_helper_calls, eval_pipeline_with_helper_calls};
use crate::function_semantics::{CollectionShape, function_semantics};
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use helm_schema_core::Predicate;

pub(crate) trait HelperCallValueResolver {
    fn resolve_helper_call(&mut self, name: &str, arg: Option<&TemplateExpr>)
    -> Option<EvalResult>;

    fn resolve_implicit_template_call(
        &mut self,
        _suffix: &str,
        _arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        None
    }
}

struct NoHelperCallResolver;

impl HelperCallValueResolver for NoHelperCallResolver {
    fn resolve_helper_call(
        &mut self,
        _name: &str,
        _arg: Option<&TemplateExpr>,
    ) -> Option<EvalResult> {
        None
    }
}

pub(crate) fn eval_expr(expr: &TemplateExpr, env: &EvalEnv) -> EvalResult {
    let mut resolver = NoHelperCallResolver;
    eval_expr_with_helper_calls(expr, env, &mut resolver)
}

pub(crate) fn direct_values_path(expr: &TemplateExpr) -> Option<String> {
    if !matches!(
        expr.deparen(),
        TemplateExpr::Field(_) | TemplateExpr::Selector { .. }
    ) {
        return None;
    }

    eval_expr(expr, &EvalEnv::default())
        .value
        .as_ref()
        .and_then(AbstractValue::unique_path)
}

/// The values identity a helper's CALL DICT binds to the first segment of a
/// dot-relative access, with the segments navigated from it.
///
/// A dict has no identity of its own — reading `.ctx` says nothing about the
/// root context — but the value it binds does: `include "x" (dict "config"
/// .Values.pdb)` makes the body's `.config.enabled` read
/// `.Values.pdb.enabled`, so a deleted `pdb` binds the member to nil and the
/// navigation aborts exactly like the direct spelling. Only a member bound
/// to a raw values path qualifies, and only where the body navigates PAST
/// it: `.config` alone reads nil without aborting.
fn call_dict_member_identity(path: &[String], env: &EvalEnv) -> Option<(String, Vec<String>)> {
    let (head, tail) = path.split_first()?;
    if tail.is_empty() {
        return None;
    }
    let bound = env
        .dot
        .as_ref()?
        .apply_to_path(std::slice::from_ref(head))?;
    Some((bound.direct_values_identity()?, tail.to_vec()))
}

/// Record that `segments` was reached by Go field access: every nonterminal
/// prefix at or past `accessed_from` must exist and host members whenever
/// the surrounding control flow executes the access. The captures ride the
/// shared fail channel so ambient guards join at absorption, and the signal
/// builder folds them into one bypass-proof arm per path instead of one per
/// read.
fn record_member_access_captures(
    segments: &[String],
    accessed_from: usize,
    env: &EvalEnv,
    effects: &mut Effects,
) {
    if segments.len() < 2 {
        return;
    }
    for len in accessed_from.max(1)..segments.len() {
        let Some(prefix) = segments.get(..len) else {
            continue;
        };
        if prefix.iter().any(String::is_empty) {
            continue;
        }
        let path = helm_schema_core::join_value_path(prefix.iter().cloned());
        record_member_host_capture(&path, &[], env, effects);
    }
}

fn record_grouped_member_access_captures(
    receiver_path: &str,
    selected_path: &[String],
    missing_receiver_aborts: bool,
    env: &EvalEnv,
    effects: &mut Effects,
) {
    let mut segments = helm_schema_core::split_value_path(receiver_path);
    let receiver_len = segments.len();
    segments.extend(selected_path.iter().cloned());
    let receiver_guard = (!receiver_path.is_empty() && !missing_receiver_aborts).then(|| {
        Predicate::from(crate::Guard::Absent {
            path: receiver_path.to_string(),
        })
        .negated()
    });

    for len in receiver_len..segments.len() {
        let Some(prefix) = segments.get(..len) else {
            continue;
        };
        let target = helm_schema_core::join_value_path(prefix.iter().cloned());
        if target.is_empty() {
            continue;
        }
        let outer = receiver_guard.as_slice();
        record_member_host_capture(&target, outer, env, effects);
    }
}

fn record_member_host_capture(
    path: &str,
    outer_predicates: &[Predicate],
    env: &EvalEnv,
    effects: &mut Effects,
) {
    let handled_kinds = env
        .member_host_conversions
        .iter()
        .filter(|conversion| {
            conversion.path == path
                && conversion
                    .outer_predicates
                    .iter()
                    .all(|predicate| env.active_predicates.contains(predicate))
        })
        .map(|conversion| conversion.input_kind.clone())
        .collect();
    let mut conjunction = outer_predicates.to_vec();
    conjunction.push(
        Predicate::from(crate::Guard::TypeIs {
            path: path.to_string(),
            schema_type: "object".to_string(),
        })
        .negated(),
    );
    let capture = crate::eval_effect::FailCapture {
        conjunction,
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::MemberAccess { handled_kinds },
    };
    if !effects.helper_fails.contains(&capture) {
        effects.helper_fails.push(capture);
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(crate) fn eval_expr_with_helper_calls(
    expr: &TemplateExpr,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    match expr {
        TemplateExpr::Parenthesized(inner) => eval_expr_with_helper_calls(inner, env, resolver),
        TemplateExpr::Field(path) if path.first().is_some_and(|segment| segment == "Values") => {
            // Inside `with $copy` over a context copy whose `Values` member
            // was replaced (`set $copy.Values …`), `.Values.…` reads the
            // copy's overridden member. Only the Overlay shape that
            // mutation produces re-routes; every other context keeps the
            // root-values shortcut, including `$.Values.…`, which names
            // the genuine root and never resolves here.
            if let Some(AbstractValue::Overlay { entries, .. }) = &env.dot
                && entries.contains_key("Values")
                && let Some(value) = env.dot.as_ref().and_then(|dot| dot.apply_to_path(path))
            {
                EvalResult::from_value(value)
            } else {
                let Some((_, tail)) = path.split_first() else {
                    return EvalResult::none();
                };
                root_values_selector_result(tail, env)
            }
        }
        TemplateExpr::Field(path) if path.is_empty() => {
            EvalResult::from_value(env.dot.clone().unwrap_or(AbstractValue::RootContext))
        }
        TemplateExpr::Field(path) => {
            let dot_base = env
                .dot
                .as_ref()
                .and_then(AbstractValue::direct_values_identity);
            let value = env.dot.as_ref().and_then(|value| value.apply_to_path(path));
            let value = value.or_else(|| {
                if !env.allow_field_root_lookup {
                    return None;
                }
                let (head, tail) = path.split_first()?;
                env.root_fields
                    .get(head)
                    .and_then(|value| value.apply_to_path(tail))
            });
            let mut result = value
                .map(EvalResult::from_value)
                .unwrap_or_else(EvalResult::none);
            // What the access navigates: the dot's own values identity when
            // it has one, otherwise the identity a CALL DICT binds to the
            // first segment.
            let access = match dot_base {
                Some(base) => Some((base, path.clone())),
                None => call_dict_member_identity(path, env),
            };
            if let Some((base, navigated)) = access {
                let mut segments = helm_schema_core::split_value_path(&base);
                let accessed_from = segments.len();
                segments.extend(navigated);
                record_member_access_captures(&segments, accessed_from, env, &mut result.effects);
            }
            attach_root_field_semantics(&mut result, path, env);
            result
        }
        TemplateExpr::Selector { operand, path }
            if matches!(operand.as_ref(), TemplateExpr::Variable(var) if var.is_empty())
                && !env.locals.contains_key("")
                && path.first().is_some_and(|segment| segment == "Values") =>
        {
            let Some((_, tail)) = path.split_first() else {
                return EvalResult::none();
            };
            root_values_selector_result(tail, env)
        }
        TemplateExpr::Variable(var) if var.is_empty() => env.locals.get(var).cloned().map_or_else(
            || EvalResult::from_value(AbstractValue::RootContext),
            |value| local_value_result(var, value, None, env),
        ),
        TemplateExpr::Variable(var) if !var.is_empty() => {
            if let Some(value) = env.locals.get(var).cloned() {
                local_value_result(var, value, None, env)
            } else if let Some(dispatch) = local_scalar_dispatch(var, env) {
                EvalResult::none().with_scalar_dispatch(dispatch.clone())
            } else if let Some(predicate) = env
                .local_truthy_reductions
                .get(var)
                .or_else(|| env.local_truthy_reductions.get(var.trim_start_matches('$')))
            {
                EvalResult::none().with_truth(predicate.clone())
            } else {
                EvalResult::none()
            }
        }
        TemplateExpr::Selector { operand, path } => {
            if let Some(result) = static_semver_numeric_selector(operand, path, env, resolver) {
                return result;
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && let Some(value) = env
                    .locals
                    .get(var)
                    .and_then(|binding| binding.apply_to_path(path))
            {
                let local_base = env
                    .locals
                    .get(var)
                    .and_then(AbstractValue::direct_values_identity);
                let selected_paths = value.fragment_source_paths();
                let mut result = local_value_result(var, value, Some(&selected_paths), env);
                if let Some(base) = local_base {
                    // A `:=`-bound local is nil-SAFE to navigate for its own
                    // hop, exactly like the grouped receiver form:
                    // `$preset := .Values.global.affinity` renders with
                    // `affinity` deleted (argo-cd), while `$preset.a.b` still
                    // aborts on a missing `a` and a present non-object still
                    // aborts with "can't evaluate field". A RANGE member
                    // variable never went through a pipeline, so it keeps the
                    // direct chain's abort on every nil member.
                    if env.pipeline_bound_locals.contains(var) {
                        record_grouped_member_access_captures(
                            &base,
                            path,
                            false,
                            env,
                            &mut result.effects,
                        );
                    } else {
                        let mut segments = helm_schema_core::split_value_path(&base);
                        let accessed_from = segments.len();
                        segments.extend(path.iter().cloned());
                        record_member_access_captures(
                            &segments,
                            accessed_from,
                            env,
                            &mut result.effects,
                        );
                    }
                }
                return with_bound_selector_paths(result, expr, env);
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && !var.is_empty()
                && path.first().is_some_and(|segment| segment == "Values")
            {
                let Some((_, tail)) = path.split_first() else {
                    return EvalResult::none();
                };
                let result = root_values_selector_result(tail, env);
                return with_bound_selector_paths(result, expr, env);
            }
            if let TemplateExpr::Variable(var) = operand.as_ref()
                && var.is_empty()
                && !env.locals.contains_key(var)
                && let Some((head, tail)) = path.split_first()
                && let Some(value) = env
                    .root_fields
                    .get(head)
                    .and_then(|value| value.apply_to_path(tail))
            {
                let mut result = EvalResult::from_value(value);
                if tail.is_empty() {
                    attach_named_root_field_semantics(&mut result, head, env);
                }
                return with_bound_selector_paths(result, expr, env);
            }
            let base = eval_expr_with_helper_calls(operand, env, resolver);
            let grouped_receiver = matches!(operand.as_ref(), TemplateExpr::Parenthesized(_))
                .then(|| {
                    base.value
                        .as_ref()
                        .and_then(AbstractValue::direct_values_identity)
                })
                .flatten();
            // Sprig `get` returns `""` for a missing key, so selecting a
            // field from its result aborts instead of taking the grouped
            // receiver's ordinary nil-safe path.
            let missing_grouped_receiver_aborts = matches!(
                operand.deparen(),
                TemplateExpr::Call { function, .. } if function == "get"
            );
            let value = base
                .value
                .as_ref()
                .and_then(|value| value.apply_to_path(path));
            let mut effects = base.effects;
            if let Some(receiver_path) = grouped_receiver {
                record_grouped_member_access_captures(
                    &receiver_path,
                    path,
                    missing_grouped_receiver_aborts,
                    env,
                    &mut effects,
                );
            }
            effects.output_paths.clear();
            effects
                .bound_output_paths
                .extend(env.bound_values.selector_paths(expr));
            match value {
                Some(value) => {
                    let mut result = EvalResult::from_value(value);
                    result.effects.merge(effects);
                    result
                }
                None => EvalResult::with_effects(None, effects),
            }
        }
        TemplateExpr::Call { function, args } => {
            eval_call_with_helper_calls(function, args, env, resolver)
        }
        TemplateExpr::Pipeline(stages) => eval_pipeline_with_helper_calls(stages, env, resolver),
        TemplateExpr::Literal(literal) => literal_result(literal),
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            EvalResult::with_effects(
                None,
                eval_expr_with_helper_calls(value, env, resolver).effects,
            )
        }
        TemplateExpr::Variable(_) | TemplateExpr::Unknown(_) => EvalResult::none(),
    }
}

fn static_semver_numeric_selector(
    operand: &TemplateExpr,
    path: &[String],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> Option<EvalResult> {
    let [field] = path else {
        return None;
    };
    let TemplateExpr::Call { function, args } = operand.deparen() else {
        return None;
    };
    let [version] = args.as_slice() else {
        return None;
    };
    if function != "semver" {
        return None;
    }
    let version = eval_expr_with_helper_calls(version, env, resolver);
    let helm_schema_core::GuardValue::String(text) =
        version.scalar_dispatch.as_ref()?.constant_value()?
    else {
        return None;
    };
    let parsed = semver::Version::parse(text.strip_prefix(['v', 'V']).unwrap_or(&text)).ok()?;
    let component = match field.as_str() {
        "Major" => parsed.major,
        "Minor" => parsed.minor,
        "Patch" => parsed.patch,
        _ => return None,
    };
    let component = i64::try_from(component).ok()?;
    Some(
        EvalResult::with_effects(None, version.effects).with_scalar_dispatch(
            ScalarValueDispatch::constant(helm_schema_core::GuardValue::Int(component)),
        ),
    )
}

/// Evaluate `.Values`-rooted selector segments (the part after `Values`),
/// applying method resolution on the typed root receiver first so `AsMap`
/// continues from the root and the derived-text methods claim nothing.
fn root_values_selector_result(segments: &[String], env: &EvalEnv) -> EvalResult {
    let Some(tail) = crate::abstract_value::resolve_root_values_methods(segments) else {
        return EvalResult::none();
    };
    let mut result = EvalResult::from_value(values_path_value(tail, env));
    record_member_access_captures(tail, 0, env, &mut result.effects);
    result
}

fn values_path_value(tail: &[String], env: &EvalEnv) -> AbstractValue {
    if let Some(root) = env.root_fields.get("Values")
        && let Some(value) = root.apply_to_path(tail)
    {
        return value;
    }
    if tail.is_empty() {
        AbstractValue::values_root()
    } else {
        AbstractValue::ValuesPath(helm_schema_core::join_value_path(tail))
    }
}

pub(crate) fn eval_exprs_effects(exprs: &[TemplateExpr], env: &EvalEnv) -> Effects {
    let mut effects = Effects::default();
    for expr in exprs {
        effects.merge(eval_expr(expr, env).effects);
    }
    effects
}

pub(crate) fn eval_helper_exprs_direct_effects(
    exprs: &[TemplateExpr],
    bindings: &HashMap<String, AbstractValue>,
    current_dot: Option<&AbstractValue>,
) -> Effects {
    let env = EvalEnv::from_helper_context(Some(bindings), current_dot).without_helper_call_args();
    eval_exprs_effects(exprs, &env)
}

pub(crate) struct HelperArgBindings {
    pub(crate) bindings: HashMap<String, AbstractValue>,
    pub(crate) scalar_dispatches: BTreeMap<String, ScalarValueDispatch>,
    /// Whole-arg evaluated value when `bindings` derived from one evaluation
    /// (the non-dot, non-merge arm). Callers reuse it as the helper body dot
    /// instead of evaluating the same expression a second time.
    pub(crate) value: Option<AbstractValue>,
}

pub(crate) fn bindings_for_helper_arg_with(
    arg: Option<&TemplateExpr>,
    outer: Option<&HashMap<String, AbstractValue>>,
    mut eval_binding: impl FnMut(&TemplateExpr) -> EvalResult,
) -> HelperArgBindings {
    let Some(arg) = arg else {
        return HelperArgBindings {
            bindings: HashMap::new(),
            scalar_dispatches: BTreeMap::new(),
            value: None,
        };
    };

    // A helper receives the argument value, not the caller's side-channel
    // effects. Stamp binding-carried metadata onto that value before the
    // helper context is split into named fields.
    let mut eval_binding = |expr: &TemplateExpr| {
        let mut result = eval_binding(expr);
        let mut output_meta = result.effects.local_output_meta.clone();
        for path in result.effects.default_paths_with_local() {
            output_meta.entry(path).or_default().defaulted = true;
        }
        result.value = result
            .value
            .map(|value| value.with_output_meta(&output_meta));
        result
    };
    let (bindings, scalar_dispatches) = match arg.deparen() {
        TemplateExpr::Field(path) if path.is_empty() => {
            (outer.cloned().unwrap_or_default(), BTreeMap::new())
        }
        TemplateExpr::Call { function, args }
            if function_semantics(function).collection == CollectionShape::Merge =>
        {
            let mut merged = HashMap::new();
            let mut scalar_dispatches = BTreeMap::new();
            for arg in args {
                let result = eval_binding(arg);
                merged.extend(bindings_from_helper_arg_value(result.value, outer));
                scalar_dispatches.extend(result.field_scalar_dispatches);
            }
            (merged, scalar_dispatches)
        }
        _ => {
            let result = eval_binding(arg);
            let value = result.value;
            return HelperArgBindings {
                bindings: bindings_from_helper_arg_value(value.clone(), outer),
                scalar_dispatches: result.field_scalar_dispatches,
                value,
            };
        }
    };
    HelperArgBindings {
        bindings,
        scalar_dispatches,
        value: None,
    }
}

fn bindings_from_helper_arg_value(
    value: Option<AbstractValue>,
    outer: Option<&HashMap<String, AbstractValue>>,
) -> HashMap<String, AbstractValue> {
    match value {
        Some(AbstractValue::Dict(map)) => map.into_iter().collect(),
        Some(AbstractValue::RootContext) => outer.cloned().unwrap_or_default(),
        Some(AbstractValue::Overlay { entries, fallback }) => {
            let mut bindings = bindings_from_helper_arg_value(Some(*fallback), outer);
            bindings.extend(entries);
            bindings
        }
        _ => HashMap::new(),
    }
}

pub(crate) fn literal_helper_call_callee<'a>(
    function: &str,
    args: &'a [TemplateExpr],
) -> Option<&'a str> {
    if !is_helper_call_function(function) {
        return None;
    }
    let Some(TemplateExpr::Literal(lit)) = args.first().map(TemplateExpr::deparen) else {
        return None;
    };
    lit.as_string()
}

pub(crate) fn is_helper_call_function(function: &str) -> bool {
    matches!(function, "include" | "template")
}

fn local_value_result(
    var: &str,
    value: AbstractValue,
    selected_paths: Option<&BTreeSet<String>>,
    env: &EvalEnv,
) -> EvalResult {
    let source_paths = value.fragment_source_paths();
    let mut result = EvalResult::from_value(value);
    if env.pipeline_bound_locals.contains(var) {
        // A pipeline-bound local's fragment value records provenance, not
        // necessarily its runtime scalar (`default`, transforms, and helper
        // output can all retain a source path while changing the value).
        // Only the separately tracked semantic domains below may recover
        // truthiness or scalar dispatch for such a binding.
        result.truth = TruthCondition::Unknown;
        result.scalar_dispatch = None;
    }
    result.effects.local_source_paths = source_paths;
    if let Some(default_paths) = env.local_default_paths.get(var) {
        result
            .effects
            .local_default_paths
            .extend(default_paths.iter().cloned());
        result.effects.add_default_paths(default_paths.clone());
    }
    if let Some(meta_by_path) = env.local_output_meta.get(var) {
        match selected_paths {
            Some(paths) => result.effects.merge_local_output_meta(
                meta_by_path
                    .iter()
                    .filter(|(path, _meta)| paths.contains(*path)),
            ),
            None => result.effects.merge_local_output_meta(meta_by_path.iter()),
        }
    }
    if selected_paths.is_none() {
        if let Some(dispatch) = local_scalar_dispatch(var, env) {
            result.truth = dispatch.truth_condition();
            result.scalar_dispatch = Some(dispatch.clone());
        } else if let Some(predicate) = env
            .local_truthy_reductions
            .get(var)
            .or_else(|| env.local_truthy_reductions.get(var.trim_start_matches('$')))
        {
            result.truth = TruthCondition::exact(predicate.clone());
        }
    }
    result
}

fn local_scalar_dispatch<'a>(var: &str, env: &'a EvalEnv) -> Option<&'a ScalarValueDispatch> {
    env.local_scalar_dispatches
        .get(var)
        .or_else(|| env.local_scalar_dispatches.get(var.trim_start_matches('$')))
}

fn literal_result(literal: &Literal) -> EvalResult {
    let truthy = match literal {
        Literal::Bool(value) => *value,
        Literal::Int(value) => *value != 0,
        Literal::Float(value) => *value != 0.0,
        Literal::String(value) | Literal::RawString(value) => !value.is_empty(),
        Literal::Nil => false,
    };
    let value = match literal {
        Literal::String(value) | Literal::RawString(value) => Some(AbstractValue::StringSet(
            [value.clone()].into_iter().collect(),
        )),
        _ => None,
    };
    let result = EvalResult::with_effects(value, Effects::default()).with_truth(if truthy {
        Predicate::True
    } else {
        Predicate::False
    });
    let scalar = match literal {
        Literal::String(value) | Literal::RawString(value) => {
            Some(helm_schema_core::GuardValue::string(value.clone()))
        }
        Literal::Bool(value) => Some(helm_schema_core::GuardValue::Bool(*value)),
        Literal::Int(value) => Some(helm_schema_core::GuardValue::Int(*value)),
        Literal::Nil => Some(helm_schema_core::GuardValue::Null),
        Literal::Float(_) => None,
    };
    match scalar {
        Some(scalar) => result.with_scalar_dispatch(ScalarValueDispatch::constant(scalar)),
        None => result,
    }
}

fn attach_root_field_semantics(result: &mut EvalResult, path: &[String], env: &EvalEnv) {
    let [field] = path else {
        return;
    };
    let root_dot = env
        .dot
        .as_ref()
        .is_none_or(|dot| matches!(dot, AbstractValue::RootContext));
    if root_dot || env.root_field_semantics_on_current_dot {
        attach_named_root_field_semantics(result, field, env);
    }
}

fn attach_named_root_field_semantics(result: &mut EvalResult, field: &str, env: &EvalEnv) {
    if let Some(dispatch) = env.root_value_dispatches.get(field) {
        result.truth = dispatch.truth_condition();
        result.scalar_dispatch = Some(dispatch.clone());
    } else if let Some(predicate) = env.root_truthy_predicates.get(field) {
        result.truth = TruthCondition::exact(predicate.clone());
    }
}

fn with_bound_selector_paths(
    mut result: EvalResult,
    expr: &TemplateExpr,
    env: &EvalEnv,
) -> EvalResult {
    result
        .effects
        .bound_output_paths
        .extend(env.bound_values.selector_paths(expr));
    result
}

pub(crate) fn apply_local_set_mutations_expr(expr: &TemplateExpr, env: &mut EvalEnv) -> bool {
    let mutation_expr = match expr {
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            value.as_ref()
        }
        _ => expr,
    };
    let result = eval_expr(mutation_expr, env);
    env.apply_local_set_mutations(&result.effects.local_set_mutations)
}
