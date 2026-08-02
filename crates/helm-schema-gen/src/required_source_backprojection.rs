//! Null-intolerant provider output slots require their source leaves.
//!
//! When a direct scalar `.Values` hole renders into a typed resource field,
//! Helm still renders on a missing or null source — emitting an explicit null
//! the provider then rejects (a Service `port` or PVC storage request, for
//! example). The source leaf must therefore be present and non-null wherever
//! the render is live, including an unconditional render. The output field
//! need not itself be required: an optional typed field is still invalid when
//! the rendered manifest explicitly supplies null.
//!
//! The projection is deliberately bounded to unconditional uses backed by a
//! non-null chart default, or branch overlays with exact decoded guards that
//! are dormant on a missing default. This keeps intentionally unset chart
//! inputs valid while still rejecting a null override that deletes a
//! provider-valid default. Dependency-owned defaults abstain because Helm
//! refills them at the subchart coalescing stage. Ordered merge layers also
//! abstain: absence in one layer selects a lower layer rather than rendering
//! null, so presence is a property of the combined result, not any individual
//! source path. Path-wide serialization or fragment facts from a separate use
//! cannot erase an independent direct provider hole; all-use nullability still
//! makes defaulted and self-guarded paths abstain. The requirements ride the
//! same root-anchored arm machinery as `fail` implications.

use std::collections::{BTreeMap, HashMap};

use helm_schema_core::{
    ContractFailImplication, ContractRequirementTarget, ContractSchemaSignals,
    FailValueRequirement, ProviderSchemaUse, ResourceSchemaOracle, ValueKind,
};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

#[expect(
    clippy::too_many_lines,
    reason = "the direct and conditional projections share one provider-nullability cache and source-default invariant"
)]
pub(crate) fn synthesized_required_source_implications(
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    subchart_defaults_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
) -> BTreeMap<String, Vec<ContractFailImplication>> {
    let mut implications: BTreeMap<String, Vec<ContractFailImplication>> = BTreeMap::new();
    let mut null_rejected_by_use: HashMap<ProviderSchemaUse, bool> = HashMap::new();

    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let segments = crate::split_value_path(value_path);
        let Some((leaf_segment, parent_segments)) = segments.split_last() else {
            continue;
        };
        if segments.iter().any(|segment| segment == "*") {
            continue;
        }
        let base_facts = evidence.facts;
        if base_facts.has_unconditional_render_use
            && !source_has_dependency_default(subchart_defaults_doc, value_path)
            && source_has_non_null_default(values_yaml_doc, value_path)
            && !base_facts.is_nullable
            && evidence.provider_schema_uses.iter().any(|use_| {
                use_.kind == ValueKind::Scalar
                    && !use_.is_self_range_collection
                    && !use_.source_null_tolerant
                    && use_.merge_layers.is_none()
                    && !use_.range_key
                    && use_.outer_guards.is_empty()
                    && *null_rejected_by_use
                        .entry(use_.clone())
                        .or_insert_with(|| provider_use_rejects_null(provider, use_))
            })
        {
            push_implication(
                &mut implications,
                helm_schema_core::join_value_path(parent_segments.iter().cloned()),
                ContractFailImplication {
                    outer_guards: Vec::new(),
                    target: ContractRequirementTarget::Value,
                    requirements: vec![FailValueRequirement::HasMemberEvenDefaulted(
                        leaf_segment.clone(),
                    )],
                },
            );
            push_implication(
                &mut implications,
                value_path.clone(),
                ContractFailImplication {
                    outer_guards: Vec::new(),
                    target: ContractRequirementTarget::Value,
                    requirements: vec![FailValueRequirement::NotSchemaType("null".to_string())],
                },
            );
        }
        for overlay in &evidence.conditional_overlays {
            if overlay.guards.is_empty() {
                continue;
            }
            if source_has_dependency_default(subchart_defaults_doc, value_path) {
                continue;
            }
            if !source_has_non_null_default(values_yaml_doc, value_path)
                && crate::condition_encoding::evaluate_guard_set_on_values(
                    &overlay.guards,
                    values_yaml_doc,
                ) != Some(false)
            {
                continue;
            }
            // A guard on the leaf itself keys the branch on the value's own
            // presence or truthiness: its dormant arm must stay open.
            if overlay
                .guards
                .iter()
                .any(|guard| guard.value_paths().contains(value_path))
            {
                continue;
            }
            // Self-`default` fallbacks and self-truthy field guards are
            // dropped from overlay guard sets as source null-tolerance, so
            // the branch's nullability fact is the marker that absence
            // renders something else (or nothing) instead of a null.
            let facts = overlay.evidence.facts;
            if facts.is_nullable {
                continue;
            }
            let renders_into_required_field =
                overlay.evidence.provider_schema_uses.iter().any(|use_| {
                    use_.kind == ValueKind::Scalar
                        && !use_.is_self_range_collection
                        && !use_.source_null_tolerant
                        && use_.merge_layers.is_none()
                        && !use_.range_key
                        && *null_rejected_by_use
                            .entry(use_.clone())
                            .or_insert_with(|| provider_use_rejects_null(provider, use_))
                });
            if !renders_into_required_field {
                continue;
            }
            push_implication(
                &mut implications,
                helm_schema_core::join_value_path(parent_segments.iter().cloned()),
                ContractFailImplication {
                    outer_guards: overlay.guards.clone(),
                    target: ContractRequirementTarget::Value,
                    requirements: vec![FailValueRequirement::HasMemberEvenDefaulted(
                        leaf_segment.clone(),
                    )],
                },
            );
            push_implication(
                &mut implications,
                value_path.clone(),
                ContractFailImplication {
                    outer_guards: overlay.guards.clone(),
                    target: ContractRequirementTarget::Value,
                    requirements: vec![FailValueRequirement::NotSchemaType("null".to_string())],
                },
            );
        }
    }

    implications
}

