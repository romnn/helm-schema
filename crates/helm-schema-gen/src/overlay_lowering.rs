use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, GuardValue,
    ResourceSchemaOracle,
};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::condition_encoding::{
    build_condition_clauses, evaluate_guard_set_on_values, guard_encodes_fully,
};
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::conditional_target_schema;
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::values_yaml::yaml_value_at_path;
use crate::{common_prefix_len, split_value_path};

pub(crate) struct ConditionalResolvedSchema {
    pub(crate) target_value_path: String,
    ancestor_segments: Vec<String>,
    relative_target_segments: Vec<String>,
    guards: Vec<ConditionalGuard>,
    pub(crate) target_schema: Value,
    pub(crate) provider_schema_candidate: Option<ProviderSchemaCandidate>,
    pub(crate) preserve_base_schema: bool,
    fold_unconditional_object_host_into_base: bool,
    /// The conditional is a pure `allOf` arm (fail implication): it adds a
    /// requirement without owning the path's shape, so base classification
    /// must ignore it entirely — an implication must never flip an
    /// overlay-owned base to the resolved schema nor empty a resolved one.
    pub(crate) arm_only: bool,
}

#[tracing::instrument(skip_all)]
pub(crate) fn collect_conditional_schemas(
    resolved_paths: &[ResolvedPathSchema],
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
) -> Vec<ConditionalResolvedSchema> {
    let mut synthesized_implications =
        crate::required_source_backprojection::synthesized_required_source_implications(
            contract_schema_signals,
            provider,
        );
    for (path, split_implications) in
        crate::required_source_backprojection::synthesized_split_segment_implications(
            contract_schema_signals,
            provider,
        )
    {
        let entries = synthesized_implications.entry(path).or_default();
        for implication in split_implications {
            if !entries.contains(&implication) {
                entries.push(implication);
            }
        }
    }
    let resolved_by_path = resolved_paths
        .iter()
        .map(|resolved| (resolved.value_path.as_str(), resolved))
        .collect::<BTreeMap<_, _>>();
    // Member-arm grafting looks up the resolved descendants under `<target>.*`
    // per Members implication; index them by the segments before the first
    // `*` once instead of rescanning every resolved path per implication.
    let mut member_descendants: BTreeMap<&[String], Vec<&ResolvedPathSchema>> = BTreeMap::new();
    for resolved in resolved_paths {
        if let Some(star) = resolved
            .path_segments
            .iter()
            .position(|segment| segment == "*")
        {
            member_descendants
                .entry(&resolved.path_segments[..star])
                .or_default()
                .push(resolved);
        }
    }
    let mut conditionals = Vec::new();

    for (target_value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let Some(resolved_target) = resolved_by_path.get(target_value_path.as_str()) else {
            continue;
        };
        let has_unconditional_self_presence_contract = evidence
            .conditional_overlays
            .iter()
            .any(|overlay| is_unconditional_self_presence_overlay(target_value_path, overlay));

        // `fail` implications: wherever the outer guards hold, the failing
        // test's negation must hold. Runtime-hard, so the requirement
        // rides an `allOf` arm — property-level union lanes (declared
        // defaults, range alternatives, carrier variants) must never
        // bypass it. An empty guard set means the requirement is
        // unconditional and the arm's condition is trivially true.
        let synthesized = synthesized_implications
            .get(target_value_path)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for implication in evidence.fail_implications.iter().chain(synthesized) {
            if is_bare_iterable_implication(implication)
                && member_implication_covers_range_domain(
                    &evidence.fail_implications,
                    &implication.outer_guards,
                )
            {
                continue;
            }
            // Member-access requirements already enforced by the resolved
            // base need no duplicate arm unless another use widens that base.
            // A mapping default alone is insufficient: requirement-only
            // parent paths deliberately do not import recursive values.yaml
            // evidence, so their resolved base can still be unconstrained.
            let member_host_only = !implication.requirements.is_empty()
                && implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::MemberHost { .. }
                    )
                });
            if member_host_only {
                let dispatched = implication.requirements.iter().any(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::MemberHost { handled_kinds }
                            if !handled_kinds.is_empty()
                    )
                });
                let widened = evidence.facts.used_as_serialized
                    || evidence.facts.used_as_yaml_serialized
                    || evidence.facts.used_as_fragment
                    || evidence.facts.has_render_use
                    || evidence.facts.is_ranged_source
                    || evidence.facts.is_partial_scalar_value_path
                    || dispatched;
                let requirement_domain = fail_requirement_runtime_types(implication);
                let resolved_domain = schema_runtime_types(&resolved_target.schema);
                let base_enforces_requirement =
                    !resolved_domain.is_empty() && resolved_domain.is_subset(&requirement_domain);
                // Only an UNCONDITIONAL requirement is provably redundant
                // with the base: a guarded one must keep its own arm — the
                // emitted base can end up wider than the resolved schema
                // this check reads (an open-map merge drops `type: object`),
                // and the guards may fire in states the base leaves open
                // (external-secrets' header read of
                // `.Values.webhook.podDisruptionBudget.enabled`).
                if base_enforces_requirement && !widened && implication.outer_guards.is_empty() {
                    continue;
                }
            }
            if !implication.outer_guards.is_empty()
                && !implication_guards_supported(
                    &implication.outer_guards,
                    target_value_path,
                    &resolved_by_path,
                )
            {
                continue;
            }
            let mut target_schema =
                crate::path_resolver::fail_requirement_schema(std::iter::once(implication));
            if crate::schema_model::is_empty_schema(&target_schema) {
                continue;
            }
            if matches!(
                &implication.target,
                helm_schema_core::ContractRequirementTarget::Value
            ) && let Some(default) = yaml_value_at_path(values_yaml_doc, target_value_path)
            {
                relax_required_members_supplied_by_default(&mut target_schema, default);
            }
            let target_segments = split_value_path(target_value_path);
            if matches!(
                &implication.target,
                helm_schema_core::ContractRequirementTarget::Members { .. }
                    | helm_schema_core::ContractRequirementTarget::MembersWhereEquals { .. }
            ) {
                for descendant in member_descendants
                    .get(target_segments.as_slice())
                    .into_iter()
                    .flatten()
                {
                    let Some(relative_segments) = descendant
                        .path_segments
                        .strip_prefix(target_segments.as_slice())
                    else {
                        continue;
                    };
                    target_schema = crate::schema_tree::insert_path_schema_value(
                        target_schema,
                        relative_segments,
                        descendant.schema.clone(),
                    );
                }
            }
            // Anchor at the ROOT: an arm appended at (or under) the target
            // node lands inside one union alternative, letting the other
            // alternatives bypass the requirement — and union lanes can
            // appear at ANY ancestor, so only the root is bypass-proof.
            let ancestor_segments: Vec<String> = Vec::new();
            // An arm guarded by the target's OWN truthiness never fires
            // on Helm-falsy inputs: those render through the complement
            // branch (harbor's `default .Capabilities.KubeVersion.Version
            // .Values.…kubeVersionOverride` reaching `semverCompare`), and
            // the falsy set spans every runtime type, so a typed base
            // would reject documents the chart renders.
            let preserve_base_schema = implication.outer_guards.is_empty()
                || (!implication_has_self_truthy_guard(implication, target_value_path)
                    && resolved_schema_admits_fail_requirement_domain(
                        &resolved_target.schema,
                        implication,
                    ));
            conditionals.push(ConditionalResolvedSchema {
                target_value_path: target_value_path.clone(),
                relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                ancestor_segments,
                guards: implication.outer_guards.clone(),
                target_schema,
                provider_schema_candidate: None,
                preserve_base_schema,
                fold_unconditional_object_host_into_base: member_host_only,
                arm_only: true,
            });
        }

        for source_overlay in &evidence.conditional_overlays {
            for overlay in kind_partitioned_overlays(source_overlay) {
                if is_unconditional_self_presence_overlay(target_value_path, &overlay) {
                    continue;
                }
                if !guards_supported_for_conditional_lowering(
                    &overlay.guards,
                    &resolved_by_path,
                    values_yaml_doc,
                ) {
                    continue;
                }

                let target_segments = split_value_path(target_value_path);
                let ancestor_segments =
                    conditional_ancestor_segments(&target_segments, &overlay.guards);
                let active_by_defaults =
                    evaluate_guard_set_on_values(&overlay.guards, values_yaml_doc);
                let resolved_overlay =
                    resolve_overlay_target_schema(target_value_path, &overlay, provider);
                // A ranged branch's runtime domain is structural evidence, not
                // a declared-default placeholder. Add it before conditional
                // policy so a fixed map default cannot reintroduce literal
                // member typing that the loop body erased (for example through
                // `quote`).
                let member_implication_owns_range_domain = overlay.evidence.facts.is_ranged_source
                    && crate::schema_model::is_empty_schema(&resolved_overlay.schema)
                    && member_implication_covers_range_domain(
                        &evidence.fail_implications,
                        &overlay.guards,
                    );
                let branch_schema = if overlay.evidence.facts.is_ranged_source
                    && !member_implication_owns_range_domain
                {
                    crate::merge::merge_schema_list(vec![
                        resolved_overlay.schema,
                        crate::runtime_iterable_schema(
                            !overlay.evidence.facts.has_structured_item_descendants
                                && !overlay.evidence.facts.has_destructured_range_use
                                && !overlay.evidence.facts.has_string_contract_items,
                        ),
                    ])
                } else {
                    resolved_overlay.schema
                };
                let target_schema = conditional_target_schema(
                    target_value_path,
                    &overlay,
                    values_yaml_doc,
                    branch_schema,
                    resolved_target.values_yaml_schema.clone(),
                    resolved_target.schema.clone(),
                    active_by_defaults,
                );
                if crate::schema_model::is_empty_schema(&target_schema) {
                    // A branch whose renders are all serialized proves the wider
                    // contract inside that branch, so it carries no schema; it
                    // stays a conditional TARGET so base classification still
                    // uncloses/opens the base the way the guarded renders
                    // demand. Mixed branches resolve their own evidence above,
                    // so a stringified occurrence never erases an independent
                    // stricter sibling.
                    if overlay.evidence.facts.used_as_serialized
                        || overlay.evidence.facts.used_as_yaml_serialized
                    {
                        conditionals.push(ConditionalResolvedSchema {
                            target_value_path: target_value_path.clone(),
                            relative_target_segments: target_segments[ancestor_segments.len()..]
                                .to_vec(),
                            ancestor_segments,
                            guards: overlay.guards.clone(),
                            target_schema,
                            provider_schema_candidate: None,
                            preserve_base_schema: overlay.preserve_base_schema
                                || has_unconditional_self_presence_contract,
                            fold_unconditional_object_host_into_base: false,
                            arm_only: false,
                        });
                    }
                    continue;
                }
                let provider_schema_candidate = resolved_overlay
                    .provider_schema_candidate
                    .filter(|candidate| candidate.survives_as(&target_schema));

                conditionals.push(ConditionalResolvedSchema {
                    target_value_path: target_value_path.clone(),
                    relative_target_segments: target_segments[ancestor_segments.len()..].to_vec(),
                    ancestor_segments,
                    guards: overlay.guards.clone(),
                    target_schema,
                    provider_schema_candidate,
                    preserve_base_schema: overlay.preserve_base_schema
                        || has_unconditional_self_presence_contract,
                    fold_unconditional_object_host_into_base: false,
                    arm_only: false,
                });
            }
        }
    }

    conditionals
}

