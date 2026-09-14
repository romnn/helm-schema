use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult, ProvenOperand, ProvenOperands};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::function_semantics::ArgumentEvaluationMode;
use helm_schema_core::Predicate;

use super::eval_all_args;
use super::strict_operands::{
    layered_strict_operand_identity_paths, record_operand_presence_result,
    strict_operand_selection_conjunctions,
};
use super::value_facts::identity_value_paths;

/// `dig "k1" … "kn" default subject`: walk literal keys through the
/// subject dict, falling back to `default` when a key is MISSING. A key
/// that is present but not a map aborts rendering (sprig type-asserts
/// every step), so the subject and every intermediate key carry a
/// truthy⇒object contract; the dug value itself may be any type.
pub(super) fn eval_dig(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some((subject_expr, rest)) = args.split_last() else {
        return eval_all_args(args, env, resolver);
    };
    let Some((default_expr, key_exprs)) = rest.split_last() else {
        return eval_all_args(args, env, resolver);
    };
    let mut keys = Vec::new();
    for key in key_exprs {
        match key.deparen() {
            TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
                keys.push(value.clone());
            }
            _ => return eval_all_args(args, env, resolver),
        }
    }
    let subject = eval_expr_with_helper_calls(subject_expr, env, resolver);
    let default_result = eval_expr_with_helper_calls(default_expr, env, resolver);
    // Subject-level claims hold in their strict form only for a RAW
    // identity subject: a selection chain (`x | default dict`) reaches the
    // dig with its fallback exactly in the states the raw path is falsy or
    // absent, so claiming the raw path's presence or even-null type there
    // would reject documents that render through the fallback.
    let raw_subject = matches!(
        subject.value.as_ref(),
        Some(AbstractValue::ValuesPath(_) | AbstractValue::JsonDecodedPath(_))
    );
    // A chain still type-asserts whatever it does pass through, so its
    // TRUTHY states carry the assertion. The claim needs a single
    // identity: with several candidates (`coalesce a b`) only the first
    // truthy one is dug, and a truthy-scoped claim on each would reject a
    // shadowed candidate the dig never sees.
    let chain_subject_path = (!raw_subject)
        .then(|| identity_value_paths(subject.value.as_ref()))
        .filter(|paths| paths.len() == 1)
        .and_then(|paths| paths.into_iter().next())
        .filter(|path| !path.is_empty());
    let mut effects = subject.effects;
    effects.merge(default_result.effects);
    for path in identity_value_paths(subject.value.as_ref()) {
        let mut step = path;
        for prefix_len in 0..keys.len() {
            if prefix_len > 0 {
                let Some(key) = prefix_len.checked_sub(1).and_then(|index| keys.get(index)) else {
                    continue;
                };
                step = helm_schema_core::append_value_path(&step, key);
            }
            // Digging from the whole-values root: the root map itself has
            // no path-level contract to state.
            if step.is_empty() {
                continue;
            }
            let captures = if prefix_len == 0 {
                if raw_subject {
                    raw_subject_captures(&step)
                } else {
                    chain_subject_path
                        .as_ref()
                        .filter(|path| *path == &step)
                        .map(|path| vec![chain_subject_capture(path)])
                        .unwrap_or_default()
                }
            } else {
                // An INTERMEDIATE step falls back to the default when nil
                // (trivy-operator's nulled `trivy.resources` renders) but
                // aborts on any other non-map — including Helm-falsy
                // scalars — so the exact scope is present-and-non-null,
                // which is `Guard::Absent`'s negation.
                vec![crate::eval_effect::FailCapture {
                    conjunction: vec![
                        Predicate::from(crate::Guard::Absent {
                            path: helm_schema_core::ValuesPath::parse(&step),
                        })
                        .negated(),
                        Predicate::from(crate::Guard::TypeIs {
                            path: helm_schema_core::ValuesPath::parse(&step),
                            schema_type: "object".to_string(),
                        })
                        .negated(),
                    ]
                    .into(),
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::Fail,
                }]
            };
            for capture in captures {
                effects.observed_facts.captures.insert(capture);
            }
        }
    }
    let value = subject.value.and_then(|value| value.apply_to_path(&keys));
    // The dug leaf is a READ of that path whose absence falls back to the
    // literal default: an output path (so required-subject walking and
    // read rows see it) marked defaulted, exactly like `default`.
    for path in identity_value_paths(value.as_ref()) {
        let path = helm_schema_core::ValuesPath::parse(&path);
        effects.output_paths.insert(path.clone());
        effects.defaults.insert(path);
    }
    EvalResult::with_effects(value, effects)
}

