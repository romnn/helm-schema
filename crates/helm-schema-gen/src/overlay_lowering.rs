use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, GuardValue,
    ProviderSchemaFragment, ResourceSchemaOracle,
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
    /// Every member access on this target rides the nil-safe grouped form
    /// (`(.Values.x).member`), which renders at an absent or null-deleted
    /// receiver instead of aborting. The base host materialized for the
    /// target's descendants must then stay untyped — this arm alone carries
    /// the object requirement, scoped to the receiver's strict presence
    /// (nack's root `global`, read only through `((.Values.global).labels)`,
    /// renders at `global: null`).
    relax_untyped_host: bool,
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
        .into_iter()
        .chain(
            crate::required_source_backprojection::synthesized_range_key_implications(
                contract_schema_signals,
                provider,
            ),
        )
        .chain(
            crate::required_source_backprojection::synthesized_ranged_member_required_implications(
                contract_schema_signals,
                provider,
            ),
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
        // A target whose member-host requirements ALL ride its own strict
        // presence was only ever read through the nil-safe grouped form
        // (`(.Values.x).member`): absence and helm's null-deletion render,
        // so the base host materialized for its descendants must stay
        // untyped and the presence-guarded arms alone carry `type: object`.
        let all_member_hosts_presence_scoped = {
            let mut member_host_implications = evidence
                .fail_implications
                .iter()
                .chain(synthesized)
                .filter(|implication| {
                    implication.requirements.iter().any(|requirement| {
                        matches!(
                            requirement,
                            helm_schema_core::FailValueRequirement::MemberHost { .. }
                        )
                    })
                })
                .peekable();
            member_host_implications.peek().is_some()
                && member_host_implications.all(|implication| {
                    implication_has_self_presence_guard(implication, target_value_path)
                })
        };
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
            // A dig-lane TYPE arm scoped by the target's own strict
            // PRESENCE behaves like the self-truthy case: absence (and
            // every state its execution gates leave dormant) must stay
            // open, so the base goes to the guarded-only lane and the arm
            // alone enforces the type where the dig actually executes
            // (KPS's `customRules` under `defaultRules.create: false`).
            // Member-shaped requirements (HasMember, MemberHost) keep the
            // established preserve rules — their presence guards scope
            // probes, not the host's whole typing.
            let presence_scoped_type_arm =
                implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::SchemaType(_)
                            | helm_schema_core::FailValueRequirement::SchemaTypeEvenNull(_)
                    )
                }) && implication_has_self_presence_guard(implication, target_value_path);
            let preserve_base_schema = implication.outer_guards.is_empty()
                || (!implication_has_self_truthy_guard(implication, target_value_path)
                    && !presence_scoped_type_arm
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
                relax_untyped_host: member_host_only && all_member_hosts_presence_scoped,
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
                            relax_untyped_host: false,
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
                    relax_untyped_host: false,
                    arm_only: false,
                });
            }
        }
    }

    append_merge_shadow_arms(&mut conditionals, contract_schema_signals, provider);
    append_omitted_member_arms(&mut conditionals, contract_schema_signals, provider);
    conditionals
}