/// The ranged-member half of the required-source projection: a wildcard
/// member LEAF (`extraPorts.*.containerPort`, `…httpHeaders.*.name`)
/// rendered into a null-intolerant provider slot emits an explicit null for
/// every member missing the leaf, which the provider rejects. Guards
/// split by scope: collection-level guards ride the arm as outer guards,
/// while a member-scoped truthiness guard becomes the ESCAPE alternative
/// of a per-member disjunction (promtail's `service` arm renders its own
/// port, so only service-less members need `containerPort`). Any other
/// member-scoped guard shape abstains — firing wider than the real branch
/// would reject members the chart renders.
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
pub(crate) fn synthesized_ranged_member_required_implications(
    contract_schema_signals: &ContractSchemaSignals,
    subchart_defaults_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
) -> BTreeMap<String, Vec<ContractFailImplication>> {
    let mut implications: BTreeMap<String, Vec<ContractFailImplication>> = BTreeMap::new();
    let mut null_rejected_by_use: HashMap<ProviderSchemaUse, bool> = HashMap::new();

    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let segments = crate::split_value_path(value_path);
        let Some(star) = segments.iter().position(|segment| segment == "*") else {
            continue;
        };
        let Some(collection_segments) = segments.get(..star) else {
            continue;
        };
        let Some(field_segments) = segments.get(star + 1..) else {
            continue;
        };
        if collection_segments.is_empty()
            || field_segments.is_empty()
            || field_segments.iter().any(|segment| segment == "*")
        {
            continue;
        }
        let collection_path =
            helm_schema_core::join_value_path(collection_segments.iter().cloned());
        let member_scope = format!("{collection_path}.*");

        let base_uses = std::iter::once((
            &[] as &[helm_schema_core::ConditionalGuard],
            evidence.facts,
            &evidence.provider_schema_uses,
        ));
        let overlay_uses = evidence.conditional_overlays.iter().map(|overlay| {
            (
                overlay.guards.as_slice(),
                overlay.evidence.facts,
                &overlay.evidence.provider_schema_uses,
            )
        });
        for (guards, facts, uses) in base_uses.chain(overlay_uses) {
            // Tolerant render forms emit something else (or nothing) on a
            // missing source; only the direct scalar hole forces the null.
            // Self-`default` fallbacks surface as nullability, exactly as
            // in the direct lane above.
            if facts.is_nullable {
                continue;
            }
            let renders_into_required_field = uses.iter().any(|use_| {
                // A bare scalar hole forces the null directly; a Sprig
                // `quote`/`squote` render does too — the transform SKIPS
                // nil operands, emitting nothing where the slot demands a
                // value (traefik's local-plugin `mountPath`). Every other
                // serialized form stays tolerant.
                let null_forcing = match use_.kind {
                    ValueKind::Scalar => true,
                    ValueKind::Serialized => use_.nil_omitting,
                    _ => false,
                };
                null_forcing
                    && !use_.is_self_range_collection
                    && !use_.source_null_tolerant
                    && use_.merge_layers.is_none()
                    && !use_.range_key
                    && use_.split_segment.is_none()
                    && *null_rejected_by_use
                        .entry(use_.clone())
                        .or_insert_with(|| provider_use_rejects_null(provider, use_))
            });
            if !renders_into_required_field {
                continue;
            }
            let mut outer_guards = Vec::new();
            let mut escapes: Vec<Vec<FailValueRequirement>> = Vec::new();
            let mut undecodable = false;
            for guard in guards {
                let paths = guard.value_paths();
                if paths.iter().all(|path| !path.contains('*')) {
                    outer_guards.push(guard.clone());
                    continue;
                }
                let member_field = |path: &str| {
                    path.strip_prefix(&format!("{member_scope}."))
                        .filter(|field| !field.contains('*'))
                        .map(helm_schema_core::split_value_path)
                };
                // A member guard becomes the exact dormant alternative:
                // an else-arm escapes when its field is truthy, while a
                // positive arm escapes when its member or tested field is
                // falsy. Defaulted leaves were already excluded by the
                // nullability facts above.
                match guard {
                    helm_schema_core::ConditionalGuard::Not(inner) => match inner.as_ref() {
                        helm_schema_core::ConditionalGuard::Truthy { path } => {
                            match member_field(path) {
                                Some(field) => {
                                    escapes.push(vec![FailValueRequirement::FieldHelmTruthy {
                                        path: field,
                                    }]);
                                }
                                None => undecodable = true,
                            }
                        }
                        _ => undecodable = true,
                    },
                    helm_schema_core::ConditionalGuard::Truthy { path }
                        if path == &member_scope =>
                    {
                        escapes.push(vec![FailValueRequirement::HelmFalsy]);
                    }
                    helm_schema_core::ConditionalGuard::Truthy { path } => {
                        match member_field(path) {
                            Some(field) => {
                                escapes.push(vec![FailValueRequirement::FieldHelmFalsy {
                                    path: field,
                                }]);
                            }
                            None => undecodable = true,
                        }
                    }
                    _ => undecodable = true,
                }
            }
            if undecodable {
                continue;
            }
            let field_path: Vec<String> = field_segments.to_vec();
            let presence = FailValueRequirement::FieldPresentNotNull { path: field_path };
            let requirements = if escapes.is_empty() {
                vec![presence]
            } else {
                let mut alternatives = escapes;
                alternatives.push(vec![presence]);
                vec![FailValueRequirement::AnyOf(alternatives)]
            };
            push_implication(
                &mut implications,
                collection_path.clone(),
                ContractFailImplication {
                    outer_guards,
                    // An integer iterable has no members to constrain;
                    // leaving that lane open is the safe direction.
                    target: dependency_defaulted_member_keys(
                        subchart_defaults_doc,
                        collection_segments,
                        field_segments,
                    )
                    .map_or(
                        ContractRequirementTarget::Members {
                            allow_integer: true,
                        },
                        |keys| ContractRequirementTarget::MembersExceptKeys {
                            keys,
                            allow_integer: true,
                        },
                    ),
                    requirements,
                },
            );
        }
    }

    implications
}

