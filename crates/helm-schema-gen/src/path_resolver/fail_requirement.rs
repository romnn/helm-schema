/// Schema for `fail`-branch requirements. Non-member requirements accept
/// null alongside the demanded type: fail tests routinely sit behind
/// `default`-chained locals, where a null input takes the fallback and
/// renders. Member requirements stay exact (a null member value really
/// aborts).
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
pub(crate) fn fail_requirement_schema<'a>(
    implications: impl IntoIterator<Item = &'a helm_schema_core::ContractFailImplication>,
) -> (Value, usize) {
    let mut parts = Vec::new();
    let mut insertion_abstentions = 0;
    for implication in implications {
        let requirement = fail_value_requirement_schema(
            &implication.requirements,
            !matches!(
                &implication.target,
                helm_schema_core::ContractRequirementTarget::Value
            ),
        );
        if is_empty_schema(&requirement) {
            continue;
        }
        match &implication.target {
            helm_schema_core::ContractRequirementTarget::Value => parts.push(requirement),
            helm_schema_core::ContractRequirementTarget::Members { allow_integer } => {
                let mut arms = vec![
                    serde_json::json!({ "type": "array", "items": requirement }),
                    serde_json::json!({
                        "type": "object",
                        "additionalProperties": requirement,
                    }),
                ];
                if *allow_integer {
                    let integer =
                        if requirements_allow_runtime_kind(&implication.requirements, "integer") {
                            serde_json::json!({ "type": "integer" })
                        } else {
                            // Nonpositive integer ranges execute no iterations,
                            // so no member reaches the body requirement.
                            serde_json::json!({ "type": "integer", "maximum": 0 })
                        };
                    arms.push(integer);
                }
                arms.push(serde_json::json!({ "type": "null" }));
                parts.push(serde_json::json!({ "anyOf": arms }));
            }
            helm_schema_core::ContractRequirementTarget::MembersExceptKeys {
                keys,
                allow_integer,
            } => {
                let properties = keys
                    .iter()
                    .map(|key| (key.clone(), serde_json::json!({})))
                    .collect::<serde_json::Map<_, _>>();
                let mut arms = vec![
                    serde_json::json!({ "type": "array", "items": requirement }),
                    serde_json::json!({
                        "type": "object",
                        "properties": properties,
                        "additionalProperties": requirement,
                    }),
                ];
                if *allow_integer {
                    let integer =
                        if requirements_allow_runtime_kind(&implication.requirements, "integer") {
                            serde_json::json!({ "type": "integer" })
                        } else {
                            serde_json::json!({ "type": "integer", "maximum": 0 })
                        };
                    arms.push(integer);
                }
                arms.push(serde_json::json!({ "type": "null" }));
                parts.push(serde_json::json!({ "anyOf": arms }));
            }
            helm_schema_core::ContractRequirementTarget::MembersMatchingPrefix { prefix } => {
                let pattern = format!("^{}", helm_schema_core::escape_regex_literal(prefix));
                parts.push(serde_json::json!({
                    "anyOf": [
                        {
                            "type": "object",
                            "patternProperties": { (pattern): requirement },
                        },
                        { "type": "array", "maxItems": 0 },
                        { "type": "null" },
                    ]
                }));
            }
            helm_schema_core::ContractRequirementTarget::MembersAt {
                target_path,
                allow_integer,
            }
            | helm_schema_core::ContractRequirementTarget::MembersAtWhereTruthy {
                target_path,
                allow_integer,
                ..
            } => {
                // Nil-tolerant requirements (comparison operands, and
                // truthy-scoped types whose absent leaf is falsy and escapes
                // the consumer) hold only when the leaf is present, so the
                // wrapper must not demand the field itself. A
                // `NotSchemaType` requirement is likewise satisfied by an
                // absent leaf: the failing test it negates fired only on
                // values OF that type (a quoted-token splice constrains
                // strings; members without the field never render it).
                let tolerant_leaf = implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::ComparableKind(_)
                            | helm_schema_core::FailValueRequirement::TruthyImpliesSchemaType(_)
                            | helm_schema_core::FailValueRequirement::NotEquals(_)
                    )
                }) || implication.requirements.iter().any(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::NotSchemaType(_)
                    )
                });
                let member = match &implication.target {
                    // A per-member selector binds only the members the
                    // chart's own gate routes to the consumer; every other
                    // member — and every non-object one, which cannot hold
                    // the selector field — passes the arm unconstrained. The
                    // gated leaf stays OPTIONAL: the selector says which
                    // members reach the consumer, not that the consumer
                    // aborts when its operand is missing, which is the
                    // separate absence claim's fact (an empty target path
                    // carries exactly that claim, as its own required
                    // member).
                    helm_schema_core::ContractRequirementTarget::MembersAtWhereTruthy {
                        guard_path,
                        ..
                    } => serde_json::json!({
                        "if": required_object_path_schema(
                            guard_path,
                            crate::condition_encoding::helm_truthy_condition_schema().into_value(),
                        ),
                        "then": optional_leaf_object_path_schema(target_path, requirement),
                    }),
                    _ if tolerant_leaf => {
                        optional_leaf_object_path_schema(target_path, requirement)
                    }
                    _ => required_object_path_schema(target_path, requirement),
                };
                let mut arms = vec![
                    serde_json::json!({ "type": "array", "items": member }),
                    serde_json::json!({
                        "type": "object",
                        "additionalProperties": member,
                    }),
                ];
                if *allow_integer {
                    // Integer iteration yields int members, which can never
                    // host the required field; only zero iterations pass.
                    arms.push(serde_json::json!({ "type": "integer", "maximum": 0 }));
                }
                arms.push(serde_json::json!({ "type": "null" }));
                parts.push(serde_json::json!({ "anyOf": arms }));
            }
            helm_schema_core::ContractRequirementTarget::MembersWhereEquals {
                guard_path,
                value,
                target_path,
            } => {
                let Some(value) = serde_json::to_value(value).ok() else {
                    continue;
                };
                let guard =
                    required_object_path_schema(guard_path, serde_json::json!({ "const": value }));
                let (target, abstentions) = crate::schema_tree::insert_path_schema_value(
                    empty_schema(),
                    target_path,
                    requirement,
                );
                insertion_abstentions += abstentions;
                let member = serde_json::json!({ "if": guard, "then": target });
                parts.push(serde_json::json!({
                    "anyOf": [
                        { "type": "array", "items": member },
                        { "type": "object", "additionalProperties": member },
                        { "type": "null" },
                    ]
                }));
            }
            helm_schema_core::ContractRequirementTarget::Keys => {
                let mut object =
                    if requirements_allow_runtime_kind(&implication.requirements, "string") {
                        serde_json::json!({ "type": "object" })
                    } else {
                        serde_json::json!({ "type": "object", "maxProperties": 0 })
                    };
                // Pattern requirements constrain each KEY's spelling
                // (traefik's uppercase gate); string keys are structural in
                // YAML maps, so only the pattern itself needs encoding.
                let mut key_schemas = Vec::new();
                for requirement in &implication.requirements {
                    match requirement {
                        helm_schema_core::FailValueRequirement::MatchesPattern {
                            pattern,
                            templated: false,
                        } => {
                            if let Some(pattern) = ecma_compatible_pattern(pattern) {
                                key_schemas.push(serde_json::json!({ "pattern": pattern }));
                            }
                        }
                        helm_schema_core::FailValueRequirement::NotMatchesPattern { pattern } => {
                            if let Some(pattern) = ecma_compatible_pattern(pattern) {
                                key_schemas
                                    .push(serde_json::json!({ "not": { "pattern": pattern } }));
                            }
                        }
                        helm_schema_core::FailValueRequirement::StringLengthBounds { min, max } => {
                            let mut bounds = serde_json::Map::new();
                            if let Some(min) = min {
                                bounds.insert("minLength".to_string(), serde_json::json!(min));
                            }
                            if let Some(max) = max {
                                bounds.insert("maxLength".to_string(), serde_json::json!(max));
                            }
                            key_schemas.push(Value::Object(bounds));
                        }
                        // Keys are always strings, so the exclusions apply
                        // directly rather than through the non-string escape.
                        helm_schema_core::FailValueRequirement::PlainScalarSafe {
                            token_initial,
                            ..
                        } => {
                            key_schemas.push(serde_json::json!({
                                "allOf": crate::resolve_policy::plain_scalar_structural_exclusions(
                                    *token_initial,
                                )
                            }));
                        }
                        _ => {}
                    }
                }
                if let Some(object) = object.as_object_mut() {
                    match key_schemas.len() {
                        0 => {}
                        1 => {
                            object.insert(
                                "propertyNames".to_string(),
                                key_schemas.pop().unwrap_or_else(|| serde_json::json!({})),
                            );
                        }
                        _ => {
                            object.insert(
                                "propertyNames".to_string(),
                                serde_json::json!({ "allOf": key_schemas }),
                            );
                        }
                    }
                }
                let array = if requirements_allow_runtime_kind(&implication.requirements, "integer")
                {
                    serde_json::json!({ "type": "array" })
                } else {
                    // An empty array never evaluates the range body, so no
                    // integer key reaches the strict consumer.
                    serde_json::json!({ "type": "array", "maxItems": 0 })
                };
                parts.push(serde_json::json!({
                    "anyOf": [object, array, { "type": "null" }]
                }));
            }
        }
    }
    (merge_schema_list(parts), insertion_abstentions)
}

