use std::collections::BTreeSet;

use helm_schema_ast::{Literal, TemplateExpr};

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::helper_meta::HelperOutputMeta;
use helm_schema_core::Predicate;

use super::eval_all_args;
use super::strict_operands::{
    layered_strict_operand_identity_paths, strict_operand_selection_conjunctions,
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
    // Subject-level claims hold only for a RAW identity subject: a
    // selection chain (`x | default dict`) reaches the dig with its
    // fallback exactly in the states the raw path is falsy or absent, so
    // claiming the raw path's presence or even-null type there would
    // reject documents that render through the fallback.
    let raw_subject = matches!(
        subject.value.as_ref(),
        Some(AbstractValue::ValuesPath(_) | AbstractValue::JsonDecodedPath(_))
    );
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
            let capture = if prefix_len == 0 {
                if !raw_subject {
                    continue;
                }
                // The SUBJECT is type-asserted before any missing-key
                // handling, so a present-but-null subject aborts too
                // (KPS's nulled `customRules`). The strict `HasKey`
                // conjunct self-scopes the claim — the dig-subject kind
                // keeps null rejected where a truthy-scoped arm would go
                // vacuous — while the companion presence capture below
                // covers the absent state (a missing subject reads as
                // nil and aborts the same assertion; loki's null-deleted
                // `storage_config`).
                let Some((parent, leaf)) = helm_schema_core::split_value_path(&step)
                    .split_last()
                    .map(|(leaf, parents)| (parents.join("."), leaf.clone()))
                else {
                    continue;
                };
                let presence = crate::eval_effect::FailCapture {
                    conjunction: Vec::new(),
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::RequiredPresence { path: step.clone() },
                };
                if !effects.helper_fails.contains(&presence) {
                    effects.helper_fails.push(presence);
                }
                crate::eval_effect::FailCapture {
                    conjunction: vec![Predicate::from(crate::Guard::HasKey {
                        path: parent,
                        key: leaf,
                    })],
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::DigSubject { path: step.clone() },
                }
            } else {
                // An INTERMEDIATE step falls back to the default when nil
                // (trivy-operator's nulled `trivy.resources` renders) but
                // aborts on any other non-map — including Helm-falsy
                // scalars — so the exact scope is present-and-non-null,
                // which is `Guard::Absent`'s negation.
                crate::eval_effect::FailCapture {
                    conjunction: vec![
                        Predicate::from(crate::Guard::Absent { path: step.clone() }).negated(),
                        Predicate::from(crate::Guard::TypeIs {
                            path: step.clone(),
                            schema_type: "object".to_string(),
                        })
                        .negated(),
                    ],
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::Fail,
                }
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
    let value = subject.value.and_then(|value| value.apply_to_path(&keys));
    // The dug leaf is a READ of that path whose absence falls back to the
    // literal default: an output path (so required-subject walking and
    // read rows see it) marked defaulted, exactly like `default`.
    for path in identity_value_paths(value.as_ref()) {
        effects.output_paths.insert(path.clone());
        effects.defaults.insert(path);
    }
    EvalResult::with_effects(value, effects)
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
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
    let mut effects = Effects::default();
    if object_host {
        record_member_host_access(&base, &mut effects);
    }
    effects.merge(base.effects);
    let Some(value) = base.value else {
        return EvalResult::with_effects(None, effects);
    };

    let mut values = vec![value];
    for arg in path_args {
        let arg_result = eval_expr_with_helper_calls(arg, env, resolver);
        effects.merge(arg_result.effects);
        let Some(options) = path_segment_options(arg, arg_result.value.as_ref()) else {
            return EvalResult::with_effects(None, effects);
        };
        // A variable key must not extend a whole-values-root identity that
        // rides one arm of a CHOICE subject: the join lost the correlation
        // between subject arms and key candidates (`index $lastMap
        // $lastKey` in nats' jsonpatch pairs the root arm with the other
        // arms' scaffolding keys), and a fabricated root member would mint
        // values properties the chart never reads. A single-valued subject
        // keeps the exact navigation — the split-path traversal reads
        // `index $.Values <segment>` per unrolled key, and helper-computed
        // key alternatives fan out over a NON-choice root exactly.
        let literal_key = matches!(
            arg.deparen(),
            TemplateExpr::Literal(Literal::String(_) | Literal::RawString(_) | Literal::Int(_))
        );
        let values_snapshot: Vec<AbstractValue> = if literal_key {
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
            for option in &options {
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
                            conjunction: Vec::new(),
                            ranged: crate::range_modes::RangeModes::default(),
                            kind: crate::eval_effect::CaptureKind::SplitIndexAccess {
                                paths: source_paths.clone(),
                                separator: separator.clone(),
                                index,
                                total_text_preimage: *total_text_preimage,
                            },
                        };
                        if !effects.helper_fails.contains(&capture) {
                            effects.helper_fails.push(capture);
                        }
                    }
                    for path in identity_value_paths(Some(value)) {
                        for conjunction in
                            super::strict_operands::operand_selection_conjunctions(&effects, &path)
                        {
                            let capture = crate::eval_effect::FailCapture {
                                conjunction,
                                ranged: crate::range_modes::RangeModes::default(),
                                kind: crate::eval_effect::CaptureKind::IndexAccess {
                                    path: path.clone(),
                                    index,
                                },
                            };
                            if !effects.helper_fails.contains(&capture) {
                                effects.helper_fails.push(capture);
                            }
                        }
                    }
                }
                if let Some(next) = apply_index_segment(value, option) {
                    for next_path in next.paths() {
                        for base_path in &base_paths {
                            if !base_path.is_empty()
                                && helm_schema_core::values_path_is_descendant(
                                    &next_path, base_path,
                                )
                            {
                                effects
                                    .local_output_meta
                                    .entry(next_path.clone())
                                    .or_insert_with(HelperOutputMeta::default)
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

    let value = AbstractValue::choice(values);
    EvalResult::with_effects(value, effects)
}

pub(super) fn record_member_host_access(operand: &EvalResult, effects: &mut Effects) {
    for (path, shadow) in layered_strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.extend(shadow.iter().cloned());
            conjunction.push(
                Predicate::from(crate::Guard::TypeIs {
                    path: path.clone(),
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
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PathSegmentOption {
    segments: Vec<String>,
    integer_index: bool,
}

/// Replace whole-values-root identity arms with opaque present values, so
/// a variable-key navigation cannot fabricate root member paths; container
/// structure and non-root identities stay intact.
fn without_values_root_identity(value: &AbstractValue) -> AbstractValue {
    match value {
        AbstractValue::ValuesPath(path)
        | AbstractValue::JsonDecodedPath(path)
        | AbstractValue::OutputPath(path, _)
            if path.is_empty() =>
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
            AbstractValue::MergedLayers(layers.iter().map(without_values_root_identity).collect())
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
        _ => value.apply_to_path(&["*".to_string()]),
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
