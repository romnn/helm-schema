use std::collections::BTreeSet;

use helm_schema_ast::TemplateExpr;

use crate::abstract_value::AbstractValue;
use crate::eval_effect::{Effects, EvalResult};
use crate::eval_env::EvalEnv;
use crate::expr_eval::{HelperCallValueResolver, eval_expr_with_helper_calls};
use crate::scalar_value::ScalarValue;
use helm_schema_core::{Predicate, ValuesPath};

use super::serialization::record_total_conversion_effects;
use super::value_facts::{identity_range_key_paths, identity_value_paths};
use crate::function_semantics::{function_semantics, strict_parser_operand_pattern};

pub(super) fn record_string_transform_effects(
    function: &str,
    value: Option<&AbstractValue>,
    string_paths: &BTreeSet<String>,
    raw_range_key_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    let influence_paths = value.map(AbstractValue::paths).unwrap_or_default();
    if function_semantics(function).is_total_stringification() {
        // Sprig's `strval` fallback renders ANY input (maps, lists, nil), so
        // a total stringification constrains nothing about its input and the
        // sink observes only the rendered text, never the input shape.
        record_total_conversion_effects(
            influence_paths.iter().map(ValuesPath::encode).collect(),
            effects,
        );
        // Sprig `quote`/`squote` SKIP nil operands entirely: a missing or
        // null source renders an explicit YAML null into the sink, unlike
        // `toString`'s always-text image.
        if matches!(function, "quote" | "squote") {
            effects.nil_omitting_paths.extend(influence_paths.clone());
        }
        // Only `toString` returns the exact `%v` rendering of its operand
        // (`quote`/`squote`/`urlquery` decorate or rewrite the text), and
        // only a pure identity operand pins that image to a path: a derived
        // operand stringifies its derivation, not the raw value. Equality
        // decoding projects literals back through this image (cilium's
        // `toString .Values.kubeProxyReplacement` chain).
        if function == "toString"
            && let Some(AbstractValue::ValuesPath(path)) = value
        {
            effects.stringified_paths.insert(path.clone());
        }
        effects.derived_range_key_paths.extend(
            identity_range_key_paths(value)
                .iter()
                .map(|path| helm_schema_core::ValuesPath::parse(path)),
        );
        return;
    }
    record_string_consumer_effects(value, string_paths, effects);
    record_raw_range_key_string_consumer_paths(raw_range_key_paths, effects);
    if matches!(function, "lower" | "upper") {
        for path in string_paths {
            let typed_path = helm_schema_core::ValuesPath::parse(path);
            let selected = effects.defaults.contains(&typed_path)
                || effects.local_default_paths.contains(&typed_path)
                || effects
                    .local_output_meta
                    .get(&typed_path)
                    .is_some_and(|meta| meta.defaulted || !meta.predicates.is_empty());
            if !selected && !effects.derived_text_paths.contains(&typed_path) {
                effects.plain_text_preserving_paths.insert(typed_path);
            }
        }
    }
    effects.derived_text_paths.extend(influence_paths.clone());
    effects.derived_range_key_paths.extend(
        identity_range_key_paths(value)
            .iter()
            .map(|path| helm_schema_core::ValuesPath::parse(path)),
    );
    if function == "b64enc" {
        effects.add_encoded_paths(influence_paths.iter().map(ValuesPath::encode).collect());
    }
}

