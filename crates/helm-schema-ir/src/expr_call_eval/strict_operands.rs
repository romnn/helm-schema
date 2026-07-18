use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use helm_schema_core::Predicate;

use super::serialization::record_total_conversion_effects;
use super::value_facts::{identity_range_key_paths, identity_value_paths};
use helm_schema_ast::{
    is_total_stringification_function, strict_parser_operand_pattern, string_operand_indices,
};

pub(super) fn record_string_transform_effects(
    function: &str,
    value: &Option<AbstractValue>,
    string_paths: &BTreeSet<String>,
    raw_range_key_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    let influence_paths = value.as_ref().map(AbstractValue::paths).unwrap_or_default();
    if is_total_stringification_function(function) {
        // Sprig's `strval` fallback renders ANY input (maps, lists, nil), so
        // a total stringification constrains nothing about its input and the
        // sink observes only the rendered text, never the input shape.
        record_total_conversion_effects(influence_paths, effects);
        effects
            .derived_range_key_paths
            .extend(identity_range_key_paths(value));
        return;
    }
    record_string_consumer_effects(string_paths, effects);
    record_raw_range_key_string_consumer_paths(raw_range_key_paths, effects);
    effects
        .derived_text_paths
        .extend(influence_paths.iter().cloned());
    effects
        .derived_range_key_paths
        .extend(identity_range_key_paths(value));
    if function == "b64enc" {
        effects.add_encoded_paths(influence_paths);
    }
}

pub(super) fn string_call_operand_facts(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut paths = BTreeSet::new();
    let mut range_key_paths = BTreeSet::new();
    for index in string_operand_indices(function, args.len()) {
        let Some(arg) = args.get(index) else {
            continue;
        };
        let result = eval_expr_with_helper_calls(arg, env, resolver);
        paths.extend(identity_value_paths(&result.value));
        let keys = identity_range_key_paths(&result.value);
        range_key_paths.extend(
            keys.difference(&result.effects.derived_range_key_paths)
                .cloned(),
        );
    }
    (paths, range_key_paths)
}

pub(super) fn pipeline_string_operand_facts(
    function: &str,
    args: &[TemplateExpr],
    piped_value: &Option<AbstractValue>,
    piped_effects: &Effects,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut paths = BTreeSet::new();
    let mut range_key_paths = BTreeSet::new();
    for index in string_operand_indices(function, args.len() + 1) {
        if index == args.len() {
            paths.extend(identity_value_paths(piped_value));
            let keys = identity_range_key_paths(piped_value);
            range_key_paths.extend(
                keys.difference(&piped_effects.derived_range_key_paths)
                    .cloned(),
            );
        } else if let Some(arg) = args.get(index) {
            let result = eval_expr_with_helper_calls(arg, env, resolver);
            paths.extend(identity_value_paths(&result.value));
            let keys = identity_range_key_paths(&result.value);
            range_key_paths.extend(
                keys.difference(&result.effects.derived_range_key_paths)
                    .cloned(),
            );
        }
    }
    (paths, range_key_paths)
}

pub(super) fn record_string_call_consumers(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let (paths, raw_range_key_paths) = string_call_operand_facts(function, args, env, resolver);
    record_string_consumer_effects(&paths, effects);
    record_raw_range_key_string_consumer_paths(&raw_range_key_paths, effects);
}

pub(super) fn record_strict_parser_call(
    function: &str,
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let Some((index, pattern)) = strict_parser_operand_pattern(function, args.len()) else {
        return;
    };
    let Some(arg) = args.get(index) else {
        return;
    };
    let operand = eval_expr_with_helper_calls(arg, env, resolver);
    let total_string_preimage = function == "mustDateModify" && is_to_string_expression(arg);
    if parser_operand_has_partitioned_identity(&operand, total_string_preimage) {
        record_strict_parser_result(&operand, pattern, total_string_preimage, effects);
    }
}