/// Per-key arms for members a guard-scoped `omit` may remove before the
/// sink reads the map: the whole-payload projection subtracts them, and
/// each key whose RETAIN guards lowered comes back as
/// `if retain-guards then map.key matches the provider's member schema`
/// (external-secrets' `adaptSecurityContext` — `runAsUser` stays
/// integer-typed exactly where the OpenShift adaptation certainly does
/// not run). Keys without retain guards stay subtracted: their survival
/// is undecidable, so their typing abstains.
fn append_omitted_member_arms(
    conditionals: &mut Vec<ConditionalResolvedSchema>,
    contract_schema_signals: &ContractSchemaSignals,
    provider: &dyn ResourceSchemaOracle,
) {
    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let mut arms: BTreeSet<(String, Vec<ConditionalGuard>, String)> = BTreeSet::new();
        // A provider use recorded on a conditional overlay branch fires
        // only under the branch guards, so its re-add arms must carry them
        // too (external-secrets renders the adapted context only under
        // `.enabled` and a member-count gate).
        let uses_with_guards = evidence
            .provider_schema_uses
            .iter()
            .map(|provider_use| (provider_use, Vec::new()))
            .chain(evidence.conditional_overlays.iter().flat_map(|overlay| {
                overlay
                    .evidence
                    .provider_schema_uses
                    .iter()
                    .map(|provider_use| (provider_use, overlay.guards.clone()))
            }));
        for (provider_use, branch_guards) in uses_with_guards {
            if provider_use.omitted_members.is_empty() {
                continue;
            }
            let Some(fragment) = provider.schema_fragment_for_use(provider_use) else {
                continue;
            };
            let payload = fragment.schema();
            let definitions = ["$defs", "definitions"]
                .iter()
                .find_map(|key| payload.get(*key).and_then(Value::as_object));
            let Some(properties) = payload.get("properties").and_then(Value::as_object) else {
                continue;
            };
            for (member, retain_guards) in &provider_use.omitted_members {
                if retain_guards.is_empty() {
                    continue;
                }
                let Some(member_schema) = properties
                    .get(member)
                    .and_then(|schema| dereferenced_payload_subschema(schema, definitions, 8))
                else {
                    continue;
                };
                let mut guards = branch_guards.clone();
                guards.extend(retain_guards.iter().cloned());
                guards.sort();
                guards.dedup();
                arms.insert((member.clone(), guards, member_schema.to_string()));
            }
        }
        let target_segments = split_value_path(value_path);
        for (member, guards, member_schema) in arms {
            let Ok(member_schema) = serde_json::from_str::<Value>(&member_schema) else {
                continue;
            };
            conditionals.push(ConditionalResolvedSchema {
                target_value_path: value_path.clone(),
                relative_target_segments: target_segments.clone(),
                ancestor_segments: Vec::new(),
                guards,
                target_schema: serde_json::json!({
                    "properties": { member: member_schema }
                }),
                provider_schema_candidate: None,
                preserve_base_schema: true,
                fold_unconditional_object_host_into_base: false,
                relax_untyped_host: false,
                arm_only: true,
            });
        }
    }
}

/// Per-key arms for SHADOWED merge layers: with destination-first
/// `merge preferred legacy`, a legacy member reaches the provider slot only
/// where every earlier layer lacks that key, so each provider property `k`
/// gets an arm `if no earlier layer has k, then legacy.k matches the
/// provider's member schema` (velero's deprecated `securityContext` beside
/// `podSecurityContext`). The arms are finite — enumerated from the
/// resolved provider payload's own properties — and the earlier layers'
/// whole-payload typing rides its ordinary self-truthy branch.
fn append_merge_shadow_arms(
    conditionals: &mut Vec<ConditionalResolvedSchema>,
    contract_schema_signals: &ContractSchemaSignals,
    provider: &dyn ResourceSchemaOracle,
) {
    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        for provider_use in &evidence.provider_schema_uses {
            let Some(merge) = provider_use.merge_layers.as_ref() else {
                continue;
            };
            let fragment = provider.schema_fragment_for_use(provider_use);
            let payload = fragment.as_ref().map(ProviderSchemaFragment::schema);
            let definitions = payload.and_then(|payload| {
                ["$defs", "definitions"]
                    .iter()
                    .find_map(|key| payload.get(*key).and_then(Value::as_object))
            });
            let target_segments = split_value_path(value_path);
            // The whole payload types this layer exactly where no earlier
            // layer can shadow it: the preferred layer's keys always win
            // (its guard is its own truthiness alone), and a shadowed layer
            // is fully visible when every earlier layer is Helm-empty. The
            // layer-absence form is the only member typing a payload with
            // DYNAMIC member names admits (KPS's rule annotations under
            // `additionalProperties: {type: string}`); enumerated members
            // additionally get the finer per-key arms below. A sink whose
            // provider fragment is unavailable still types through its
            // metadata field kind (keda's CRD annotations merge).
            let provider_whole = payload
                .and_then(Value::as_object)
                .map(|object| Value::Object(object.clone()))
                .and_then(|value| dereferenced_payload_subschema(&value, definitions, 8))
                .map(|mut whole| {
                    if let Some(object) = whole.as_object_mut() {
                        object.remove("$defs");
                        object.remove("definitions");
                    }
                    whole
                });
            let metadata_whole = metadata_sink_schema(&provider_use.path.0);
            let whole = match (provider_whole, metadata_whole) {
                (Some(provider_whole), Some(metadata_whole)) => {
                    Some(crate::merge::merge_schema_list(vec![
                        provider_whole,
                        metadata_whole,
                    ]))
                }
                (whole, None) | (None, whole) => whole,
            };
            if let Some(mut whole) = whole {
                if merge.nil_scrubbed_layers.get(merge.position) == Some(&true) {
                    null_relax_member_schemas(&mut whole);
                }
                let mut guards = vec![ConditionalGuard::Truthy {
                    path: value_path.clone(),
                }];
                guards.extend(merge.shadowed_by().iter().map(|earlier| {
                    ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                        path: earlier.clone(),
                    }))
                }));
                guards.extend(provider_use.outer_guards.iter().cloned());
                guards.sort();
                guards.dedup();
                conditionals.push(ConditionalResolvedSchema {
                    target_value_path: value_path.clone(),
                    relative_target_segments: target_segments.clone(),
                    ancestor_segments: Vec::new(),
                    guards,
                    target_schema: whole,
                    provider_schema_candidate: None,
                    preserve_base_schema: true,
                    fold_unconditional_object_host_into_base: false,
                    relax_untyped_host: false,
                    arm_only: true,
                });
            }
            if merge.position == 0 {
                continue;
            }
            let Some(properties) = payload
                .and_then(|payload| payload.get("properties"))
                .and_then(Value::as_object)
            else {
                continue;
            };
            for (member, member_schema) in properties {
                let Some(mut member_schema) =
                    dereferenced_payload_subschema(member_schema, definitions, 8)
                else {
                    continue;
                };
                if merge.nil_scrubbed_layers.get(merge.position) == Some(&true) {
                    null_relax_member_schemas(&mut member_schema);
                    member_schema = serde_json::json!({
                        "anyOf": [member_schema, { "type": "null" }]
                    });
                }
                let mut guards: Vec<ConditionalGuard> = merge
                    .shadowed_by()
                    .iter()
                    .map(|earlier| {
                        ConditionalGuard::Not(Box::new(ConditionalGuard::HasKey {
                            path: earlier.clone(),
                            key: member.clone(),
                        }))
                    })
                    .collect();
                guards.extend(provider_use.outer_guards.iter().cloned());
                guards.sort();
                guards.dedup();
                let target_schema = serde_json::json!({
                    "properties": { member: member_schema }
                });
                conditionals.push(ConditionalResolvedSchema {
                    target_value_path: value_path.clone(),
                    relative_target_segments: target_segments.clone(),
                    ancestor_segments: Vec::new(),
                    guards,
                    target_schema,
                    provider_schema_candidate: None,
                    preserve_base_schema: true,
                    fold_unconditional_object_host_into_base: false,
                    relax_untyped_host: false,
                    arm_only: true,
                });
            }
        }
    }
}