fn kind_partitioned_overlays(overlay: &ConditionalPathOverlay) -> Vec<ConditionalPathOverlay> {
    let mut kinds = BTreeSet::new();
    for use_ in &overlay.evidence.provider_schema_uses {
        if !use_.resource.kind_candidates.is_empty() {
            kinds.insert(use_.resource.kind.clone());
            kinds.extend(use_.resource.kind_candidates.iter().cloned());
        }
    }
    if kinds.is_empty() {
        return vec![overlay.clone()];
    }
    let Some(selector) = kind_selector_path(&overlay.guards, &kinds) else {
        return vec![overlay.clone()];
    };

    kinds
        .into_iter()
        .filter_map(|kind| {
            let mut partition = overlay.clone();
            partition.guards.push(ConditionalGuard::Eq {
                path: selector.clone(),
                value: GuardValue::string(kind.clone()),
            });
            partition.guards.sort();
            partition.guards.dedup();
            partition.evidence.provider_schema_uses.retain_mut(|use_| {
                if use_.resource.kind_candidates.is_empty() {
                    return true;
                }
                let supports_kind =
                    use_.resource.kind == kind || use_.resource.kind_candidates.contains(&kind);
                if supports_kind {
                    use_.resource.kind = kind.clone();
                    use_.resource.kind_candidates.clear();
                }
                supports_kind
            });
            (!partition.evidence.provider_schema_uses.is_empty()).then_some(partition)
        })
        .collect()
}