/// The claims a RAW identity subject carries. `dig` type-asserts the
/// subject before any missing-key handling, so a present-but-null subject
/// aborts too (kube-prometheus-stack's nulled `customRules`). The strict
/// `HasKey` conjunct self-scopes the type claim — the dig-subject kind keeps
/// null rejected where a truthy-scoped arm would go vacuous — while the
/// companion presence claim covers the absent state (a missing subject reads
/// as nil and aborts the same assertion; loki's null-deleted
/// `storage_config`).
fn raw_subject_captures(path: &str) -> Vec<crate::eval_effect::FailCapture> {
    let Some((parent, leaf)) =
        helm_schema_core::split_value_path(path)
            .split_last()
            .map(|(leaf, parents)| {
                (
                    helm_schema_core::join_encoded_value_path(parents.iter()),
                    leaf.clone(),
                )
            })
    else {
        return Vec::new();
    };
    vec![
        crate::eval_effect::FailCapture {
            conjunction: Vec::new().into(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::RequiredPresence {
                path: helm_schema_core::ValuesPath::parse(path),
            },
        },
        crate::eval_effect::FailCapture {
            conjunction: vec![Predicate::from(crate::Guard::HasKey {
                path: helm_schema_core::ValuesPath::parse(&parent),
                key: leaf,
            })]
            .into(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::DigSubject {
                path: helm_schema_core::ValuesPath::parse(path),
            },
        },
    ]
}

/// The claim a SELECTION chain subject carries. `dig` type-asserts whatever
/// the chain hands it, so a TRUTHY non-map aborts ("interface conversion:
/// interface {} is string, not map[string]interface {}") while `""`, `0`,
/// `false`, an empty collection, null, and absence all render the fallback.
/// The self-truthy guard states that, and it keeps the falsy set out of the
/// base host, which the dug leaf's own read would otherwise type as an
/// object.
fn chain_subject_capture(path: &str) -> crate::eval_effect::FailCapture {
    crate::eval_effect::FailCapture {
        conjunction: vec![Predicate::from(crate::Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse(path),
        })]
        .into(),
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::DigSubject {
            path: helm_schema_core::ValuesPath::parse(path),
        },
    }
}

pub(super) fn eval_index(
    args: &[TemplateExpr],
    object_host: bool,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> EvalResult {
    let Some((base_expr, path_args)) = args.split_first() else {
        return EvalResult::none();
    };
    let base = eval_expr_with_helper_calls(base_expr, env, resolver);
    let mut execution_effects = Effects::default();
    if object_host {
        record_member_host_access(&base, &mut execution_effects);
    }
    // Both spellings reject a nil subject before any key lookup: Go's
    // `index` answers "index of untyped nil" (cilium's
    // `index .Values.extraConfig …`) and sprig's `get` type-asserts its
    // map parameter.
    record_operand_presence_result(&base, &mut execution_effects);
    if base.value.is_none() {
        execution_effects.merge(base.effects);
        return EvalResult::with_effects(None, execution_effects);
    }

    let mut steps = Vec::new();
    for arg in path_args {
        let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
        execution_effects.merge(arg_result.effects);
        let Some(options) = path_segment_options(arg, arg_result.value.as_ref()) else {
            execution_effects.merge(base.effects);
            return EvalResult::with_effects(None, execution_effects);
        };
        let literal_key = matches!(
            arg.deparen(),
            TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_) | Literal::Int(_))
        );
        steps.push(IndexStep {
            options,
            literal_key,
        });
    }

    let mut result = project_index_result(base, &steps, env);
    result.effects.merge(execution_effects);
    result
}