/// Admit `null` for every MEMBER of a nil-scrubbed layer's payload
/// schema, recursively: the scrub removes nil map members at any depth
/// before the sink renders, so a null member spelling never reaches the
/// provider. The payload's own top level keeps its typing — the layer
/// arm already scopes it by the layer's truthiness. List items stay
/// strict (the scrub copies non-map members verbatim, nested nulls
/// included). A provider-`required` member nulled away renders as a
/// missing field the provider rejects; the relaxation deliberately
/// abstains from re-encoding that as an input rejection.
fn null_relax_member_schemas(schema: &mut Value) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    for group in ["allOf", "anyOf", "oneOf"] {
        if let Some(Value::Array(arms)) = object.get_mut(group) {
            for arm in arms {
                null_relax_member_schemas(arm);
            }
        }
    }
    for members_key in ["properties", "patternProperties"] {
        if let Some(Value::Object(members)) = object.get_mut(members_key) {
            for member in members.values_mut() {
                null_relax_member_schemas(member);
                if member.is_object() {
                    let original = std::mem::take(member);
                    *member = serde_json::json!({ "anyOf": [original, { "type": "null" }] });
                }
            }
        }
    }
    if let Some(additional) = object.get_mut("additionalProperties")
        && additional.is_object()
    {
        null_relax_member_schemas(additional);
        let original = std::mem::take(additional);
        *additional = serde_json::json!({ "anyOf": [original, { "type": "null" }] });
    }
}

/// The sink's metadata field-kind schema when the slot is a
/// `metadata.annotations`/`metadata.labels` string map. Scalar metadata
/// fields never host a map merge, so only the string-map kinds apply.
fn metadata_sink_schema(path: &[String]) -> Option<Value> {
    let parent = path
        .len()
        .checked_sub(2)
        .and_then(|index| path.get(index))?;
    if parent != "metadata" {
        return None;
    }
    matches!(path.last()?.as_str(), "labels" | "annotations").then(|| {
        serde_json::json!({
            "type": "object",
            "additionalProperties": { "type": "string" },
        })
    })
}