fn kind_selector_path(guards: &[ConditionalGuard], kinds: &BTreeSet<String>) -> Option<String> {
    fn collect(guard: &ConditionalGuard, kinds: &BTreeSet<String>, paths: &mut BTreeSet<String>) {
        match guard {
            ConditionalGuard::Eq {
                path,
                value: GuardValue::String(value),
            }
            | ConditionalGuard::NotEq {
                path,
                value: GuardValue::String(value),
            } if kinds.contains(value) => {
                paths.insert(path.clone());
            }
            ConditionalGuard::Not(inner) => collect(inner, kinds, paths),
            ConditionalGuard::AllOf(inner) | ConditionalGuard::AnyOf(inner) => {
                for guard in inner {
                    collect(guard, kinds, paths);
                }
            }
            ConditionalGuard::Truthy { .. }
            | ConditionalGuard::With { .. }
            | ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::HasKey { .. } => {}
        }
    }

    let mut paths = BTreeSet::new();
    for guard in guards {
        collect(guard, kinds, &mut paths);
    }
    let mut paths = paths.into_iter();
    let path = paths.next()?;
    paths.next().is_none().then_some(path)
}

fn is_unconditional_self_presence_overlay(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
) -> bool {
    matches!(
        overlay.guards.as_slice(),
        [ConditionalGuard::Not(inner)]
            if matches!(
                inner.as_ref(),
                ConditionalGuard::Absent { path } if path == target_value_path
            )
    )
}