pub(super) fn record_strict_parser_pipeline(
    function: &str,
    args: &[TemplateExpr],
    piped: &EvalResult,
    piped_is_direct_values_path: bool,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let Some((index, pattern)) = strict_parser_operand_pattern(function, args.len() + 1) else {
        return;
    };
    if index == args.len() {
        if piped_is_direct_values_path || parser_operand_has_partitioned_identity(piped, false) {
            record_strict_parser_result(piped, pattern, false, effects);
        }
    } else if let Some(arg) = args.get(index) {
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        let total_string_preimage = function == "mustDateModify" && is_to_string_expression(arg);
        if parser_operand_has_partitioned_identity(&operand, total_string_preimage) {
            record_strict_parser_result(&operand, pattern, total_string_preimage, effects);
        }
    }
}

fn is_to_string_expression(expr: &TemplateExpr) -> bool {
    match expr.deparen() {
        TemplateExpr::Call { function, args } => function == "toString" && args.len() == 1,
        TemplateExpr::Pipeline(stages) => stages.last().is_some_and(|stage| {
            matches!(
                stage.deparen(),
                TemplateExpr::Call { function, args }
                    if function == "toString" && args.is_empty()
            )
        }),
        _ => false,
    }
}

fn parser_operand_has_partitioned_identity(
    operand: &EvalResult,
    total_string_preimage: bool,
) -> bool {
    let paths = parser_operand_identity_paths(operand, total_string_preimage);
    paths.len() == 1
        || (!paths.is_empty()
            && paths.iter().all(|path| {
                operand.effects.defaults.contains(path)
                    || operand.effects.local_default_paths.contains(path)
                    || operand
                        .effects
                        .local_output_meta
                        .get(path)
                        .is_some_and(|meta| !meta.predicates.is_empty())
                    || parser_output_metas(&operand.value, path)
                        .iter()
                        .any(|meta| !meta.predicates.is_empty() || meta.defaulted)
            }))
}