/// Replace payload-internal `$ref`s with their payload-level definitions so
/// a property subschema stays self-contained when copied into an arm.
/// Cyclic or unresolved references abstain via the depth bound.
fn dereferenced_payload_subschema(
    schema: &Value,
    definitions: Option<&serde_json::Map<String, Value>>,
    depth: u8,
) -> Option<Value> {
    if depth == 0 {
        return None;
    }
    match schema {
        Value::Object(object) => {
            if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                let name = reference
                    .strip_prefix("#/$defs/")
                    .or_else(|| reference.strip_prefix("#/definitions/"))?;
                let definition = definitions?.get(name)?;
                return dereferenced_payload_subschema(definition, definitions, depth - 1);
            }
            let mut out = serde_json::Map::new();
            for (key, value) in object {
                out.insert(
                    key.clone(),
                    dereferenced_payload_subschema(value, definitions, depth)?,
                );
            }
            Some(Value::Object(out))
        }
        Value::Array(items) => Some(Value::Array(
            items
                .iter()
                .map(|item| dereferenced_payload_subschema(item, definitions, depth))
                .collect::<Option<_>>()?,
        )),
        other => Some(other.clone()),
    }
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
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. } => {}
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

/// Whether an outer guard scopes the arm to the target's own strict
/// PRESENCE — `¬Absent(target)` or a `HasKey` naming the target as its
/// parent's member. Such arms fire only where the value exists, so the
/// base must keep its independent resolution.
fn implication_has_self_presence_guard(
    implication: &helm_schema_core::ContractFailImplication,
    target_value_path: &str,
) -> bool {
    implication.outer_guards.iter().any(|guard| match guard {
        ConditionalGuard::Not(inner) => matches!(
            inner.as_ref(),
            ConditionalGuard::Absent { path } if path == target_value_path
        ),
        ConditionalGuard::HasKey { path, key } => {
            let mut segments = split_value_path(path);
            segments.push(key.clone());
            segments == split_value_path(target_value_path)
        }
        _ => false,
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
    use helm_schema_core::ContractRequirementTarget;

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
                types.retain(|runtime_type| {
                    requirement_admits_runtime_type(requirement, runtime_type)
                });
            }
            types
        }
    }
}

fn requirement_admits_runtime_type(
    requirement: &helm_schema_core::FailValueRequirement,
    runtime_type: &str,
) -> bool {
    use helm_schema_core::FailValueRequirement;
    match requirement {
        FailValueRequirement::SchemaType(required)
        | FailValueRequirement::ComparableKind(required) => {
            runtime_type == "null"
                || runtime_type == required
                || required == "number" && runtime_type == "integer"
        }
        FailValueRequirement::SchemaTypeEvenNull(required) => {
            runtime_type == required || required == "number" && runtime_type == "integer"
        }
        // Every runtime kind has a Helm-falsy escape spelling.
        FailValueRequirement::TruthyImpliesSchemaType(_) => true,
        FailValueRequirement::HelmTruthy => runtime_type != "null",
        FailValueRequirement::HelmFalsy => true,
        FailValueRequirement::FieldHelmFalsy { .. } => true,
        FailValueRequirement::FieldNotEquals { .. } => true,
        FailValueRequirement::FieldEquals { .. }
        | FailValueRequirement::FieldPresentNotNull { .. }
        | FailValueRequirement::FieldHelmTruthy { .. } => runtime_type == "object",
        FailValueRequirement::NotEquals(_) => true,
        FailValueRequirement::NotSchemaType(rejected) => {
            runtime_type != rejected && !(rejected == "number" && runtime_type == "integer")
        }
        FailValueRequirement::HasMember(_) => runtime_type == "object",
        FailValueRequirement::MatchesPattern { .. }
        | FailValueRequirement::NotMatchesPattern { .. }
        | FailValueRequirement::StringLengthBounds { .. } => runtime_type == "string",
        FailValueRequirement::MemberHost { handled_kinds } => {
            runtime_type == "object" || handled_kinds.iter().any(|handled| handled == runtime_type)
        }
        FailValueRequirement::Iterable { allow_integer } => {
            matches!(runtime_type, "array" | "null" | "object")
                || *allow_integer && runtime_type == "integer"
        }
        FailValueRequirement::IndexableAt(_) => {
            matches!(runtime_type, "array" | "string")
        }
        FailValueRequirement::SplitSegmentsAtLeast {
            allow_non_string, ..
        } => runtime_type == "string" || *allow_non_string,
        // Constrains rendered content, not the value's kind.
        FailValueRequirement::QuotedSerializationSafe { .. } => true,
        // A kind survives when SOME alternative fully admits it.
        FailValueRequirement::AnyOf(alternatives) => alternatives.iter().any(|alternative| {
            alternative
                .iter()
                .all(|requirement| requirement_admits_runtime_type(requirement, runtime_type))
        }),
    }
}