fn is_bare_iterable_implication(implication: &helm_schema_core::ContractFailImplication) -> bool {
    matches!(
        &implication.target,
        helm_schema_core::ContractRequirementTarget::Value
    ) && matches!(
        implication.requirements.as_slice(),
        [helm_schema_core::FailValueRequirement::Iterable { .. }]
    )
}

fn member_implication_covers_range_domain(
    implications: &[helm_schema_core::ContractFailImplication],
    guards: &[ConditionalGuard],
) -> bool {
    implications.iter().any(|implication| {
        implication.outer_guards == guards
            && matches!(
                &implication.target,
                helm_schema_core::ContractRequirementTarget::Members { .. }
                    | helm_schema_core::ContractRequirementTarget::MembersWhereEquals { .. }
            )
    })
}

fn implication_has_self_truthy_guard(
    implication: &helm_schema_core::ContractFailImplication,
    target_value_path: &str,
) -> bool {
    implication.outer_guards.iter().any(|guard| {
        matches!(
            guard,
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path }
                if path == target_value_path
        )
    })
}

fn resolved_schema_admits_fail_requirement_domain(
    resolved_schema: &Value,
    implication: &helm_schema_core::ContractFailImplication,
) -> bool {
    !crate::schema_model::is_empty_schema(resolved_schema)
        && fail_requirement_runtime_types(implication)
            .is_subset(&schema_runtime_types(resolved_schema))
}

fn fail_requirement_runtime_types(
    implication: &helm_schema_core::ContractFailImplication,
) -> BTreeSet<&'static str> {
    use helm_schema_core::{ContractRequirementTarget, FailValueRequirement};

    let all_types = || {
        BTreeSet::from([
            "array", "boolean", "integer", "null", "number", "object", "string",
        ])
    };
    match &implication.target {
        ContractRequirementTarget::Members { allow_integer } => {
            let mut types = BTreeSet::from(["array", "null", "object"]);
            if *allow_integer {
                types.insert("integer");
            }
            types
        }
        ContractRequirementTarget::MembersMatchingPrefix { .. } => {
            BTreeSet::from(["array", "null", "object"])
        }
        ContractRequirementTarget::MembersWhereEquals { .. } => {
            BTreeSet::from(["array", "null", "object"])
        }
        ContractRequirementTarget::MembersAt { allow_integer, .. } => {
            let mut types = BTreeSet::from(["array", "null", "object"]);
            if *allow_integer {
                types.insert("integer");
            }
            types
        }
        ContractRequirementTarget::Keys => BTreeSet::from(["array", "null", "object"]),
        ContractRequirementTarget::Value => {
            let mut types = all_types();
            for requirement in &implication.requirements {
                types.retain(|runtime_type| match requirement {
                    FailValueRequirement::SchemaType(required)
                    | FailValueRequirement::ComparableKind(required) => {
                        *runtime_type == "null"
                            || *runtime_type == required
                            || required == "number" && *runtime_type == "integer"
                    }
                    FailValueRequirement::NotSchemaType(rejected) => {
                        *runtime_type != rejected
                            && !(rejected == "number" && *runtime_type == "integer")
                    }
                    FailValueRequirement::HasMember(_) => *runtime_type == "object",
                    FailValueRequirement::MatchesPattern { .. } => *runtime_type == "string",
                    FailValueRequirement::MemberHost { handled_kinds } => {
                        *runtime_type == "object"
                            || handled_kinds.iter().any(|handled| handled == runtime_type)
                    }
                    FailValueRequirement::Iterable { allow_integer } => {
                        matches!(*runtime_type, "array" | "null" | "object")
                            || *allow_integer && *runtime_type == "integer"
                    }
                    FailValueRequirement::IndexableAt(_) => {
                        matches!(*runtime_type, "array" | "string")
                    }
                    FailValueRequirement::SplitSegmentsAtLeast {
                        allow_non_string, ..
                    } => *runtime_type == "string" || *allow_non_string,
                });
            }
            types
        }
    }
}