fn source_has_non_null_default(values_yaml_doc: &YamlValue, value_path: &str) -> bool {
    crate::values_yaml::yaml_value_at_path(values_yaml_doc, value_path)
        .is_some_and(|value| !value.is_null())
}

fn source_has_dependency_default(subchart_defaults_doc: &YamlValue, value_path: &str) -> bool {
    crate::values_yaml::yaml_value_at_path(subchart_defaults_doc, value_path).is_some()
}

fn dependency_defaulted_member_keys(
    subchart_defaults_doc: &YamlValue,
    collection_segments: &[String],
    field_segments: &[String],
) -> Option<std::collections::BTreeSet<String>> {
    let YamlValue::Mapping(members) =
        crate::values_yaml::yaml_value_at_segments(subchart_defaults_doc, collection_segments)?
    else {
        return None;
    };
    let keys = members
        .iter()
        .filter_map(|(key, member)| {
            let key = key.as_str()?;
            crate::values_yaml::yaml_value_at_segments(member, field_segments)
                .filter(|default| !default.is_null())
                .map(|_| key.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>();
    (!keys.is_empty()).then_some(keys)
}

fn provider_use_rejects_null(
    provider: &dyn ResourceSchemaOracle,
    provider_use: &ProviderSchemaUse,
) -> bool {
    let Some(fragment) = provider.schema_fragment_for_use(provider_use) else {
        return false;
    };
    jsonschema::validator_for(fragment.schema())
        .is_ok_and(|validator| !validator.is_valid(&Value::Null))
}

/// Fail implications for provider slots observed through a SPLIT SEGMENT
/// of the raw string (tempo's `regexSplit ":" . -1 | last` port suffix):
/// wherever the source is truthy, its named segment must satisfy the
/// integer slot. Base provider uses are self-scoped or unconditional by
/// construction, so the self-truthy guard never over-fires — falsy sources
/// skip their `with`-scoped render (or render an empty segment the sibling
/// string-contract arm already governs).
pub(crate) fn synthesized_split_segment_implications(
    contract_schema_signals: &ContractSchemaSignals,
    provider: &dyn ResourceSchemaOracle,
) -> BTreeMap<String, Vec<ContractFailImplication>> {
    let mut implications: BTreeMap<String, Vec<ContractFailImplication>> = BTreeMap::new();
    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        for use_ in &evidence.provider_schema_uses {
            let Some(segment) = &use_.split_segment else {
                continue;
            };
            if use_.kind != ValueKind::Scalar {
                continue;
            }
            let Some(pattern) = provider.schema_fragment_for_use(use_).and_then(|fragment| {
                crate::resolve_policy::split_segment_pattern(fragment.schema(), segment)
            }) else {
                continue;
            };
            push_implication(
                &mut implications,
                value_path.clone(),
                ContractFailImplication {
                    outer_guards: vec![helm_schema_core::ConditionalGuard::Truthy {
                        path: value_path.clone(),
                    }],
                    target: ContractRequirementTarget::Value,
                    requirements: vec![FailValueRequirement::MatchesPattern {
                        pattern,
                        templated: false,
                    }],
                },
            );
        }
    }
    implications
}

/// Range-KEY slot uses: `- name: {{ $key }}` renders each key of a directly
/// ranged collection at a provider slot. When that slot admits only
/// strings, the integer keys of a non-empty LIST lane cannot render
/// validly, so the collection's key domain must be strings; an empty list
/// runs zero iterations and stays open, as do maps (JSON object keys are
/// always strings, so the arm is vacuous for them beyond documentation).
pub(crate) fn synthesized_range_key_implications(
    contract_schema_signals: &ContractSchemaSignals,
    provider: &dyn ResourceSchemaOracle,
) -> BTreeMap<String, Vec<ContractFailImplication>> {
    let mut implications: BTreeMap<String, Vec<ContractFailImplication>> = BTreeMap::new();
    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let overlay_uses = evidence.conditional_overlays.iter().flat_map(|overlay| {
            overlay
                .evidence
                .provider_schema_uses
                .iter()
                .map(move |use_| (overlay.guards.as_slice(), use_))
        });
        for (branch_guards, use_) in evidence
            .provider_schema_uses
            .iter()
            .map(|use_| (&[] as &[helm_schema_core::ConditionalGuard], use_))
            .chain(overlay_uses)
        {
            if !use_.range_key || use_.kind != ValueKind::Scalar {
                continue;
            }
            let Some(fragment) = provider.schema_fragment_for_use(use_) else {
                continue;
            };
            if crate::overlay_lowering::schema_runtime_types(fragment.schema())
                != std::collections::BTreeSet::from(["string"])
            {
                continue;
            }
            let mut requirements = vec![FailValueRequirement::SchemaType("string".to_string())];
            // The slot's own string constraints hold for every rendered
            // key, so they project onto the collection's key domain
            // (traefik's Gateway listener names must spell the CRD's
            // lowercase SectionName). Only top-level keywords project:
            // union-shaped fragments keep the plain string typing.
            if let Some(pattern) = fragment.schema().get("pattern").and_then(Value::as_str) {
                requirements.push(FailValueRequirement::MatchesPattern {
                    pattern: pattern.to_string(),
                    templated: false,
                });
            }
            let min = fragment.schema().get("minLength").and_then(Value::as_u64);
            let max = fragment.schema().get("maxLength").and_then(Value::as_u64);
            if min.is_some() || max.is_some() {
                requirements.push(FailValueRequirement::StringLengthBounds { min, max });
            }
            // No self-truthy guard: the Keys encoding itself leaves the
            // empty-array, null, and (vacuously) absent lanes open, and a
            // self-truthy guard would trip the base-replacement rule for
            // self-guarded arms, erasing the path's independent base facts.
            push_implication(
                &mut implications,
                value_path.clone(),
                ContractFailImplication {
                    outer_guards: branch_guards.to_vec(),
                    target: ContractRequirementTarget::Keys,
                    requirements,
                },
            );
        }
    }
    implications
}

fn push_implication(
    implications: &mut BTreeMap<String, Vec<ContractFailImplication>>,
    target_value_path: String,
    implication: ContractFailImplication,
) {
    let entries = implications.entry(target_value_path).or_default();
    if !entries.contains(&implication) {
        entries.push(implication);
    }
}