fn parser_operand_identity_paths(
    operand: &EvalResult,
    total_string_preimage: bool,
) -> BTreeSet<String> {
    fn collect(
        value: &AbstractValue,
        effects: &Effects,
        total_string_preimage: bool,
        paths: &mut BTreeSet<String>,
    ) {
        match value {
            AbstractValue::ValuesPath(path) | AbstractValue::JsonDecodedPath(path) => {
                if total_string_preimage
                    || (!effects.shape_erased_paths.contains(path)
                        && !effects.derived_text_paths.contains(path))
                {
                    paths.insert(path.clone());
                }
            }
            AbstractValue::OutputPath(path, meta) => {
                if !meta.shape_erased
                    && !meta.derived_text
                    && !meta.yaml_serialized
                    && !meta.json_serialized
                {
                    paths.insert(path.clone());
                }
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, effects, total_string_preimage, paths);
                }
            }
            AbstractValue::MergedLayers(layers) => {
                for layer in layers {
                    collect(layer, effects, total_string_preimage, paths);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::KeysList(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::Dict(_)
            | AbstractValue::List(_)
            | AbstractValue::Overlay { .. }
            | AbstractValue::SplitList { .. }
            | AbstractValue::SplitSegment { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut paths = BTreeSet::new();
    if let Some(value) = &operand.value {
        collect(value, &operand.effects, total_string_preimage, &mut paths);
    }
    paths
}

fn record_strict_parser_result(
    operand: &EvalResult,
    pattern: &str,
    total_string_preimage: bool,
    effects: &mut Effects,
) {
    for path in parser_operand_identity_paths(operand, total_string_preimage) {
        // Escape tokens recorded on the operand's metas exempt raw strings
        // a replace/split-prefix chain transformed before parsing.
        let escapes: BTreeSet<String> = parser_output_metas(&operand.value, &path)
            .iter()
            .flat_map(|meta| meta.lexical_escapes.iter().cloned())
            .collect();
        let pattern = crate::helper_meta::pattern_with_lexical_escapes(pattern, &escapes);
        for conjunction in parser_operand_selection_conjunctions(operand, &path) {
            push_value_pattern_capture(conjunction, path.clone(), pattern.clone(), false, effects);
        }
    }
}

fn parser_operand_selection_conjunctions(operand: &EvalResult, path: &str) -> Vec<Vec<Predicate>> {
    let base = operand_selection_conjunctions(&operand.effects, path);
    let metas = parser_output_metas(&operand.value, path);
    if metas.is_empty() {
        return base;
    }

    let mut out = Vec::new();
    for shared in base {
        for meta in &metas {
            let branches = if meta.predicates.is_empty() {
                vec![BTreeSet::new()]
            } else {
                meta.predicates.iter().cloned().collect()
            };
            for branch in branches {
                let mut conjunction = shared.clone();
                conjunction.extend(branch);
                if meta.defaulted {
                    conjunction.push(Predicate::truthy_path(path.to_string()));
                }
                // A sibling branch reassigned this binding away from the
                // raw path: the parser observes the raw value only
                // where those reassignments did not run.
                conjunction.extend(meta.capture_exclusions.iter().cloned());
                conjunction.sort();
                conjunction.dedup();
                out.push(conjunction);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn parser_output_metas(
    value: &Option<AbstractValue>,
    path: &str,
) -> Vec<crate::helper_meta::HelperOutputMeta> {
    fn collect(
        value: &AbstractValue,
        path: &str,
        metas: &mut Vec<crate::helper_meta::HelperOutputMeta>,
    ) {
        match value {
            AbstractValue::OutputPath(candidate, meta) if candidate == path => {
                if !metas.contains(meta) {
                    metas.push(meta.clone());
                }
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, path, metas);
                }
            }
            AbstractValue::MergedLayers(layers) => {
                for layer in layers {
                    collect(layer, path, metas);
                }
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::ValuesPath(_)
            | AbstractValue::JsonDecodedPath(_)
            | AbstractValue::RangeKey(_)
            | AbstractValue::KeysList(_)
            | AbstractValue::OutputPath(_, _)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::Dict(_)
            | AbstractValue::List(_)
            | AbstractValue::Overlay { .. }
            | AbstractValue::SplitList { .. }
            | AbstractValue::SplitSegment { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    let mut metas = Vec::new();
    if let Some(value) = value {
        collect(value, path, &mut metas);
    }
    metas
}

/// Record that an expression stage consumes the RAW value of `paths` as a
/// Go string, failing rendering otherwise. A path that already passed a
/// converting stage (`printf … | trunc`) or flows out of a shape-erasing
/// local binding reaches the consumer as derived text, so the earlier
/// conversion owns the contract. A path behind an ordered value selector is
/// consumed only on its selected arm, so its contract is captured as a
/// conditional fail-class implication instead of an unconditional row
/// contract.
pub(super) fn record_string_consumer_effects(paths: &BTreeSet<String>, effects: &mut Effects) {
    for path in paths {
        if effects.derived_text_paths.contains(path)
            || effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| meta.shape_erased || meta.derived_text)
        {
            continue;
        }
        let has_selection_condition = effects.defaults.contains(path)
            || effects.local_default_paths.contains(path)
            || effects
                .local_output_meta
                .get(path)
                .is_some_and(|meta| !meta.predicates.is_empty());
        if has_selection_condition {
            for conjunction in operand_selection_conjunctions(effects, path) {
                push_value_type_capture(conjunction, path.clone(), "string".to_string(), effects);
            }
        } else {
            effects.string_contract_paths.insert(path.clone());
            effects.direct_string_consumer_paths.insert(path.clone());
        }
    }
}

pub(super) fn record_range_key_string_consumer_effects(
    value: &Option<AbstractValue>,
    effects: &mut Effects,
) {
    let paths = identity_range_key_paths(value);
    let raw_paths = paths
        .difference(&effects.derived_range_key_paths)
        .cloned()
        .collect::<BTreeSet<_>>();
    record_raw_range_key_string_consumer_paths(&raw_paths, effects);
    effects.derived_range_key_paths.extend(paths);
}

pub(super) fn record_raw_range_key_string_consumer_paths(
    raw_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    if !raw_paths.is_empty() {
        let capture = crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::RangeKeyStrings {
                paths: raw_paths.clone(),
            },
        };
        if !effects.helper_fails.contains(&capture) {
            effects.helper_fails.push(capture);
        }
    }
    effects
        .derived_range_key_paths
        .extend(raw_paths.iter().cloned());
}

/// Records the runtime operand contract of a strict collection function.
///
/// The call itself does not skip Helm-empty values. Only a `default` or `coalesce` selection
/// makes a raw source conditional on truthiness; structural `if`/`with` guards join later when
/// the effects are absorbed at the execution site.
pub(super) fn record_strict_kind_operands(
    args: &[TemplateExpr],
    schema_type: &str,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        record_strict_kind_result(&operand, schema_type, effects);
    }
}

pub(super) fn record_strict_kind_result(
    operand: &EvalResult,
    schema_type: &str,
    effects: &mut Effects,
) {
    for path in strict_operand_identity_paths(operand) {
        for conjunction in strict_operand_selection_conjunctions(operand, &path) {
            push_value_type_capture(conjunction, path.clone(), schema_type.to_string(), effects);
        }
    }
}

/// Records a comparison operand's kind: Go's `eq`/`ne` compare `nil`
/// against anything, so a missing or null operand renders while a present
/// value of a different basic kind aborts.
pub(super) fn record_comparable_kind_result(
    operand: &EvalResult,
    schema_type: &str,
    effects: &mut Effects,
) {
    for path in strict_operand_identity_paths(operand) {
        for conjunction in strict_operand_selection_conjunctions(operand, &path) {
            let capture = crate::eval_effect::FailCapture {
                conjunction,
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::ComparableKind {
                    path: path.clone(),
                    schema_type: schema_type.to_string(),
                },
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
}

pub(super) fn record_collection_item_kind_result(
    operand: &EvalResult,
    schema_type: &str,
    pattern: Option<&str>,
    effects: &mut Effects,
) {
    let mut collection_paths = BTreeSet::new();
    let mut individual_paths = BTreeSet::new();

    fn collect(
        value: &AbstractValue,
        collection_paths: &mut BTreeSet<String>,
        individual_paths: &mut BTreeSet<String>,
        direct_collection: bool,
    ) {
        match value {
            AbstractValue::ValuesPath(path)
            | AbstractValue::JsonDecodedPath(path)
            | AbstractValue::OutputPath(path, _) => {
                if direct_collection {
                    collection_paths.insert(path.clone());
                } else if let Some(parent) = path.strip_suffix(".*") {
                    collection_paths.insert(parent.to_string());
                } else {
                    individual_paths.insert(path.clone());
                }
            }
            AbstractValue::List(items) => {
                for item in items {
                    collect(item, collection_paths, individual_paths, false);
                }
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(
                        choice,
                        collection_paths,
                        individual_paths,
                        direct_collection,
                    );
                }
            }
            AbstractValue::MergedLayers(layers) => {
                for layer in layers {
                    collect(layer, collection_paths, individual_paths, direct_collection);
                }
            }
            AbstractValue::Overlay { entries, fallback } => {
                for item in entries.values() {
                    collect(item, collection_paths, individual_paths, false);
                }
                collect(
                    fallback,
                    collection_paths,
                    individual_paths,
                    direct_collection,
                );
            }
            AbstractValue::Top
            | AbstractValue::Unknown
            | AbstractValue::RangeKey(_)
            | AbstractValue::KeysList(_)
            | AbstractValue::RootContext
            | AbstractValue::StringSet(_)
            | AbstractValue::DerivedBoolean(_)
            | AbstractValue::Dict(_)
            | AbstractValue::SplitList { .. }
            | AbstractValue::SplitSegment { .. }
            | AbstractValue::Widened(_) => {}
        }
    }

    if let Some(value) = &operand.value {
        collect(value, &mut collection_paths, &mut individual_paths, true);
    }
    for path in collection_paths {
        for conjunction in strict_operand_selection_conjunctions(operand, &path) {
            let capture = crate::eval_effect::FailCapture {
                conjunction,
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::CollectionItems {
                    paths: BTreeSet::from([path.clone()]),
                    schema_type: schema_type.to_string(),
                    pattern: pattern.map(str::to_string),
                },
            };
            if !effects.helper_fails.contains(&capture) {
                effects.helper_fails.push(capture);
            }
        }
    }
    for path in individual_paths {
        for conjunction in strict_operand_selection_conjunctions(operand, &path) {
            if let Some(pattern) = pattern {
                push_value_pattern_capture(
                    conjunction.clone(),
                    path.clone(),
                    pattern.to_string(),
                    false,
                    effects,
                );
            }
            push_value_type_capture(conjunction, path.clone(), schema_type.to_string(), effects);
        }
    }
}

pub(super) fn record_forbidden_kind(
    path: &str,
    schema_type: &str,
    mut conjunction: Vec<Predicate>,
    effects: &mut Effects,
) {
    conjunction.push(Predicate::from(crate::Guard::TypeIs {
        path: path.to_string(),
        schema_type: schema_type.to_string(),
    }));
    push_fail_capture(conjunction, effects);
}

pub(super) fn push_fail_capture(conjunction: Vec<Predicate>, effects: &mut Effects) {
    let capture = crate::eval_effect::FailCapture {
        conjunction,
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::Fail,
    };
    if !effects.helper_fails.contains(&capture) {
        effects.helper_fails.push(capture);
    }
}

pub(super) fn push_value_type_capture(
    conjunction: Vec<Predicate>,
    path: String,
    schema_type: String,
    effects: &mut Effects,
) {
    let capture = crate::eval_effect::FailCapture {
        conjunction,
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::ValueType { path, schema_type },
    };
    if !effects.helper_fails.contains(&capture) {
        effects.helper_fails.push(capture);
    }
}

fn push_value_pattern_capture(
    conjunction: Vec<Predicate>,
    path: String,
    pattern: String,
    templated: bool,
    effects: &mut Effects,
) {
    let capture = crate::eval_effect::FailCapture {
        conjunction,
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::ValuePattern {
            path,
            pattern,
            templated,
        },
    };
    if !effects.helper_fails.contains(&capture) {
        effects.helper_fails.push(capture);
    }
}

pub(super) fn strict_operand_identity_paths(operand: &EvalResult) -> BTreeSet<String> {
    identity_value_paths(&operand.value)
        .into_iter()
        .filter(|path| {
            !operand.effects.shape_erased_paths.contains(path)
                && !operand.effects.derived_text_paths.contains(path)
                && !operand
                    .effects
                    .local_output_meta
                    .get(path)
                    .is_some_and(|meta| meta.shape_erased || meta.derived_text)
        })
        .collect()
}

pub(super) fn strict_operand_selection_conjunctions(
    operand: &EvalResult,
    path: &str,
) -> Vec<Vec<Predicate>> {
    operand_selection_conjunctions(&operand.effects, path)
}

pub(super) fn operand_selection_conjunctions(effects: &Effects, path: &str) -> Vec<Vec<Predicate>> {
    let mut shared = BTreeSet::new();
    if effects.defaults.contains(path) || effects.local_default_paths.contains(path) {
        shared.insert(Predicate::truthy_path(path));
    }
    let Some(meta) = effects.local_output_meta.get(path) else {
        return vec![shared.into_iter().collect()];
    };
    if meta.predicates.is_empty() {
        return vec![shared.into_iter().collect()];
    }
    meta.predicates
        .iter()
        .map(|branch| {
            let mut conjunction = shared.clone();
            conjunction.extend(branch.iter().cloned());
            conjunction.into_iter().collect()
        })
        .collect()
}

/// `len` requires a length-bearing value (string, list, or map): numeric
/// and boolean operands abort rendering outright.
pub(super) fn record_length_bearing_operand(
    args: &[TemplateExpr],
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        record_length_bearing_result(&operand, effects);
    }
}

pub(super) fn record_length_bearing_result(operand: &EvalResult, effects: &mut Effects) {
    for path in strict_operand_identity_paths(operand) {
        for kind in ["boolean", "integer", "number"] {
            for conjunction in strict_operand_selection_conjunctions(operand, &path) {
                record_forbidden_kind(&path, kind, conjunction, effects);
            }
        }
    }
}