fn schema_runtime_types(schema: &Value) -> BTreeSet<&'static str> {
    let all_types = || {
        BTreeSet::from([
            "array", "boolean", "integer", "null", "number", "object", "string",
        ])
    };
    let Some(object) = schema.as_object() else {
        return if schema.as_bool() == Some(false) {
            BTreeSet::new()
        } else {
            all_types()
        };
    };

    let mut types = match object.get("type") {
        Some(Value::String(schema_type)) => runtime_types_for_declared_type(schema_type),
        Some(Value::Array(schema_types)) => schema_types
            .iter()
            .filter_map(Value::as_str)
            .flat_map(runtime_types_for_declared_type)
            .collect(),
        _ => all_types(),
    };
    if let Some(value) = object.get("const") {
        let const_types = BTreeSet::from([runtime_type_for_value(value)]);
        types = types.intersection(&const_types).copied().collect();
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        let enum_types = values.iter().map(runtime_type_for_value).collect();
        types = types.intersection(&enum_types).copied().collect();
    }

    for keyword in ["anyOf", "oneOf"] {
        if let Some(arms) = object.get(keyword).and_then(Value::as_array) {
            let arm_types = arms.iter().flat_map(schema_runtime_types).collect();
            types = types.intersection(&arm_types).copied().collect();
        }
    }
    if let Some(arms) = object.get("allOf").and_then(Value::as_array) {
        for arm in arms {
            let arm_types = schema_runtime_types(arm);
            types = types.intersection(&arm_types).copied().collect();
        }
    }

    types
}

fn runtime_type_for_value(value: &Value) -> &'static str {
    match value {
        Value::Array(_) => "array",
        Value::Bool(_) => "boolean",
        Value::Null => "null",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::Object(_) => "object",
        Value::String(_) => "string",
    }
}

fn runtime_types_for_declared_type(schema_type: &str) -> BTreeSet<&'static str> {
    match schema_type {
        "array" => BTreeSet::from(["array"]),
        "boolean" => BTreeSet::from(["boolean"]),
        "integer" => BTreeSet::from(["integer"]),
        "null" => BTreeSet::from(["null"]),
        "number" => BTreeSet::from(["integer", "number"]),
        "object" => BTreeSet::from(["object"]),
        "string" => BTreeSet::from(["string"]),
        _ => BTreeSet::new(),
    }
}

fn relax_required_members_supplied_by_default(schema: &mut Value, default: &YamlValue) {
    let (Some(schema), YamlValue::Mapping(defaults)) = (schema.as_object_mut(), default) else {
        return;
    };
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|member| {
            member
                .as_str()
                .is_none_or(|member| !defaults.contains_key(YamlValue::String(member.to_string())))
        });
        if required.is_empty() {
            schema.remove("required");
        }
    }
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        for (member, member_schema) in properties {
            if let Some(member_default) = defaults.get(YamlValue::String(member.clone())) {
                relax_required_members_supplied_by_default(member_schema, member_default);
            }
        }
    }
    for keyword in ["allOf", "anyOf", "oneOf"] {
        let Some(branches) = schema.get_mut(keyword).and_then(Value::as_array_mut) else {
            continue;
        };
        for branch in branches {
            relax_required_members_supplied_by_default(branch, default);
        }
    }
}

pub(crate) fn resolve_overlay_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    provider: &dyn ResourceSchemaOracle,
) -> ResolvedPathSchema {
    let evidence = overlay.evidence.as_path_evidence(target_value_path);
    PathSchemaResolver::resolve_single_path_evidence(&evidence, provider)
}

fn conditional_ancestor_segments(
    target_segments: &[String],
    guards: &[ConditionalGuard],
) -> Vec<String> {
    let mut shared_prefix = target_segments.to_vec();
    for guard in guards {
        for guard_path in guard.value_paths() {
            let guard_path = split_value_path(&guard_path);
            shared_prefix.truncate(common_prefix_len(&shared_prefix, &guard_path));
        }
    }
    shared_prefix
}