fn project_index_result(mut base: EvalResult, steps: &[IndexStep], env: &EvalEnv) -> EvalResult {
    let proven_operands = base.proven_operands.take();
    let mut effects = base.effects;
    let value = base
        .value
        .and_then(|value| project_index_value(value, steps, &mut effects));
    let mut result = value.map_or_else(
        || EvalResult::with_effects(None, Effects::default()),
        |value| EvalResult::from_value_with_memo(value, env.predicate_memo.as_ref()),
    );
    result.effects.merge(effects);
    result.proven_operands = proven_operands.map(|proven| ProvenOperands {
        known: proven
            .known
            .into_iter()
            .map(|operand| ProvenOperand {
                condition: operand.condition,
                evaluation_mode: ArgumentEvaluationMode::Evaluated,
                result: Box::new(project_index_result(*operand.result, steps, env)),
            })
            .collect(),
        has_unresolved: proven.has_unresolved,
    });
    result
}

fn project_index_value(
    value: AbstractValue,
    steps: &[IndexStep],
    effects: &mut Effects,
) -> Option<AbstractValue> {
    let mut values = vec![value];
    for step in steps {
        // A variable key must not extend a whole-values-root identity that
        // rides one arm of a choice subject.
        // The uncorrelated root arm would otherwise pair with another arm's
        // key and mint a values property the chart never reads.
        let values_snapshot: Vec<AbstractValue> = if step.literal_key {
            values.clone()
        } else {
            values
                .iter()
                .map(|value| match value {
                    AbstractValue::Choice(_)
                    | AbstractValue::FirstTruthy(_)
                    | AbstractValue::MergedLayers(_) => without_values_root_identity(value),
                    other => other.clone(),
                })
                .collect()
        };
        let mut next_values = Vec::new();
        for value in &values_snapshot {
            let base_paths = value.paths();
            for option in &step.options {
                if option.integer_index
                    && let Some(index) = option
                        .segments
                        .first()
                        .and_then(|segment| segment.parse::<usize>().ok())
                {
                    if let AbstractValue::SplitList {
                        source_paths,
                        separator,
                        total_text_preimage,
                    } = value
                    {
                        let capture = crate::eval_effect::FailCapture {
                            conjunction: Vec::new().into(),
                            ranged: crate::range_modes::RangeModes::default(),
                            kind: crate::eval_effect::CaptureKind::SplitIndexAccess {
                                paths: source_paths.clone(),
                                separator: separator.clone(),
                                index,
                                total_text_preimage: *total_text_preimage,
                            },
                        };
                        effects.observed_facts.captures.insert(capture);
                    }
                    for path in identity_value_paths(Some(value)) {
                        for conjunction in
                            super::strict_operands::operand_selection_conjunctions(effects, &path)
                        {
                            let capture = crate::eval_effect::FailCapture {
                                conjunction,
                                ranged: crate::range_modes::RangeModes::default(),
                                kind: crate::eval_effect::CaptureKind::IndexAccess {
                                    path: helm_schema_core::ValuesPath::parse(&path),
                                    index,
                                },
                            };
                            effects.observed_facts.captures.insert(capture);
                        }
                    }
                }
                if let Some(next) = apply_index_segment(value, option) {
                    for next_path in next.paths() {
                        for base_path in &base_paths {
                            if base_path.segments().next().is_some()
                                && next_path.is_descendant_of(base_path)
                            {
                                effects
                                    .local_output_meta
                                    .entry(next_path.clone())
                                    .or_default()
                                    .suppress_predicate_path(base_path.clone());
                            }
                        }
                    }
                    next_values.push(next);
                }
            }
        }
        values = next_values;
    }
    AbstractValue::choice(values)
}