/// Like [`required_object_path_schema`], but the LEAF member stays
/// optional: nil-tolerant requirements (comparison operands) constrain the
/// field only when it is present. Intermediate segments stay required
/// because field access through an absent parent aborts rendering with a
/// nil-pointer error before the tolerant leaf comparison runs.
fn requirements_allow_runtime_kind(
    requirements: &[helm_schema_core::FailValueRequirement],
    schema_type: &str,
) -> bool {
    use helm_schema_core::FailValueRequirement;

    requirements.iter().all(|requirement| match requirement {
        FailValueRequirement::SchemaType(required)
        // Null is asserted away before Sprig's missing-key handling runs.
        | FailValueRequirement::SchemaTypeEvenNull(required) => required == schema_type,
        // Every runtime kind has a Helm-falsy spelling that escapes the
        // consumer, so the truthy-scoped requirement excludes no kind.
        FailValueRequirement::TruthyImpliesSchemaType(_)
        // Every runtime kind has a Helm-falsy spelling.
        | FailValueRequirement::HelmFalsy
        | FailValueRequirement::NotEquals(_)
        // Applies only to present fields on objects; every other kind
        // passes vacuously (a missing field differs from every literal).
        | FailValueRequirement::FieldNotEquals { .. }
        // The requirement constrains rendered CONTENT, not the value's kind
        // (non-strings format as safe plain tokens, and every string kind
        // has token-safe inhabitants).
        | FailValueRequirement::QuotedSerializationSafe { .. }
        | FailValueRequirement::PlainScalarSafe { .. }
        // The field constraint applies only to objects carrying the field;
        // every other kind passes vacuously.
        | FailValueRequirement::FieldHelmFalsy { .. } => true,
        // Every runtime kind except null has truthy inhabitants.
        FailValueRequirement::HelmTruthy => schema_type != "null",
        FailValueRequirement::ComparableKind(required) => {
            required == schema_type || schema_type == "null"
        }
        FailValueRequirement::PrintfStringOperand => {
            matches!(schema_type, "object" | "string")
        }
        FailValueRequirement::NotSchemaType(rejected) => rejected != schema_type,
        FailValueRequirement::MatchesPattern { .. }
        | FailValueRequirement::NotMatchesPattern { .. }
        | FailValueRequirement::StringLengthBounds { .. } => schema_type == "string",
        FailValueRequirement::Iterable { allow_integer } => {
            matches!(schema_type, "array" | "object" | "null")
                || schema_type == "integer" && *allow_integer
        }
        FailValueRequirement::HasMember(_)
        | FailValueRequirement::HasMemberEvenDefaulted(_)
        | FailValueRequirement::FieldEquals { .. }
        // Presence of a (truthy or non-null) field needs an object host.
        | FailValueRequirement::FieldPresentNotNull { .. }
        | FailValueRequirement::FieldHelmTruthy { .. } => schema_type == "object",
        FailValueRequirement::MemberHost { handled_kinds, .. } => {
            schema_type == "object" || handled_kinds.iter().any(|kind| kind == schema_type)
        }
        FailValueRequirement::IndexableAt(_) => matches!(schema_type, "array" | "string"),
        FailValueRequirement::SplitSegmentsAtLeast {
            allow_non_string, ..
        } => schema_type == "string" || *allow_non_string,
        FailValueRequirement::AnyOf(alternatives) => alternatives
            .iter()
            .any(|alternative| requirements_allow_runtime_kind(alternative, schema_type)),
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
fn fail_value_requirement_schema(
    requirements: &[helm_schema_core::FailValueRequirement],
    per_member: bool,
) -> Value {
    use helm_schema_core::FailValueRequirement;
    let mut parts = Vec::new();
    let mut required_members: Vec<&str> = Vec::new();
    for requirement in requirements {
        match requirement {
            FailValueRequirement::SchemaType(schema_type) => {
                if per_member {
                    parts.push(type_schema(schema_type));
                } else {
                    parts.push(crate::schema_model::type_union_schema([
                        schema_type.as_str(),
                        "null",
                    ]));
                }
            }
            // No null tolerance: the consumer type-asserts before its nil
            // handling, so an explicit null aborts (absence stays open
            // through the arm's properties anchoring).
            FailValueRequirement::SchemaTypeEvenNull(schema_type) => {
                parts.push(type_schema(schema_type));
            }
            // Only truthy values reach the consumer; every Helm-falsy
            // spelling escapes through the selection and stays accepted.
            FailValueRequirement::TruthyImpliesSchemaType(schema_type) => {
                parts.push(serde_json::json!({
                    "anyOf": [
                        type_schema(schema_type),
                        { "not": { "$ref": format!(
                            "#/$defs/{}",
                            crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                        ) } },
                    ]
                }));
            }
            FailValueRequirement::HelmTruthy => {
                parts.push(serde_json::json!({ "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) }));
            }
            FailValueRequirement::HelmFalsy => {
                parts.push(serde_json::json!({ "not": { "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) } }));
            }
            // `properties` constrains only PRESENT keys on objects — an
            // absent or null field differs from every literal, so no
            // `required` rides along.
            FailValueRequirement::FieldNotEquals { path, value } => {
                let Some(value) = guard_value_to_json(value) else {
                    continue;
                };
                let mut node = serde_json::json!({ "not": { "const": value } });
                for segment in path.iter().rev() {
                    node = serde_json::json!({ "properties": { segment: node } });
                }
                parts.push(node);
            }
            // `properties` constrains only PRESENT keys on objects, which
            // is exactly the tolerance the negated truthiness test needs:
            // an absent or falsy field renders, a truthy one aborts.
            FailValueRequirement::FieldHelmFalsy { path } => {
                let mut node = serde_json::json!({ "not": { "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) } });
                for segment in path.iter().rev() {
                    node = serde_json::json!({ "properties": { segment: node } });
                }
                parts.push(node);
            }
            FailValueRequirement::NotEquals(value) => {
                let Some(value) = guard_value_to_json(value) else {
                    continue;
                };
                parts.push(serde_json::json!({ "not": { "const": value } }));
            }
            // Nil compares, so a null member is as valid as an absent one.
            FailValueRequirement::ComparableKind(schema_type) => {
                parts.push(crate::schema_model::type_union_schema([
                    schema_type.as_str(),
                    "null",
                ]));
            }
            FailValueRequirement::NotSchemaType(schema_type) => {
                parts.push(serde_json::json!({ "not": type_schema(schema_type) }));
            }
            FailValueRequirement::HasMember(member)
            | FailValueRequirement::HasMemberEvenDefaulted(member) => {
                required_members.push(member);
            }
            FailValueRequirement::MatchesPattern { pattern, templated } => {
                // JSON Schema patterns are ECMA 262; abstaining on an
                // untranslatable Go/RE2 pattern only widens the arm back
                // to its other requirements.
                if let Some(pattern) = ecma_compatible_pattern(pattern) {
                    let matches = serde_json::json!({ "type": "string", "pattern": pattern });
                    if *templated {
                        // The pattern constrains `tpl`'s OUTPUT: a raw value
                        // carrying a template action renders to something
                        // that may match, so admit it alongside the
                        // action-free strings the pattern already accepts.
                        parts.push(serde_json::json!({
                            "anyOf": [
                                matches,
                                { "type": "string", "pattern": "\\{\\{" },
                            ]
                        }));
                    } else {
                        parts.push(matches);
                    }
                }
            }
            FailValueRequirement::NotMatchesPattern { pattern } => {
                // Abstaining on an untranslatable pattern only widens the
                // arm back to its other requirements, as for MatchesPattern.
                if let Some(pattern) = ecma_compatible_pattern(pattern) {
                    parts.push(serde_json::json!({
                        "type": "string",
                        "not": { "pattern": pattern },
                    }));
                }
            }
            FailValueRequirement::StringLengthBounds { min, max } => {
                let mut bounds = serde_json::Map::new();
                bounds.insert("type".to_string(), serde_json::json!("string"));
                if let Some(min) = min {
                    bounds.insert("minLength".to_string(), serde_json::json!(min));
                }
                if let Some(max) = max {
                    bounds.insert("maxLength".to_string(), serde_json::json!(max));
                }
                parts.push(Value::Object(bounds));
            }
            FailValueRequirement::MemberHost { handled_kinds, .. } => {
                let types = std::iter::once("object").chain(
                    handled_kinds
                        .iter()
                        .filter(|kind| kind.as_str() != "object")
                        .map(String::as_str),
                );
                parts.push(crate::schema_model::type_union_schema(types));
            }
            FailValueRequirement::Iterable { allow_integer } => {
                parts.push(crate::runtime_iterable_schema(*allow_integer));
            }
            FailValueRequirement::IndexableAt(index) => {
                parts.push(serde_json::json!({
                    "anyOf": [
                        { "type": "array", "minItems": index + 1 },
                        { "type": "string" },
                    ]
                }));
            }
            FailValueRequirement::SplitSegmentsAtLeast {
                separator,
                segments,
                allow_non_string,
            } => {
                let occurrences = segments.saturating_sub(1);
                let pattern = format!(
                    "^(?:[\\s\\S]*{}){{{occurrences}}}",
                    helm_schema_core::escape_regex_literal(separator)
                );
                let string = serde_json::json!({ "type": "string", "pattern": pattern });
                if *allow_non_string {
                    parts.push(serde_json::json!({
                        "anyOf": [string, { "not": { "type": "string" } }]
                    }));
                } else {
                    parts.push(string);
                }
            }
            FailValueRequirement::QuotedSerializationSafe { style, templated } => {
                parts.push(crate::quoted_serialization::reference_schema(
                    *style, *templated,
                ));
            }
            FailValueRequirement::PrintfStringOperand => {
                parts.push(serde_json::json!({
                    "anyOf": [
                        crate::resolve_policy::printf_string_formattable_string_schema(),
                        crate::resolve_policy::printf_string_formattable_mapping_schema(),
                    ]
                }));
            }
            FailValueRequirement::PlainScalarSafe {
                token_initial,
                templated,
            } => {
                parts.push(plain_scalar_safe_schema(*token_initial, *templated));
            }
            // Presence rides the equality (Go's `eq` aborts on nil), so
            // every wrapping level requires its segment and the host must
            // be an object.
            FailValueRequirement::FieldEquals { path, value } => {
                let Some(value) = guard_value_to_json(value) else {
                    continue;
                };
                let mut node = serde_json::json!({ "const": value });
                for segment in path.iter().rev() {
                    node = serde_json::json!({
                        "type": "object",
                        "required": [segment],
                        "properties": { segment: node },
                    });
                }
                parts.push(node);
            }
            // Presence rides along both field forms: an absent field is
            // null-rendered (not-null fails) and falsy (truthy fails), so
            // every wrapping level requires its segment.
            FailValueRequirement::FieldPresentNotNull { path } => {
                let mut node = serde_json::json!({ "not": { "type": "null" } });
                for segment in path.iter().rev() {
                    node = serde_json::json!({
                        "type": "object",
                        "required": [segment],
                        "properties": { segment: node },
                    });
                }
                parts.push(node);
            }
            FailValueRequirement::FieldHelmTruthy { path } => {
                let mut node = serde_json::json!({ "$ref": format!(
                    "#/$defs/{}",
                    crate::condition_encoding::HELM_TRUTHY_DEFINITION_NAME
                ) });
                for segment in path.iter().rev() {
                    node = serde_json::json!({
                        "type": "object",
                        "required": [segment],
                        "properties": { segment: node },
                    });
                }
                parts.push(node);
            }
            FailValueRequirement::AnyOf(alternatives) => {
                let arms: Vec<Value> = alternatives
                    .iter()
                    .map(|alternative| fail_value_requirement_schema(alternative, per_member))
                    .collect();
                // Field-based alternatives all READ a member field, and a
                // field read aborts on non-object members, so the
                // alternation nests inside one object schema. Descendant
                // property insertions then merge as siblings of the inner
                // `anyOf` (a conjunction); emitting a bare `anyOf` would
                // let the union combiner treat the carrier as one more
                // arm and make the alternation vacuous.
                let field_based = alternatives.iter().flatten().all(|requirement| {
                    matches!(
                        requirement,
                        FailValueRequirement::HasMember(_)
                            | FailValueRequirement::FieldEquals { .. }
                            | FailValueRequirement::FieldNotEquals { .. }
                            | FailValueRequirement::FieldHelmFalsy { .. }
                            | FailValueRequirement::FieldPresentNotNull { .. }
                            | FailValueRequirement::FieldHelmTruthy { .. }
                    )
                });
                if field_based {
                    parts.push(serde_json::json!({ "type": "object", "anyOf": arms }));
                } else {
                    parts.push(serde_json::json!({ "anyOf": arms }));
                }
            }
        }
    }
    if !required_members.is_empty() {
        required_members.sort_unstable();
        required_members.dedup();
        parts.push(serde_json::json!({
            "type": "object",
            "required": required_members,
        }));
    }
    // Requirements are CONJUNCTIVE: they must all hold for the tested
    // value. `merge_schema_list` is the evidence-union combiner whose
    // fallback is `anyOf`, which would silently weaken a multi-requirement
    // implication (two not-equals arms union into a tautology).
    let mut conjuncts: Vec<Value> = Vec::new();
    for part in parts {
        if !conjuncts.contains(&part) {
            conjuncts.push(part);
        }
    }
    match conjuncts.len() {
        0 => empty_schema(),
        1 => conjuncts.remove(0),
        _ => serde_json::json!({ "allOf": conjuncts }),
    }
}