fn guards_supported_for_conditional_lowering(
    guards: &[ConditionalGuard],
    resolved_by_path: &BTreeMap<&str, &ResolvedPathSchema>,
    values_yaml_doc: &YamlValue,
) -> bool {
    guards_supported_with_self_path(guards, None, resolved_by_path, values_yaml_doc)
}

/// Fail-implication guard support is more permissive than overlay guard
/// support on TWO axes, both bounded by the arm-only shape (an implication
/// adds an `if guards then requirement` arm and never contributes rows or
/// base structure, so a guard that never fires costs nothing):
/// - a truthy guard over the implication's OWN target path is the
///   capture's structurally derived test subject (`if truthy(x) then x is
///   a string`), not a decoded ambient condition, so the fabricated-path
///   concern does not apply to it even when the chart never declares it;
/// - truthy guards over other undeclared-but-resolved paths lower
///   type-generically: the requirement is a hard render failure, and a
///   fabricated guard path merely leaves the arm inactive.
fn implication_guards_supported(
    guards: &[ConditionalGuard],
    target_value_path: &str,
    resolved_by_path: &BTreeMap<&str, &ResolvedPathSchema>,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                path == target_value_path || resolved_by_path.contains_key(path.as_str())
            }
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::HasKey { .. } => true,
            ConditionalGuard::Not(inner) => implication_guards_supported(
                std::slice::from_ref(inner),
                target_value_path,
                resolved_by_path,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                implication_guards_supported(guards, target_value_path, resolved_by_path)
            }
        })
}

fn guards_supported_with_self_path(
    guards: &[ConditionalGuard],
    self_path: Option<&str>,
    resolved_by_path: &BTreeMap<&str, &ResolvedPathSchema>,
    values_yaml_doc: &YamlValue,
) -> bool {
    !guards.is_empty()
        && guards.iter().all(|guard| match guard {
            // The truthiness condition encoding is type-generic (const true,
            // non-zero number, non-empty string/array/object). Approximate
            // lookups never reach conditional overlays, so every resolved
            // guard path here is structural evidence even when values.yaml
            // does not declare the finite member (literal-dict range keys).
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                self_path == Some(path.as_str())
                    || yaml_value_at_path(values_yaml_doc, path).is_some()
                    || resolved_by_path.contains_key(path.as_str())
            }
            ConditionalGuard::Eq { .. }
            | ConditionalGuard::NotEq { .. }
            | ConditionalGuard::Absent { .. }
            | ConditionalGuard::TypeIs { .. }
            | ConditionalGuard::MatchesPattern { .. }
            | ConditionalGuard::IntGt { .. }
            | ConditionalGuard::HasKey { .. } => true,
            ConditionalGuard::Not(inner) => guards_supported_with_self_path(
                std::slice::from_ref(inner),
                self_path,
                resolved_by_path,
                values_yaml_doc,
            ),
            ConditionalGuard::AllOf(guards) | ConditionalGuard::AnyOf(guards) => {
                guards_supported_with_self_path(
                    guards,
                    self_path,
                    resolved_by_path,
                    values_yaml_doc,
                )
            }
        })
}

#[tracing::instrument(skip_all)]
/// Lower terminating validator formulas: for each clause, no valid values
/// document satisfies ALL its guards, so the document gets
/// `if <guards> then false` at the guards' shared ancestor. Clauses with
/// any unencodable guard are skipped whole — a partially encoded `if`
/// would reject documents the validator never terminates.
pub(crate) fn append_terminal_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[Vec<ConditionalGuard>],
    values_yaml_doc: &YamlValue,
) {
    for guards in clauses {
        // A clause every guard of which can hold VACUOUSLY (with the
        // guarded path or an ancestor absent) must anchor at the root: an
        // `if` nested under `properties.<ancestor>` never fires for
        // documents missing the ancestor — exactly a state such a clause
        // covers (a helper's `required global.version` rejects a document
        // with no `global` at all).
        let ancestor_segments = if guards.iter().all(guard_holds_vacuously) {
            Vec::new()
        } else {
            shared_guard_ancestor_segments(guards)
        };
        if !guards
            .iter()
            .all(|guard| guard_encodes_fully(guard, &ancestor_segments, values_yaml_doc))
        {
            continue;
        }
        let condition = SchemaNode::all_of(build_condition_clauses(
            guards,
            &ancestor_segments,
            values_yaml_doc,
        ));
        root_schema.append_conditional(
            &ancestor_segments,
            condition,
            SchemaNode::foreign(Value::Bool(false)),
        );
    }
}