pub(super) fn record_member_host_access(operand: &EvalResult, effects: &mut Effects) {
    for (path, shadow) in layered_strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.extend(shadow.iter().cloned());
            conjunction.push(
                Predicate::from(crate::Guard::TypeIs {
                    path: helm_schema_core::ValuesPath::parse(&path),
                    schema_type: "object".to_string(),
                })
                .negated(),
            );
            let capture = crate::eval_effect::FailCapture {
                conjunction,
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::MemberAccess {
                    handled_kinds: BTreeSet::new(),
                },
            };
            effects.observed_facts.captures.insert(capture);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PathSegmentOption {
    segments: Vec<String>,
    integer_index: bool,
}

struct IndexStep {
    options: Vec<PathSegmentOption>,
    literal_key: bool,
}

/// Replace whole-values-root identity arms with opaque present values, so
/// a variable-key navigation cannot fabricate root member paths; container
/// structure and non-root identities stay intact.
fn without_values_root_identity(value: &AbstractValue) -> AbstractValue {
    match value {
        AbstractValue::ValuesPath(path) if path.segments().len() == 0 => AbstractValue::Unknown,
        AbstractValue::JsonDecodedPath(path) | AbstractValue::OutputPath(path, _)
            if path.segments().next().is_none() =>
        {
            AbstractValue::Unknown
        }
        AbstractValue::Choice(choices) => {
            AbstractValue::choice(choices.iter().map(without_values_root_identity).collect())
                .unwrap_or(AbstractValue::Unknown)
        }
        AbstractValue::FirstTruthy(candidates) => AbstractValue::first_truthy(
            candidates
                .iter()
                .map(without_values_root_identity)
                .collect(),
        )
        .unwrap_or(AbstractValue::Unknown),
        AbstractValue::MergedLayers(layers) => {
            AbstractValue::merged_layers(layers.iter().map(without_values_root_identity).collect())
                .unwrap_or(AbstractValue::Unknown)
        }
        other => other.clone(),
    }
}

pub(super) fn apply_index_segment(
    value: &AbstractValue,
    option: &PathSegmentOption,
) -> Option<AbstractValue> {
    if !option.integer_index {
        return value.apply_to_path(&option.segments);
    }

    match value {
        AbstractValue::List(items) => {
            let index = option.segments.first()?.parse::<usize>().ok()?;
            items.get(index).cloned()
        }
        AbstractValue::SplitList { .. } => Some(AbstractValue::Unknown),
        AbstractValue::Choice(choices) => AbstractValue::choice(
            choices
                .iter()
                .filter_map(|choice| apply_index_segment(choice, option))
                .collect(),
        ),
        AbstractValue::FirstTruthy(candidates) => AbstractValue::choice(
            candidates
                .iter()
                .filter_map(|candidate| apply_index_segment(candidate, option))
                .collect(),
        ),
        _ => value.indexed_item(),
    }
}

pub(super) fn path_segment_options(
    expr: &TemplateExpr,
    evaluated_value: Option<&AbstractValue>,
) -> Option<Vec<PathSegmentOption>> {
    match expr.deparen() {
        TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)) => {
            Some(vec![PathSegmentOption {
                segments: vec![value.clone()],
                integer_index: false,
            }])
        }
        TemplateExpr::Literal(Literal::Int(value)) => Some(vec![PathSegmentOption {
            segments: vec![value.to_string()],
            integer_index: true,
        }]),
        _ => {
            let strings = evaluated_value
                .map(AbstractValue::strings)
                .unwrap_or_default();
            if strings.is_empty() {
                None
            } else {
                // An evaluated key selects exactly ONE member: `index`/`get`
                // treat the string atomically, so a dotted key stays a
                // single (escaped) segment.
                let mut options = Vec::new();
                for value in strings {
                    options.push(PathSegmentOption {
                        segments: vec![value.clone()],
                        integer_index: false,
                    });
                }
                options.sort_by(|left, right| left.segments.cmp(&right.segments));
                options.dedup();
                Some(options)
            }
        }
    }
}