pub(crate) fn schema_runtime_types(schema: &Value) -> BTreeSet<&'static str> {
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
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. } => true,
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
            | ConditionalGuard::IntLt { .. }
            | ConditionalGuard::HasKey { .. }
            | ConditionalGuard::ContainsMemberEquals { .. }
            | ConditionalGuard::AtMostOneMember { .. }
            | ConditionalGuard::MinMembers { .. } => true,
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
    subchart_defaults_doc: &YamlValue,
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
        if !guards.iter().all(|guard| {
            guard_encodes_fully(
                guard,
                &ancestor_segments,
                values_yaml_doc,
                subchart_defaults_doc,
            )
        }) {
            continue;
        }
        let condition = SchemaNode::all_of(build_condition_clauses(
            guards,
            &ancestor_segments,
            values_yaml_doc,
            subchart_defaults_doc,
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
        | ConditionalGuard::IntLt { .. }
        | ConditionalGuard::HasKey { .. }
        | ConditionalGuard::ContainsMemberEquals { .. }
        | ConditionalGuard::MinMembers { .. } => false,
        ConditionalGuard::Eq { value, .. } => matches!(value, GuardValue::Null),
        ConditionalGuard::NotEq { .. }
        | ConditionalGuard::Absent { .. }
        | ConditionalGuard::AtMostOneMember { .. }
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
    subchart_defaults_doc: &YamlValue,
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
    // Nil-safe member hosts drop the structural `type: object` their
    // descendants materialized: the presence-guarded arm emitted below is
    // the exact contract (grouped reads render at absent/null receivers).
    // Only arms that actually emit may relax — a dropped arm would turn
    // the relaxation into a plain widening.
    for conditional in &conditionals {
        if conditional.relax_untyped_host
            && !crate::schema_model::is_empty_schema(&conditional.target_schema)
        {
            let mut segments = conditional.ancestor_segments.clone();
            segments.extend(conditional.relative_target_segments.iter().cloned());
            root_schema.relax_host_object_type(&segments);
        }
    }
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
    // Arms sharing one scope and one encoded condition conjoin their
    // contents: `if C then A` beside `if C then B` is `if C then A ∧ B`,
    // and the repeated condition trees dominate emitted size on charts
    // whose lanes share a few big gates (temporal's per-service config).
    // Coalesced arms keep the FIRST occurrence's position so unaffected
    // documents keep their emission order; trivially-true fragments have
    // no if-block to save and land as their own conjuncts unchanged.
    let mut emissions: Vec<(Vec<String>, SchemaNode, Vec<SchemaNode>)> = Vec::new();
    let mut emission_index: BTreeMap<(Vec<String>, String), usize> = BTreeMap::new();
    for ((ancestor_segments, _), group) in by_content {
        // An empty guard set is trivially true: the fragment applies
        // unconditionally (an unguarded fail implication).
        if group.guard_sets.iter().any(Vec::is_empty) {
            emissions.push((
                ancestor_segments,
                SchemaNode::empty(),
                vec![SchemaNode::foreign(group.fragment)],
            ));
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
                        subchart_defaults_doc,
                        &mut condition_cache,
                    ))
                })
                .collect();
        let condition = if conditions.len() == 1 {
            conditions.remove(0)
        } else {
            SchemaNode::any_of(conditions)
        };
        let key = (
            ancestor_segments.clone(),
            condition.clone().into_value().to_string(),
        );
        match emission_index.entry(key) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                emissions[*entry.get()]
                    .2
                    .push(SchemaNode::foreign(group.fragment));
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(emissions.len());
                emissions.push((
                    ancestor_segments,
                    condition,
                    vec![SchemaNode::foreign(group.fragment)],
                ));
            }
        }
    }
    for (ancestor_segments, condition, mut contents) in emissions {
        let content = if contents.len() == 1 {
            contents.remove(0)
        } else {
            SchemaNode::all_of(contents)
        };
        root_schema.append_conditional(&ancestor_segments, condition, content);
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