/// Whether the guard can be satisfied with its path (or an ancestor)
/// absent from the document.
fn guard_holds_vacuously(guard: &ConditionalGuard) -> bool {
    match guard {
        ConditionalGuard::Truthy { .. }
        | ConditionalGuard::With { .. }
        | ConditionalGuard::TypeIs { .. }
        | ConditionalGuard::MatchesPattern { .. }
        | ConditionalGuard::IntGt { .. }
        | ConditionalGuard::HasKey { .. } => false,
        ConditionalGuard::Eq { value, .. } => matches!(value, GuardValue::Null),
        ConditionalGuard::NotEq { .. }
        | ConditionalGuard::Absent { .. }
        | ConditionalGuard::Not(_) => true,
        ConditionalGuard::AllOf(inner) => inner.iter().all(guard_holds_vacuously),
        ConditionalGuard::AnyOf(inner) => inner.iter().any(guard_holds_vacuously),
    }
}

/// The longest common prefix of the PARENTS of every path the guards
/// reference. Presence tests (`required`/`Absent` encodings) need the
/// tested segment to stay relative, so a single-path clause anchors at the
/// path's parent rather than the path itself.
fn shared_guard_ancestor_segments(guards: &[ConditionalGuard]) -> Vec<String> {
    let mut shared: Option<Vec<String>> = None;
    for guard in guards {
        for guard_path in guard.value_paths() {
            let mut segments = split_value_path(&guard_path);
            segments.pop();
            shared = Some(match shared {
                None => segments,
                Some(prefix) => {
                    let len = common_prefix_len(&prefix, &segments);
                    prefix[..len].to_vec()
                }
            });
        }
    }
    shared.unwrap_or_default()
}

#[tracing::instrument(skip_all)]
pub(crate) fn append_conditional_schemas(
    root_schema: &mut SchemaDocument,
    mut conditionals: Vec<ConditionalResolvedSchema>,
    values_yaml_doc: &YamlValue,
) {
    let mut condition_cache = crate::condition_encoding::ConditionFragmentCache::new();
    conditionals.retain(|conditional| {
        let folds_into_base = conditional.fold_unconditional_object_host_into_base
            && conditional.arm_only
            && conditional.guards.is_empty()
            && conditional.ancestor_segments.is_empty()
            && is_object_domain_only(&conditional.target_schema)
            && root_schema.constrain_existing_path_to_object(&conditional.relative_target_segments);
        !folds_into_base
    });
    // Conditionals sharing one guard set and scope conjoin into one if/then:
    // `allOf [{if G then A}, {if G then B}]` is `{if G then A ∧ B}`, and the
    // repeated `if` blocks dominate emitted size on charts with many guarded
    // blocks. Distinct targets merge disjointly; a leaf collision falls back
    // to its own conditional.
    let mut grouped: BTreeMap<
        (Vec<String>, Vec<ConditionalGuard>),
        Vec<ConditionalResolvedSchema>,
    > = BTreeMap::new();
    for conditional in conditionals {
        // Schema-less conditionals exist only to mark a serialized-use
        // target for base classification; nothing to emit.
        if crate::schema_model::is_empty_schema(&conditional.target_schema) {
            continue;
        }
        grouped
            .entry((
                conditional.ancestor_segments.clone(),
                conditional.guards.clone(),
            ))
            .or_default()
            .push(conditional);
    }
    struct ContentGroup {
        fragment: Value,
        guard_sets: Vec<Vec<ConditionalGuard>>,
    }
    let mut by_content: BTreeMap<(Vec<String>, String), ContentGroup> = BTreeMap::new();
    for ((ancestor_segments, guards), group) in grouped {
        let mut merged: Option<Value> = None;
        let mut separate = Vec::new();
        for conditional in group {
            let fragment = build_target_fragment(
                &conditional.relative_target_segments,
                SchemaNode::foreign(conditional.target_schema),
            )
            .into_value();
            match &mut merged {
                None => merged = Some(fragment),
                Some(target) => {
                    if !merge_disjoint_property_fragment(target, fragment.clone()) {
                        separate.push(fragment);
                    }
                }
            }
        }
        for fragment in merged.into_iter().chain(separate) {
            // Conditionals with identical content under one scope disjoin
            // their guards: `if G1 then X` and `if G2 then X` is
            // `if anyOf [G1, G2] then X`, and X (often a repeated provider
            // schema) is the dominant emitted size.
            by_content
                .entry((ancestor_segments.clone(), fragment.to_string()))
                .or_insert_with(|| ContentGroup {
                    fragment,
                    guard_sets: Vec::new(),
                })
                .guard_sets
                .push(guards.clone());
        }
    }
    for ((ancestor_segments, _), group) in by_content {
        // An empty guard set is trivially true: the fragment applies
        // unconditionally (an unguarded fail implication).
        if group.guard_sets.iter().any(Vec::is_empty) {
            root_schema.append_conditional(
                &ancestor_segments,
                SchemaNode::empty(),
                SchemaNode::foreign(group.fragment),
            );
            continue;
        }
        let mut conditions: Vec<SchemaNode> =
            helm_schema_core::GuardDnf::normalize_conditional_guard_disjunction(group.guard_sets)
                .into_iter()
                .map(|guards| {
                    SchemaNode::all_of(crate::condition_encoding::build_condition_clauses_cached(
                        &guards,
                        &ancestor_segments,
                        values_yaml_doc,
                        &mut condition_cache,
                    ))
                })
                .collect();
        let condition = if conditions.len() == 1 {
            conditions.remove(0)
        } else {
            SchemaNode::any_of(conditions)
        };
        root_schema.append_conditional(
            &ancestor_segments,
            condition,
            SchemaNode::foreign(group.fragment),
        );
    }
}

