pub(crate) fn member_descendant_projection(
    target_segments: &[String],
    descendants: &[&ResolvedPathSchema],
    insertion_abstentions: &mut usize,
) -> Option<Value> {
    let mut member_schema = descendants
        .iter()
        .filter_map(|descendant| {
            let relative = descendant.path_segments.strip_prefix(target_segments)?;
            (relative == ["*"]
                && !crate::schema_model::is_empty_schema(&descendant.structural_schema))
            .then(|| descendant.structural_schema.clone())
        })
        .reduce(crate::merge::merge_two_schemas);
    let mut has_descendant = member_schema.is_some();

    for descendant in descendants {
        let Some(relative) = descendant.path_segments.strip_prefix(target_segments) else {
            continue;
        };
        let Some(("*", tail)) = relative
            .split_first()
            .map(|(head, tail)| (head.as_str(), tail))
        else {
            continue;
        };
        if tail.is_empty() {
            continue;
        }
        has_descendant = true;
        let current = member_schema
            .take()
            .unwrap_or_else(|| SchemaNode::untyped_member_host().into_value());
        let (inserted, abstentions) = crate::schema_tree::insert_path_schema_value(
            current,
            tail,
            descendant.structural_schema.clone(),
        );
        *insertion_abstentions += abstentions;
        member_schema = Some(inserted);
    }
    if !has_descendant {
        return None;
    }
    Some(member_schema.unwrap_or_else(|| SchemaNode::untyped_member_host().into_value()))
}