pub(super) fn string_invocation_operand_facts(
    function: &str,
    args: &[TemplateExpr],
    piped: Option<&EvalResult>,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut paths = BTreeSet::new();
    let mut range_key_paths = BTreeSet::new();
    for index in function_semantics(function)
        .string_operand_indices(args.len() + usize::from(piped.is_some()))
    {
        if index == args.len() {
            if let Some(piped) = piped {
                paths.extend(identity_value_paths(piped.value.as_ref()));
                let keys = identity_range_key_paths(piped.value.as_ref());
                range_key_paths.extend(keys.into_iter().filter(|path| {
                    !piped
                        .effects
                        .derived_range_key_paths
                        .contains(&helm_schema_core::ValuesPath::parse(path))
                }));
            }
        } else if let Some(arg) = args.get(index) {
            let result = eval_expr_with_helper_calls(arg, env, resolver);
            paths.extend(identity_value_paths(result.value.as_ref()));
            let keys = identity_range_key_paths(result.value.as_ref());
            range_key_paths.extend(keys.into_iter().filter(|path| {
                !result
                    .effects
                    .derived_range_key_paths
                    .contains(&helm_schema_core::ValuesPath::parse(path))
            }));
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
    let mut raw_range_key_paths = BTreeSet::new();
    for index in function_semantics(function).string_operand_indices(args.len()) {
        let Some(arg) = args.get(index) else {
            continue;
        };
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        let paths = identity_value_paths(operand.value.as_ref());
        record_string_consumer_effects(operand.value.as_ref(), &paths, effects);
        let keys = identity_range_key_paths(operand.value.as_ref());
        raw_range_key_paths.extend(keys.into_iter().filter(|path| {
            !operand
                .effects
                .derived_range_key_paths
                .contains(&helm_schema_core::ValuesPath::parse(path))
        }));
    }
    record_raw_range_key_string_consumer_paths(&raw_range_key_paths, effects);
}

pub(super) fn record_strict_parser_invocation(
    function: &str,
    args: &[TemplateExpr],
    piped: Option<(&EvalResult, bool)>,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    let Some((index, pattern)) = strict_parser_operand_pattern(
        function_semantics(function),
        args.len() + usize::from(piped.is_some()),
    ) else {
        return;
    };
    if index == args.len() {
        let Some((piped, piped_is_direct_values_path)) = piped else {
            return;
        };
        if piped_is_direct_values_path || parser_operand_has_partitioned_identity(piped, false) {
            record_strict_parser_result(piped, pattern, false, effects);
        }
        return;
    }
    let Some(arg) = args.get(index) else {
        return;
    };
    let operand = eval_expr_with_helper_calls(arg, env, resolver);
    let total_string_preimage = function == "mustDateModify" && is_to_string_expression(arg);
    if parser_operand_has_partitioned_identity(&operand, total_string_preimage) {
        record_strict_parser_result(&operand, pattern, total_string_preimage, effects);
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
                let typed_path = helm_schema_core::ValuesPath::parse(path);
                operand.effects.defaults.contains(&typed_path)
                    || operand.effects.local_default_paths.contains(&typed_path)
                    || operand
                        .effects
                        .local_output_meta
                        .get(&typed_path)
                        .is_some_and(|meta| !meta.predicates.is_empty())
                    || parser_output_metas(operand.value.as_ref(), path)
                        .iter()
                        .any(|meta| {
                            !meta.predicates.is_empty()
                                || !meta.capture_exclusions.is_empty()
                                || meta.defaulted
                        })
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
            AbstractValue::ValuesPath(path) => {
                let encoded = path.encode();
                if total_string_preimage
                    || (!effects.observed_facts.shape_erased_paths.contains(path)
                        && !effects.derived_text_paths.contains(path))
                {
                    paths.insert(encoded);
                }
            }
            AbstractValue::JsonDecodedPath(path) => {
                if total_string_preimage
                    || (!effects.observed_facts.shape_erased_paths.contains(path)
                        && !effects.derived_text_paths.contains(path))
                {
                    paths.insert(path.encode());
                }
            }
            AbstractValue::OutputPath(path, meta) => {
                // A `stringified` arm is the exact `%v` rendering of the
                // path — the identity on raw strings, while a lexical
                // pattern is vacuous on non-string instances — so the
                // parser identity survives the derivation flags the
                // stringification itself set (the datadog empty-tag
                // fallback wraps the raw arm in exclusion meta before its
                // `toString` reassignment).
                if meta.stringified
                    || (!meta.shape_erased
                        && !meta.derived_text
                        && !meta.yaml_serialized
                        && !meta.json_serialized)
                {
                    paths.insert(path.encode());
                }
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, effects, total_string_preimage, paths);
                }
            }
            AbstractValue::FirstTruthy(candidates) => {
                for candidate in candidates {
                    collect(candidate, effects, total_string_preimage, paths);
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
        let escapes: BTreeSet<crate::helper_meta::LexicalEscape> =
            parser_output_metas(operand.value.as_ref(), &path)
                .iter()
                .flat_map(|meta| meta.lexical_escapes.iter().cloned())
                .collect();
        let pattern = crate::helper_meta::pattern_with_lexical_escapes(pattern, &escapes);
        for conjunction in parser_operand_selection_conjunctions(operand, &path) {
            push_value_pattern_capture(conjunction, &path, pattern.clone(), false, effects);
        }
    }
}

fn parser_operand_selection_conjunctions(
    operand: &EvalResult,
    path: &str,
) -> Vec<helm_schema_core::Conjunction> {
    let base = operand_selection_conjunctions(&operand.effects, path);
    let metas = parser_output_metas(operand.value.as_ref(), path);
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
                out.push(conjunction);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn parser_output_metas(
    value: Option<&AbstractValue>,
    path: &str,
) -> Vec<crate::helper_meta::HelperOutputMeta> {
    fn collect(
        value: &AbstractValue,
        path: &str,
        metas: &mut Vec<crate::helper_meta::HelperOutputMeta>,
    ) {
        match value {
            AbstractValue::OutputPath(candidate, meta) if candidate.encode() == path => {
                if !metas.contains(meta) {
                    metas.push(meta.clone());
                }
            }
            AbstractValue::Choice(choices) => {
                for choice in choices {
                    collect(choice, path, metas);
                }
            }
            AbstractValue::FirstTruthy(candidates) => {
                for candidate in candidates {
                    collect(candidate, path, metas);
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
pub(super) fn record_string_consumer_effects(
    value: Option<&AbstractValue>,
    paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    for path in paths {
        let direct_identity = matches!(value, Some(AbstractValue::ValuesPath(identity)) if identity.encode() == *path);
        let requirements = string_operand_requirements(value, effects, path);
        for (route, conjunction) in requirements {
            let capture = crate::eval_effect::FailCapture {
                conjunction: Vec::new().into(),
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::StringRequirement {
                    path: helm_schema_core::ValuesPath::parse(path),
                    route,
                    selection: conjunction.clone(),
                },
            };
            effects.observed_facts.captures.insert(capture);
            if direct_identity {
                let capture = crate::eval_effect::FailCapture {
                    conjunction,
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::AbsenceAborts {
                        path: helm_schema_core::ValuesPath::parse(path),
                    },
                };
                effects.observed_facts.captures.insert(capture);
            }
        }
    }
}

fn string_operand_requirements(
    value: Option<&AbstractValue>,
    effects: &Effects,
    path: &str,
) -> Vec<(
    crate::eval_effect::StringRequirementRoute,
    helm_schema_core::Conjunction,
)> {
    let output_metas = parser_output_metas(value, path);
    let typed_path = helm_schema_core::ValuesPath::parse(path);
    let path_is_derived = effects.derived_text_paths.contains(&typed_path)
        || effects
            .local_output_meta
            .get(&typed_path)
            .is_some_and(|meta| meta.shape_erased || meta.derived_text);
    let mut requirements = if output_metas.is_empty() {
        if path_is_derived {
            Vec::new()
        } else {
            let conjunctions = operand_selection_conjunctions(effects, path);
            let exact_identity = match value {
                Some(
                    AbstractValue::ValuesPath(candidate)
                    | AbstractValue::JsonDecodedPath(candidate),
                ) => candidate.encode() == path,
                _ => false,
            };
            if !exact_identity
                && conjunctions
                    .iter()
                    .all(|conjunction| conjunction.is_empty())
            {
                return Vec::new();
            }
            conjunctions
                .into_iter()
                .map(|conjunction| {
                    let route = if conjunction.is_empty() {
                        crate::eval_effect::StringRequirementRoute::Direct
                    } else {
                        crate::eval_effect::StringRequirementRoute::Selected
                    };
                    (route, conjunction)
                })
                .collect()
        }
    } else {
        output_metas
            .into_iter()
            .filter(|meta| {
                meta.yaml_serialized
                    || meta.json_serialized
                    || (!path_is_derived && !meta.shape_erased && !meta.derived_text)
            })
            .flat_map(|meta| {
                let branches = if meta.predicates.is_empty() {
                    vec![helm_schema_core::Conjunction::default()]
                } else {
                    meta.predicates
                        .iter()
                        .map(|branch| branch.iter().cloned().collect())
                        .collect()
                };
                branches.into_iter().map(move |branch| {
                    let route = if meta.yaml_serialized || meta.json_serialized {
                        crate::eval_effect::StringRequirementRoute::Serialized
                    } else if branch.is_empty() {
                        crate::eval_effect::StringRequirementRoute::Direct
                    } else {
                        crate::eval_effect::StringRequirementRoute::Selected
                    };
                    (route, branch)
                })
            })
            .collect()
    };
    requirements.sort();
    requirements.dedup();
    requirements
}

pub(super) fn record_range_key_string_consumer_effects(
    value: Option<&AbstractValue>,
    effects: &mut Effects,
) {
    let paths = identity_range_key_paths(value);
    let raw_paths = paths
        .iter()
        .filter(|path| {
            !effects
                .derived_range_key_paths
                .contains(&helm_schema_core::ValuesPath::parse(path))
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    record_raw_range_key_string_consumer_paths(&raw_paths, effects);
    effects.derived_range_key_paths.extend(
        paths
            .iter()
            .map(|path| helm_schema_core::ValuesPath::parse(path)),
    );
}

pub(super) fn record_raw_range_key_string_consumer_paths(
    raw_paths: &BTreeSet<String>,
    effects: &mut Effects,
) {
    if !raw_paths.is_empty() {
        let capture = crate::eval_effect::FailCapture {
            conjunction: Vec::new().into(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::RangeKeyStrings {
                paths: raw_paths
                    .iter()
                    .map(|path| helm_schema_core::ValuesPath::parse(path))
                    .collect(),
            },
        };
        effects.observed_facts.captures.insert(capture);
    }
    effects.derived_range_key_paths.extend(
        raw_paths
            .iter()
            .map(|path| helm_schema_core::ValuesPath::parse(path)),
    );
}

/// Records the runtime operand contract of a strict collection function.
///
/// The call itself does not skip Helm-empty values. Only a `default` or `coalesce` selection
/// makes a raw source conditional on truthiness; structural `if`/`with` guards join later when
/// the effects are absorbed at the execution site.
pub(super) fn record_strict_kind_operands(
    function: &str,
    args: &[TemplateExpr],
    schema_type: &str,
    env: &EvalEnv,
    resolver: &mut impl HelperCallValueResolver,
    effects: &mut Effects,
) {
    for arg in args {
        let operand = eval_expr_with_helper_calls(arg, env, resolver);
        // `direct_values_path` resolves against an EMPTY environment, so it
        // answers exactly the question the map-parameter class asks: a
        // spelling it names is a field read that hands the parameter a nil
        // interface, while a pipeline result, a call result, and a
        // `:=`-bound local all reach it invalid instead. A with-scoped dot
        // member does abort at runtime but cannot resolve here, so it
        // abstains.
        let nil_aborts = function_semantics(function)
            .nil_aborts(crate::expr_eval::direct_values_path(arg).is_some());
        record_strict_kind_result(&operand, schema_type, nil_aborts, effects);
    }
}

pub(super) fn record_strict_kind_result(
    operand: &EvalResult,
    schema_type: &str,
    nil_aborts: bool,
    effects: &mut Effects,
) {
    for (path, shadow) in layered_strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.extend(shadow.iter().cloned());
            push_value_type_capture(
                conjunction,
                &path,
                schema_type.to_string(),
                nil_aborts,
                effects,
            );
        }
    }
    if nil_aborts {
        record_operand_presence_result(operand, effects);
    }
}

/// Records that a NIL operand aborts the call wherever it executes: the
/// path must be present and non-null there.
///
/// The truthy⇒kind capture beside this one cannot state it — absence is
/// Helm-falsy, so that capture's own guard excuses exactly the state that
/// aborts. The function catalog's `nil_aborts` facet decides which positions
/// carry the claim.
pub(super) fn record_operand_presence_result(operand: &EvalResult, effects: &mut Effects) {
    // Only an operand that IS one raw values path carries the claim, the
    // same rule the string lane applies: a derived operand (a merge, a
    // `default` chain, a helper's rendered text) hands the call whatever the
    // derivation produced, so those paths' own absence is not what aborts. Reading
    // the layered identities instead fabricates subjects — k8s-infra's
    // preset merge grew 951 clauses over spellings like
    // `otelAgent.presets.hostMetrics.scrapers.service.pipelines`, which
    // name no key the chart ever has.
    let Some(AbstractValue::ValuesPath(path)) = &operand.value else {
        return;
    };
    // The whole values root is always present; only a member has an
    // absence to claim (`index $.Values $key`).
    if path.segments().len() == 0 {
        return;
    }
    let encoded_path = path.encode();
    for conjunction in strict_operand_selection_conjunctions(operand, &encoded_path) {
        let capture = crate::eval_effect::FailCapture {
            conjunction,
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::AbsenceAborts { path: path.clone() },
        };
        effects.observed_facts.captures.insert(capture);
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
    if let Some(dispatch) = &operand.scalar_dispatch {
        // Only identity arms consume the raw path. Literal and rendered arms
        // compare their produced value and therefore cannot type the source.
        for (condition, value) in &dispatch.arms {
            let ScalarValue::Identity(path) = value else {
                continue;
            };
            for mut conjunction in strict_operand_selection_conjunctions(operand, &path.encode()) {
                if condition != &Predicate::True {
                    conjunction.push(condition.clone());
                }
                let capture = crate::eval_effect::FailCapture {
                    conjunction,
                    ranged: crate::range_modes::RangeModes::default(),
                    kind: crate::eval_effect::CaptureKind::ComparableKind {
                        path: path.clone(),
                        schema_type: schema_type.to_string(),
                    },
                };
                effects.observed_facts.captures.insert(capture);
            }
        }
        return;
    }
    for (path, shadow) in layered_strict_operand_identity_paths(operand) {
        for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
            conjunction.extend(shadow.iter().cloned());
            let capture = crate::eval_effect::FailCapture {
                conjunction,
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::ComparableKind {
                    path: helm_schema_core::ValuesPath::parse(&path),
                    schema_type: schema_type.to_string(),
                },
            };
            effects.observed_facts.captures.insert(capture);
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic operation together makes its state transitions easier to audit"
)]
pub(super) fn record_collection_item_kind_result(
    operand: &EvalResult,
    schema_type: &str,
    pattern: Option<&str>,
    effects: &mut Effects,
) {
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
                    collection_paths.insert(path.encode());
                } else if let Some(parent) = path.item_parent() {
                    collection_paths.insert(parent.encode());
                } else {
                    individual_paths.insert(path.encode());
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
            AbstractValue::FirstTruthy(candidates) => {
                for candidate in candidates {
                    collect(
                        candidate,
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

    let mut collection_paths = BTreeSet::new();
    let mut individual_paths = BTreeSet::new();
    if let Some(value) = &operand.value {
        collect(value, &mut collection_paths, &mut individual_paths, true);
    }
    for path in collection_paths {
        for conjunction in strict_operand_selection_conjunctions(operand, &path) {
            let capture = crate::eval_effect::FailCapture {
                conjunction,
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::CollectionItems {
                    paths: BTreeSet::from([helm_schema_core::ValuesPath::parse(&path)]),
                    schema_type: schema_type.to_string(),
                    pattern: pattern.map(str::to_string),
                },
            };
            effects.observed_facts.captures.insert(capture);
        }
    }
    for path in individual_paths {
        for conjunction in strict_operand_selection_conjunctions(operand, &path) {
            if let Some(pattern) = pattern {
                push_value_pattern_capture(
                    conjunction.clone(),
                    &path,
                    pattern.to_string(),
                    false,
                    effects,
                );
            }
            push_value_type_capture(conjunction, &path, schema_type.to_string(), false, effects);
        }
    }
}

pub(super) fn record_forbidden_kind(
    path: &str,
    schema_type: &str,
    conjunction: impl Into<helm_schema_core::Conjunction>,
    effects: &mut Effects,
) {
    let mut conjunction = conjunction.into();
    conjunction.push(Predicate::from(crate::Guard::TypeIs {
        path: helm_schema_core::ValuesPath::parse(path),
        schema_type: schema_type.to_string(),
    }));
    push_fail_capture(conjunction, effects);
}

pub(super) fn push_fail_capture(
    conjunction: impl Into<helm_schema_core::Conjunction>,
    effects: &mut Effects,
) {
    let capture = crate::eval_effect::FailCapture {
        conjunction: conjunction.into(),
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::Fail,
    };
    effects.observed_facts.captures.insert(capture);
}

pub(super) fn push_value_type_capture(
    conjunction: impl Into<helm_schema_core::Conjunction>,
    path: &str,
    schema_type: String,
    null_aborts: bool,
    effects: &mut Effects,
) {
    let capture = crate::eval_effect::FailCapture {
        conjunction: conjunction.into(),
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::ValueType {
            path: helm_schema_core::ValuesPath::parse(path),
            schema_type,
            null_aborts,
        },
    };
    effects.observed_facts.captures.insert(capture);
}

fn push_value_pattern_capture(
    conjunction: impl Into<helm_schema_core::Conjunction>,
    path: &str,
    pattern: String,
    templated: bool,
    effects: &mut Effects,
) {
    let capture = crate::eval_effect::FailCapture {
        conjunction: conjunction.into(),
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::ValuePattern {
            path: helm_schema_core::ValuesPath::parse(path),
            pattern,
            templated,
        },
    };
    effects.observed_facts.captures.insert(capture);
}

fn strict_operand_path_is_clean(path: &str, effects: &Effects) -> bool {
    let typed_path = helm_schema_core::ValuesPath::parse(path);
    !effects
        .observed_facts
        .shape_erased_paths
        .contains(&typed_path)
        && !effects.derived_text_paths.contains(&typed_path)
        && !effects
            .local_output_meta
            .get(&typed_path)
            .is_some_and(|meta| meta.shape_erased || meta.derived_text)
}

/// Facts one merge layer contributes to the walk: the identity paths the
/// layer's runtime value can be, and whether EVERY alternative of the layer
/// resolves to such a path — only then does "all of them absent" prove the
/// layer cannot shadow the layers below it.
struct StrictLayerWalk {
    paths: BTreeSet<String>,
    absence_expressible: bool,
}

/// The strict-operand identities together with the merge-shadowing
/// conditions under which the operand's runtime value IS that identity.
///
/// A [`AbstractValue::MergedLayers`] operand reaches a strict consumer as
/// exactly one layer's member: the highest-precedence layer that supplies
/// it. The top layer's contract therefore holds whenever its path is
/// present, while a deeper layer's contract holds only where every earlier
/// layer's identity is ABSENT (which fires less often than the real
/// "earlier layer lacks the key" condition — the sound direction for fail
/// captures). A layer whose alternatives are not all path-backed (a
/// literal member, an unknown overwrite map) blocks every deeper layer
/// instead of letting deeper contracts fire unshadowed. Non-merged shapes
/// keep the flat per-path behavior.
///
/// Every layer's conditions carry the layer path's own TRUTHINESS: the
/// strict consumer eats the MERGED value, which exists whether or not any
/// one layer supplies the member, so a layer's contract must never demand
/// the layer path's presence (the airflow worker-set members). Scoping to
/// truthy layer values keeps the capture inside the states where the
/// layer demonstrably feeds the consumer.
#[expect(
    clippy::too_many_lines,
    reason = "the exhaustive raw, decoded, and rendered identity arms keep transform ownership reviewable"
)]
pub(super) fn layered_strict_operand_identity_paths(
    operand: &EvalResult,
) -> Vec<(String, Vec<Predicate>)> {
    fn collect(
        value: &AbstractValue,
        effects: &Effects,
        shadow: &[Predicate],
        emit: bool,
        layered: bool,
        out: &mut Vec<(String, Vec<Predicate>)>,
    ) -> StrictLayerWalk {
        match value {
            AbstractValue::ValuesPath(path) => {
                let path = path.encode();
                if emit && strict_operand_path_is_clean(&path, effects) {
                    let mut conditions = shadow.to_vec();
                    if layered {
                        conditions.push(Predicate::truthy_path(path.clone()));
                    }
                    let entry = (path.clone(), conditions);
                    if !out.contains(&entry) {
                        out.push(entry);
                    }
                }
                StrictLayerWalk {
                    paths: BTreeSet::from([path]),
                    absence_expressible: true,
                }
            }
            AbstractValue::JsonDecodedPath(path) | AbstractValue::OutputPath(path, _) => {
                let path = path.encode();
                if emit && strict_operand_path_is_clean(&path, effects) {
                    let mut conditions = shadow.to_vec();
                    if layered {
                        conditions.push(Predicate::truthy_path(path.clone()));
                    }
                    let entry = (path.clone(), conditions);
                    if !out.contains(&entry) {
                        out.push(entry);
                    }
                }
                StrictLayerWalk {
                    paths: BTreeSet::from([path.clone()]),
                    absence_expressible: true,
                }
            }
            AbstractValue::Choice(choices) => {
                let mut paths = BTreeSet::new();
                let mut absence_expressible = true;
                for choice in choices {
                    let walk = collect(choice, effects, shadow, emit, layered, out);
                    paths.extend(walk.paths);
                    absence_expressible &= walk.absence_expressible;
                }
                StrictLayerWalk {
                    paths,
                    absence_expressible,
                }
            }
            AbstractValue::FirstTruthy(candidates) => {
                let mut paths = BTreeSet::new();
                let mut absence_expressible = true;
                for candidate in candidates {
                    let walk = collect(candidate, effects, shadow, emit, layered, out);
                    paths.extend(walk.paths);
                    absence_expressible &= walk.absence_expressible;
                }
                StrictLayerWalk {
                    paths,
                    absence_expressible,
                }
            }
            AbstractValue::MergedLayers(layers) => {
                let mut shadow = shadow.to_vec();
                let mut paths = BTreeSet::new();
                let mut absence_expressible = true;
                let mut unshadowed = true;
                for layer in layers {
                    let walk = collect(layer, effects, &shadow, emit && unshadowed, true, out);
                    if walk.absence_expressible && !walk.paths.is_empty() {
                        for path in &walk.paths {
                            shadow.push(Predicate::from(crate::Guard::Absent {
                                path: helm_schema_core::ValuesPath::parse(path),
                            }));
                        }
                    } else {
                        unshadowed = false;
                    }
                    paths.extend(walk.paths);
                    absence_expressible &= walk.absence_expressible;
                }
                StrictLayerWalk {
                    paths,
                    absence_expressible,
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
            | AbstractValue::Widened(_) => StrictLayerWalk {
                paths: BTreeSet::new(),
                absence_expressible: false,
            },
        }
    }

    let mut out = Vec::new();
    if let Some(value) = &operand.value {
        collect(value, &operand.effects, &[], true, false, &mut out);
    }
    out
}

pub(super) fn strict_operand_selection_conjunctions(
    operand: &EvalResult,
    path: &str,
) -> Vec<helm_schema_core::Conjunction> {
    operand_selection_conjunctions(&operand.effects, path)
}

pub(super) fn operand_selection_conjunctions(
    effects: &Effects,
    path: &str,
) -> Vec<helm_schema_core::Conjunction> {
    let mut shared = BTreeSet::new();
    let typed_path = helm_schema_core::ValuesPath::parse(path);
    if effects.defaults.contains(&typed_path) || effects.local_default_paths.contains(&typed_path) {
        shared.insert(Predicate::truthy_path(path));
    }
    let Some(meta) = effects.local_output_meta.get(&typed_path) else {
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
/// and boolean operands abort rendering outright, and so does a nil one
/// ("len of nil pointer").
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
    for (path, shadow) in layered_strict_operand_identity_paths(operand) {
        for kind in ["boolean", "integer", "number"] {
            for mut conjunction in strict_operand_selection_conjunctions(operand, &path) {
                conjunction.extend(shadow.iter().cloned());
                record_forbidden_kind(&path, kind, conjunction, effects);
            }
        }
    }
    record_operand_presence_result(operand, effects);
}