fn is_object_domain_only(schema: &Value) -> bool {
    let Some(object) = schema.as_object() else {
        return false;
    };
    if object.len() != 1 {
        return false;
    }
    match object.get("type") {
        Some(Value::String(schema_type)) => schema_type == "object",
        Some(Value::Array(schema_types)) => {
            !schema_types.is_empty()
                && schema_types
                    .iter()
                    .all(|schema_type| schema_type.as_str() == Some("object"))
        }
        _ => ["anyOf", "oneOf"].into_iter().any(|keyword| {
            object
                .get(keyword)
                .and_then(Value::as_array)
                .is_some_and(|branches| {
                    !branches.is_empty() && branches.iter().all(is_object_domain_only)
                })
        }),
    }
}

/// Merge `incoming` into `target` when both are plain `properties` object
/// fragments whose leaves do not collide; returns false (leaving `target`
/// unchanged) when they do.
fn merge_disjoint_property_fragment(target: &mut Value, incoming: Value) -> bool {
    fn mergeable(target: &Value, incoming: &Value) -> bool {
        let (Some(target), Some(incoming)) = (target.as_object(), incoming.as_object()) else {
            return false;
        };
        let plain_object = |node: &serde_json::Map<String, Value>| {
            node.keys().all(|key| key == "properties" || key == "type")
                && node.get("type").and_then(Value::as_str) == Some("object")
        };
        if !plain_object(target) || !plain_object(incoming) {
            return false;
        }
        let (Some(Value::Object(target_props)), Some(Value::Object(incoming_props))) =
            (target.get("properties"), incoming.get("properties"))
        else {
            return false;
        };
        incoming_props.iter().all(|(key, value)| {
            target_props
                .get(key)
                .is_none_or(|existing| mergeable(existing, value))
        })
    }
    fn merge(target: &mut Value, incoming: Value) {
        let Value::Object(mut incoming_object) = incoming else {
            return;
        };
        let Some(Value::Object(incoming_props)) = incoming_object.remove("properties") else {
            return;
        };
        let Some(target_props) = target
            .as_object_mut()
            .and_then(|object| object.get_mut("properties"))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        for (key, value) in incoming_props {
            match target_props.get_mut(&key) {
                Some(existing) => merge(existing, value),
                None => {
                    target_props.insert(key, value);
                }
            }
        }
    }
    if !mergeable(target, &incoming) {
        return false;
    }
    merge(target, incoming);
    true
}

fn build_target_fragment(path_segments: &[String], leaf_schema: SchemaNode) -> SchemaNode {
    let Some((head, tail)) = path_segments.split_first() else {
        return leaf_schema;
    };

    let child = if tail.is_empty() {
        leaf_schema
    } else {
        build_target_fragment(tail, leaf_schema)
    };
    if head == "*" {
        return SchemaNode::foreign(serde_json::json!({
            "additionalProperties": child.clone().into_value(),
            "items": child.into_value(),
        }));
    }
    // The carrier must claim nothing about the ancestor values themselves: a
    // `with`-chain skips falsy ancestors entirely, so the arm has to hold
    // vacuously there. `properties` descent alone already encodes "when this
    // member exists on an object, the leaf requirement applies"; asserting
    // `type: object` on the carrier would reject the skipped falsy states.
    SchemaNode::untyped_member_host().property(head.clone(), child)
}