fn structural_collection_member_projection(schema: &Value) -> Option<Value> {
    if crate::schema_model::is_empty_schema(schema) {
        return None;
    }
    let object = schema.as_object()?;

    for keyword in ["anyOf", "oneOf"] {
        let Some(arms) = object.get(keyword).and_then(Value::as_array) else {
            continue;
        };
        let members = arms
            .iter()
            .filter_map(structural_collection_member_projection)
            .collect::<Vec<_>>();
        if members.is_empty() {
            return None;
        }
        if members.iter().any(crate::schema_model::is_empty_schema) {
            return Some(crate::schema_model::empty_schema());
        }
        return Some(
            SchemaNode::any_of(members.into_iter().map(SchemaNode::foreign).collect()).into_value(),
        );
    }

    if let Some(arms) = object.get("allOf").and_then(Value::as_array) {
        let members = arms
            .iter()
            .filter_map(structural_collection_member_projection)
            .filter(|member| !crate::schema_model::is_empty_schema(member))
            .collect::<Vec<_>>();
        if !members.is_empty() {
            return Some(
                SchemaNode::all_of(members.into_iter().map(SchemaNode::foreign).collect())
                    .into_value(),
            );
        }
    }

    let schema_types = match object.get("type") {
        Some(Value::String(schema_type)) => vec![schema_type.as_str()],
        Some(Value::Array(schema_types)) => schema_types
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let object_lane = schema_types.contains(&"object")
        || (schema_types.is_empty()
            && object.keys().any(|keyword| {
                matches!(
                    keyword.as_str(),
                    "additionalProperties"
                        | "maxProperties"
                        | "minProperties"
                        | "patternProperties"
                        | "properties"
                        | "propertyNames"
                        | "required"
                )
            }));
    let array_lane = schema_types.contains(&"array")
        || (schema_types.is_empty()
            && object.keys().any(|keyword| {
                matches!(
                    keyword.as_str(),
                    "additionalItems"
                        | "contains"
                        | "items"
                        | "maxItems"
                        | "minItems"
                        | "prefixItems"
                        | "uniqueItems"
                )
            }));
    if !object_lane && !array_lane {
        return None;
    }

    let mut members = Vec::new();
    if object_lane {
        let member = match object.get("additionalProperties") {
            Some(Value::Bool(_)) | None => crate::schema_model::empty_schema(),
            Some(schema) => schema.clone(),
        };
        members.push(member);
    }
    if array_lane {
        let member = match object.get("items") {
            Some(schema) if !schema.is_boolean() && !schema.is_array() => schema.clone(),
            _ => crate::schema_model::empty_schema(),
        };
        members.push(member);
    }
    if members.iter().any(crate::schema_model::is_empty_schema) {
        return Some(crate::schema_model::empty_schema());
    }
    match members.as_slice() {
        [] => None,
        [member] => Some(member.clone()),
        _ => Some(
            SchemaNode::any_of(members.into_iter().map(SchemaNode::foreign).collect()).into_value(),
        ),
    }
}

/// Per-key arms for members a guard-scoped `omit` may remove before the
/// sink reads the map: the whole-payload projection subtracts them, and
/// each key whose RETAIN guards lowered comes back as
/// `if retain-guards then map.key matches the provider's member schema`
/// (external-secrets' `adaptSecurityContext` — `runAsUser` stays
/// integer-typed exactly where the `OpenShift` adaptation certainly does
/// not run). Keys without retain guards stay subtracted: their survival
/// is undecidable, so their typing abstains.
fn append_omitted_member_arms(
    conditionals: &mut Vec<LoweredConjunct>,
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
            conditionals.push(LoweredConjunct::schema(
                EmissionOrigin::OmittedMember,
                ConditionalFlavor::Ordinary,
                value_path.clone(),
                Vec::new(),
                target_segments.clone(),
                guards,
                Vec::new(),
                serde_json::json!({
                    "properties": { member: member_schema }
                }),
                None,
                ConditionalBaseEffect::None,
                false,
            ));
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
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
fn append_merge_shadow_arms(
    conditionals: &mut Vec<LoweredConjunct>,
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
                if merge.own_transform() == helm_schema_core::MergeLayerTransform::NilScrubbed {
                    null_relax_member_schemas(&mut whole);
                }
                let own_guard = match merge.own_transform() {
                    helm_schema_core::MergeLayerTransform::ParsedMap => ConditionalGuard::TypeIs {
                        path: value_path.clone(),
                        schema_type: "object".to_string(),
                    },
                    helm_schema_core::MergeLayerTransform::Identity
                    | helm_schema_core::MergeLayerTransform::NilScrubbed => {
                        ConditionalGuard::Truthy {
                            path: value_path.clone(),
                        }
                    }
                };
                let mut guards = vec![own_guard];
                guards.extend(
                    merge
                        .shadowed_by()
                        .iter()
                        .enumerate()
                        .map(|(position, earlier)| {
                            let earlier_live = match merge
                                .transforms
                                .get(position)
                                .copied()
                                .unwrap_or(helm_schema_core::MergeLayerTransform::Identity)
                            {
                                helm_schema_core::MergeLayerTransform::ParsedMap => {
                                    ConditionalGuard::AllOf(vec![
                                        ConditionalGuard::TypeIs {
                                            path: earlier.clone(),
                                            schema_type: "object".to_string(),
                                        },
                                        ConditionalGuard::Truthy {
                                            path: earlier.clone(),
                                        },
                                    ])
                                }
                                helm_schema_core::MergeLayerTransform::Identity
                                | helm_schema_core::MergeLayerTransform::NilScrubbed => {
                                    ConditionalGuard::Truthy {
                                        path: earlier.clone(),
                                    }
                                }
                            };
                            ConditionalGuard::Not(Box::new(earlier_live))
                        }),
                );
                guards.extend(provider_use.outer_guards.iter().cloned());
                guards.sort();
                guards.dedup();
                let base_effect = if evidence.facts.has_unlayered_non_control_use {
                    ConditionalBaseEffect::Preserve
                } else {
                    ConditionalBaseEffect::Own
                };
                conditionals.push(LoweredConjunct::schema(
                    EmissionOrigin::MergeShadow,
                    ConditionalFlavor::Ordinary,
                    value_path.clone(),
                    Vec::new(),
                    target_segments.clone(),
                    guards,
                    Vec::new(),
                    whole,
                    None,
                    base_effect,
                    false,
                ));
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
                if merge.own_transform() == helm_schema_core::MergeLayerTransform::NilScrubbed {
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
                conditionals.push(LoweredConjunct::schema(
                    EmissionOrigin::MergeShadow,
                    ConditionalFlavor::Ordinary,
                    value_path.clone(),
                    Vec::new(),
                    target_segments.clone(),
                    guards,
                    Vec::new(),
                    target_schema,
                    None,
                    ConditionalBaseEffect::None,
                    false,
                ));
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
                    | helm_schema_core::ContractRequirementTarget::MembersExceptKeys { .. }
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
        ContractRequirementTarget::Members { allow_integer }
        | ContractRequirementTarget::MembersExceptKeys { allow_integer, .. }
        | ContractRequirementTarget::MembersAt { allow_integer, .. }
        | ContractRequirementTarget::MembersAtWhereTruthy { allow_integer, .. } => {
            let mut types = BTreeSet::from(["array", "null", "object"]);
            if *allow_integer {
                types.insert("integer");
            }
            types
        }
        ContractRequirementTarget::MembersMatchingPrefix { .. }
        | ContractRequirementTarget::MembersWhereEquals { .. } => {
            BTreeSet::from(["array", "null", "object"])
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
        FailValueRequirement::TruthyImpliesSchemaType(_)
        | FailValueRequirement::HelmFalsy
        | FailValueRequirement::FieldHelmFalsy { .. }
        | FailValueRequirement::FieldNotEquals { .. }
        | FailValueRequirement::NotEquals(_)
        // Constrains rendered content, not the value's kind (non-strings
        // format as safe plain tokens, and every string kind has token-safe
        // inhabitants).
        | FailValueRequirement::QuotedSerializationSafe { .. }
        | FailValueRequirement::PlainScalarSafe { .. } => true,
        FailValueRequirement::PrintfStringOperand => {
            matches!(runtime_type, "object" | "string")
        }
        FailValueRequirement::HelmTruthy => runtime_type != "null",
        FailValueRequirement::FieldEquals { .. }
        | FailValueRequirement::FieldPresentNotNull { .. }
        | FailValueRequirement::FieldHelmTruthy { .. }
        | FailValueRequirement::HasMember(_)
        | FailValueRequirement::HasMemberEvenDefaulted(_) => runtime_type == "object",
        FailValueRequirement::NotSchemaType(rejected) => {
            runtime_type != rejected && !(rejected == "number" && runtime_type == "integer")
        }
        FailValueRequirement::MatchesPattern { .. }
        | FailValueRequirement::NotMatchesPattern { .. }
        | FailValueRequirement::StringLengthBounds { .. } => runtime_type == "string",
        FailValueRequirement::MemberHost { handled_kinds, .. } => {
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

