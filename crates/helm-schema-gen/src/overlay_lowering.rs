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
use crate::emission_policy::{
    ConditionalFlavor, EmissionClass, EmissionOrigin, GuardScopes, NestedGuardScope, TerminalWhen,
};
use crate::emission_report::EmissionReport;
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::conditional_target_schema;
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::values_yaml::yaml_value_at_path;
use crate::{common_prefix_len, split_value_path};

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConditionalBaseEffect {
    /// This pure requirement does not participate in base ownership.
    None,
    /// The conditional domain owns the path and leaves no unconditional base.
    Own,
    /// The conditional domain owns the path beside a retained, unclosed base.
    Preserve,
    /// This incomplete domain does not own a base, but prevents an exact
    /// sibling domain from claiming completeness.
    Require,
}

#[derive(Debug, Clone)]
pub(crate) struct ConjunctCarrier {
    pub(crate) target_value_path: String,
    pub(crate) ancestor_segments: Vec<String>,
    pub(crate) relative_target_segments: Vec<String>,
    pub(crate) base_effect: ConditionalBaseEffect,
    /// Every member access on this target rides the nil-safe grouped form
    /// (`(.Values.x).member`), which renders at an absent or null-deleted
    /// receiver instead of aborting. The base host materialized for the
    /// target's descendants must then stay untyped — this arm alone carries
    /// the object requirement, scoped to the receiver's strict presence
    /// (nack's root `global`, read only through `((.Values.global).labels)`,
    /// renders at `global: null`).
    relax_untyped_host: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct LoweredConjunct {
    pub(crate) class: EmissionClass,
    pub(crate) origin: EmissionOrigin,
    pub(crate) carrier: ConjunctCarrier,
    pub(crate) schema: Value,
    pub(crate) provider_candidate: Option<ProviderSchemaCandidate>,
}

impl LoweredConjunct {
    #[expect(
        clippy::too_many_arguments,
        reason = "the constructor makes the complete lowered carrier and its policy class auditable at every producer"
    )]
    fn schema(
        origin: EmissionOrigin,
        flavor: ConditionalFlavor,
        target_value_path: String,
        ancestor_segments: Vec<String>,
        relative_target_segments: Vec<String>,
        guards: Vec<ConditionalGuard>,
        nested_guard_scopes: Vec<NestedGuardScope>,
        target_schema: Value,
        provider_schema_candidate: Option<ProviderSchemaCandidate>,
        base_effect: ConditionalBaseEffect,
        relax_untyped_host: bool,
    ) -> Self {
        let class = EmissionClass::conditional(
            GuardScopes::new(guards, nested_guard_scopes),
            &ancestor_segments,
            flavor,
        );
        Self {
            class,
            origin,
            carrier: ConjunctCarrier {
                target_value_path,
                ancestor_segments,
                relative_target_segments,
                base_effect,
                relax_untyped_host,
            },
            schema: target_schema,
            provider_candidate: provider_schema_candidate,
        }
    }

    pub(crate) fn terminal(guards: Vec<ConditionalGuard>) -> Self {
        let class = if guards.is_empty() {
            EmissionClass::terminal_always()
        } else {
            EmissionClass::terminal_guarded(guards).unwrap_or_else(EmissionClass::terminal_always)
        };
        Self {
            class,
            origin: EmissionOrigin::FailImplication,
            carrier: ConjunctCarrier {
                target_value_path: String::new(),
                ancestor_segments: Vec::new(),
                relative_target_segments: Vec::new(),
                base_effect: ConditionalBaseEffect::None,
                relax_untyped_host: false,
            },
            schema: Value::Bool(false),
            provider_candidate: None,
        }
    }

    fn guard_scopes(&self) -> Option<&GuardScopes> {
        match &self.class {
            EmissionClass::Conditional { guards, .. } => Some(guards),
            EmissionClass::Mandatory => Some(&EMPTY_GUARD_SCOPES),
            EmissionClass::Terminal { .. } => None,
        }
    }

    fn outer_guards(&self) -> &[ConditionalGuard] {
        self.guard_scopes()
            .map(|scopes| scopes.outer.as_slice())
            .unwrap_or_default()
    }

    fn nested_guard_scopes(&self) -> &[NestedGuardScope] {
        self.guard_scopes()
            .map(|scopes| scopes.nested.as_slice())
            .unwrap_or_default()
    }

    pub(crate) fn terminal_guards(&self) -> Option<&[ConditionalGuard]> {
        match &self.class {
            EmissionClass::Terminal {
                when: TerminalWhen::Always,
            } => Some(&[]),
            EmissionClass::Terminal {
                when: TerminalWhen::Guarded(scopes),
            } => Some(scopes.scopes().outer.as_slice()),
            EmissionClass::Mandatory | EmissionClass::Conditional { .. } => None,
        }
    }
}

static EMPTY_GUARD_SCOPES: GuardScopes = GuardScopes {
    outer: Vec::new(),
    nested: Vec::new(),
};

#[tracing::instrument(skip_all)]
#[expect(
    clippy::too_many_lines,
    reason = "keeping this semantic lowering operation together makes its state transitions easier to audit"
)]
pub(crate) fn collect_conditional_schemas(
    resolved_paths: &[ResolvedPathSchema],
    contract_schema_signals: &ContractSchemaSignals,
    values_yaml_doc: &YamlValue,
    subchart_defaults_doc: &YamlValue,
    provider: &dyn ResourceSchemaOracle,
) -> Vec<LoweredConjunct> {
    let mut synthesized_implications =
        crate::required_source_backprojection::synthesized_required_source_implications(
            contract_schema_signals,
            values_yaml_doc,
            subchart_defaults_doc,
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
                subchart_defaults_doc,
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
                .entry(resolved.path_segments.get(..star).unwrap_or_default())
                .or_default()
                .push(resolved);
        }
    }
    let mut conditionals = Vec::new();

    if let Some(root_implications) = synthesized_implications.get("") {
        for implication in root_implications {
            if !implication.outer_guards.is_empty()
                && !implication_guards_supported(&implication.outer_guards, "", &resolved_by_path)
            {
                continue;
            }
            let target_schema =
                crate::path_resolver::fail_requirement_schema(std::iter::once(implication));
            if crate::schema_model::is_empty_schema(&target_schema) {
                continue;
            }
            conditionals.push(LoweredConjunct::schema(
                EmissionOrigin::Backprojection,
                ConditionalFlavor::Ordinary,
                String::new(),
                Vec::new(),
                Vec::new(),
                implication.outer_guards.clone(),
                Vec::new(),
                target_schema,
                None,
                ConditionalBaseEffect::None,
                false,
            ));
        }
    }

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
        for (implication, origin) in evidence
            .fail_implications
            .iter()
            .map(|implication| (implication, EmissionOrigin::FailImplication))
            .chain(
                synthesized
                    .iter()
                    .map(|implication| (implication, EmissionOrigin::Backprojection)),
            )
        {
            if is_bare_iterable_implication(implication)
                && member_implication_covers_range_domain(
                    &evidence.fail_implications,
                    &implication.outer_guards,
                )
            {
                continue;
            }
            let member_host_only = !implication.requirements.is_empty()
                && implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::MemberHost { .. }
                    )
                });
            let member_host_complete_domain = member_host_only
                && implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::MemberHost {
                            complete_domain: true,
                            ..
                        }
                    )
                });
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
            // Abort-grade presence is exempt: the consumer aborts on an
            // absent subject, and under coalesced-document semantics a
            // default-supplied member is absent exactly when null-deleted
            // — the state the arm must reject (loki's `dig` subjects).
            let abort_grade_presence = implication.requirements.iter().all(|requirement| {
                matches!(
                    requirement,
                    helm_schema_core::FailValueRequirement::HasMemberEvenDefaulted(_)
                )
            });
            if !abort_grade_presence
                && matches!(
                    &implication.target,
                    helm_schema_core::ContractRequirementTarget::Value
                )
                && let Some(default) = yaml_value_at_path(values_yaml_doc, target_value_path)
            {
                relax_required_members_supplied_by_default(&mut target_schema, default);
            }
            let target_segments = split_value_path(target_value_path);
            if matches!(
                &implication.target,
                helm_schema_core::ContractRequirementTarget::Members { .. }
                    | helm_schema_core::ContractRequirementTarget::MembersExceptKeys { .. }
                    | helm_schema_core::ContractRequirementTarget::MembersWhereEquals { .. }
            ) && let Some(member_schema) = member_descendant_projection(
                target_segments.as_slice(),
                member_descendants
                    .get(target_segments.as_slice())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            ) {
                target_schema = crate::schema_tree::conjoin_collection_member_schema_value(
                    target_schema,
                    &member_schema,
                );
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
            // A member-access domain owns the declared fallback: outside its
            // exact arms the chart never navigates the host, so values.yaml
            // shape is not a runtime constraint. Only independent structural
            // evidence may retain the base beside those arms.
            let presence_scoped_type_arm =
                implication.requirements.iter().all(|requirement| {
                    matches!(
                        requirement,
                        helm_schema_core::FailValueRequirement::SchemaType(_)
                            | helm_schema_core::FailValueRequirement::SchemaTypeEvenNull(_)
                    )
                }) && implication_has_self_presence_guard(implication, target_value_path);
            // Only runtime structural evidence can retain a base beside a
            // guarded requirement. A compatible values.yaml sample does not
            // constrain states where an unrelated caller gate keeps every
            // consumer dormant.
            let resolved_domain = &resolved_target.structural_schema;
            let preserve_base_schema = (member_host_only && !member_host_complete_domain)
                || implication.outer_guards.is_empty()
                || (!implication_has_self_truthy_guard(implication, target_value_path)
                    && !presence_scoped_type_arm
                    && resolved_schema_admits_fail_requirement_domain(
                        resolved_domain,
                        implication,
                    ));
            let base_effect = if !preserve_base_schema {
                ConditionalBaseEffect::Own
            } else if member_host_only && !member_host_complete_domain {
                ConditionalBaseEffect::Require
            } else {
                ConditionalBaseEffect::None
            };
            conditionals.push(LoweredConjunct::schema(
                origin,
                ConditionalFlavor::Ordinary,
                target_value_path.clone(),
                ancestor_segments.clone(),
                target_segments
                    .get(ancestor_segments.len()..)
                    .unwrap_or_default()
                    .to_vec(),
                implication.outer_guards.clone(),
                Vec::new(),
                target_schema,
                None,
                base_effect,
                member_host_complete_domain && all_member_hosts_presence_scoped,
            ));
        }

        for source_overlay in &evidence.conditional_overlays {
            for partition in kind_partitioned_overlays(source_overlay) {
                let overlay = partition.overlay;
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
                let Some((outer_guards, nested_guard_scopes)) =
                    partition_guard_scopes(&target_segments, &overlay.guards)
                else {
                    continue;
                };
                let ancestor_segments = nested_guard_scopes
                    .first()
                    .filter(|_| outer_guards.is_empty())
                    .map(|scope| {
                        let mut parent = scope.ancestor_segments.clone();
                        parent.pop();
                        parent
                    })
                    .unwrap_or_else(|| {
                        conditional_ancestor_segments(&target_segments, &outer_guards)
                    });
                let active_by_defaults =
                    evaluate_guard_set_on_values(&overlay.guards, values_yaml_doc);
                let resolved_overlay =
                    resolve_overlay_target_schema(target_value_path, &overlay, provider);
                // The range header supplies the branch's complete runtime
                // domain. Its declared sample shape cannot remain as an
                // unconditional base without deleting valid map or integer
                // lanes while the range is active.
                let preserve_overlay_base = !overlay.evidence.facts.is_ranged_source
                    && (overlay.preserve_base_schema || has_unconditional_self_presence_contract);
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
                if member_implication_owns_range_domain {
                    // The fail implication already carries the branch's
                    // complete runtime domain. Keep only this empty ownership
                    // marker; passing an evidence-free overlay through
                    // conditional policy would substitute its values.yaml
                    // sample shape and re-type members the range accepts.
                    conditionals.push(LoweredConjunct::schema(
                        EmissionOrigin::Overlay,
                        partition.flavor,
                        target_value_path.clone(),
                        ancestor_segments.clone(),
                        target_segments
                            .get(ancestor_segments.len()..)
                            .unwrap_or_default()
                            .to_vec(),
                        outer_guards.clone(),
                        nested_guard_scopes.clone(),
                        crate::schema_model::empty_schema(),
                        None,
                        if preserve_overlay_base {
                            ConditionalBaseEffect::Preserve
                        } else {
                            ConditionalBaseEffect::Own
                        },
                        false,
                    ));
                    continue;
                }
                let range_allows_integer = !overlay.evidence.facts.has_structured_item_descendants
                    && !overlay.evidence.facts.has_destructured_range_use
                    && !overlay.evidence.facts.has_string_contract_items;
                let mut range_domain = crate::runtime_iterable_schema(range_allows_integer);
                let mut member_schemas = Vec::new();
                if let Some(member_schema) = member_descendant_projection(
                    target_segments.as_slice(),
                    member_descendants
                        .get(target_segments.as_slice())
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                ) {
                    member_schemas.push(member_schema);
                }
                if let Some(member_schema) =
                    structural_collection_member_projection(&resolved_target.structural_schema)
                {
                    member_schemas.push(member_schema);
                }
                if !member_schemas.is_empty() {
                    let member_schema = crate::merge::merge_schema_list(member_schemas);
                    range_domain = crate::schema_tree::conjoin_collection_member_schema_value(
                        range_domain,
                        &member_schema,
                    );
                }
                let branch_schema = if overlay.evidence.facts.has_self_range_guard_render_use {
                    // The render executes only after this subject's range
                    // header accepts it. That exact Helm input domain has
                    // priority over a provider backprojection from the loop
                    // body, which describes the emitted value but cannot
                    // make the already-running range reject its subject.
                    crate::merge::union_schema_list(vec![resolved_overlay.schema, range_domain])
                } else if overlay.evidence.facts.is_ranged_source {
                    crate::merge::merge_schema_list(vec![resolved_overlay.schema, range_domain])
                } else {
                    resolved_overlay.schema
                };
                let target_schema = conditional_target_schema(
                    target_value_path,
                    &overlay,
                    values_yaml_doc,
                    branch_schema,
                    &resolved_target.values_yaml_schema,
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
                        conditionals.push(LoweredConjunct::schema(
                            EmissionOrigin::Overlay,
                            partition.flavor,
                            target_value_path.clone(),
                            ancestor_segments.clone(),
                            target_segments
                                .get(ancestor_segments.len()..)
                                .unwrap_or_default()
                                .to_vec(),
                            outer_guards.clone(),
                            nested_guard_scopes.clone(),
                            target_schema,
                            None,
                            if preserve_overlay_base {
                                ConditionalBaseEffect::Preserve
                            } else {
                                ConditionalBaseEffect::Own
                            },
                            false,
                        ));
                    }
                    continue;
                }
                let provider_schema_candidate = resolved_overlay
                    .provider_schema_candidate
                    .filter(|candidate| candidate.survives_as(&target_schema));

                conditionals.push(LoweredConjunct::schema(
                    EmissionOrigin::Overlay,
                    partition.flavor,
                    target_value_path.clone(),
                    ancestor_segments.clone(),
                    target_segments
                        .get(ancestor_segments.len()..)
                        .unwrap_or_default()
                        .to_vec(),
                    outer_guards,
                    nested_guard_scopes,
                    target_schema,
                    provider_schema_candidate,
                    if preserve_overlay_base {
                        ConditionalBaseEffect::Preserve
                    } else {
                        ConditionalBaseEffect::Own
                    },
                    false,
                ));
            }
        }
    }

    append_merge_shadow_arms(&mut conditionals, contract_schema_signals, provider);
    append_omitted_member_arms(&mut conditionals, contract_schema_signals, provider);
    conditionals
}

fn member_descendant_projection(
    target_segments: &[String],
    descendants: &[&ResolvedPathSchema],
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
        member_schema = Some(crate::schema_tree::insert_path_schema_value(
            current,
            tail,
            descendant.structural_schema.clone(),
        ));
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

struct PartitionedOverlay {
    overlay: ConditionalPathOverlay,
    flavor: ConditionalFlavor,
}

fn kind_partitioned_overlays(overlay: &ConditionalPathOverlay) -> Vec<PartitionedOverlay> {
    let mut kinds = BTreeSet::new();
    for use_ in &overlay.evidence.provider_schema_uses {
        if !use_.resource.kind_candidates.is_empty() {
            kinds.insert(use_.resource.kind.clone());
            kinds.extend(use_.resource.kind_candidates.iter().cloned());
        }
    }
    if kinds.is_empty() {
        return vec![PartitionedOverlay {
            overlay: overlay.clone(),
            flavor: ConditionalFlavor::Ordinary,
        }];
    }
    let Some(selector) = kind_selector_path(&overlay.guards, &kinds) else {
        return vec![PartitionedOverlay {
            overlay: overlay.clone(),
            flavor: ConditionalFlavor::Ordinary,
        }];
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
            (!partition.evidence.provider_schema_uses.is_empty()).then_some(PartitionedOverlay {
                overlay: partition,
                flavor: ConditionalFlavor::KindPartition,
            })
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
            | ConditionalGuard::ContainsTruthyMember { .. }
            | ConditionalGuard::ContainsEquals { .. }
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

pub(crate) fn resolve_overlay_target_schema(
    target_value_path: &str,
    overlay: &ConditionalPathOverlay,
    provider: &dyn ResourceSchemaOracle,
) -> ResolvedPathSchema {
    let evidence = overlay.evidence.as_path_evidence(target_value_path);
    PathSchemaResolver::resolve_single_path_evidence(&evidence, provider)
}

fn partition_guard_scopes(
    target_segments: &[String],
    guards: &[ConditionalGuard],
) -> Option<(Vec<ConditionalGuard>, Vec<NestedGuardScope>)> {
    let mut outer_guards = Vec::new();
    let mut nested: BTreeMap<Vec<String>, Vec<ConditionalGuard>> = BTreeMap::new();

    for guard in guards {
        let mut member_anchor = None;
        let mut saw_document_path = false;
        for path in guard.value_paths() {
            let segments = split_value_path(&path);
            let Some(last_wildcard) = segments.iter().rposition(|segment| segment == "*") else {
                saw_document_path = true;
                continue;
            };
            let anchor = segments.get(..=last_wildcard)?.to_vec();
            if !target_segments.starts_with(&anchor)
                || member_anchor
                    .as_ref()
                    .is_some_and(|existing| existing != &anchor)
            {
                return None;
            }
            member_anchor = Some(anchor);
        }

        let Some(member_anchor) = member_anchor else {
            outer_guards.push(guard.clone());
            continue;
        };
        // One Boolean guard cannot read both a ranged member and an outer
        // document path from inside that member: JSON Schema has no upward
        // navigation. Expression lowering keeps ordinary conjunctions as
        // separate guards, so only genuinely inseparable formulas abstain.
        if saw_document_path {
            return None;
        }
        nested.entry(member_anchor).or_default().push(guard.clone());
    }

    outer_guards.sort();
    outer_guards.dedup();
    let mut nested_guard_scopes = nested
        .into_iter()
        .map(|(ancestor_segments, mut guards)| {
            guards.sort();
            guards.dedup();
            NestedGuardScope {
                ancestor_segments,
                guards,
            }
        })
        .collect::<Vec<_>>();
    nested_guard_scopes.sort_by_key(|scope| scope.ancestor_segments.len());
    if nested_guard_scopes.windows(2).any(|scopes| {
        let [outer, inner] = scopes else {
            return false;
        };
        !inner
            .ancestor_segments
            .starts_with(&outer.ancestor_segments)
    }) {
        return None;
    }

    Some((outer_guards, nested_guard_scopes))
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
            // The values ROOT owns no resolved path entry, but its
            // truthiness encodes at the document node itself. Rejecting it
            // here would drop the WHOLE implication — every sibling arm
            // with it — because an unsupported guard is fatal to the
            // any-of, which is how a root-scoped `with` cost nats the
            // member-host typing of the five hosts it navigates.
            ConditionalGuard::Truthy { path } | ConditionalGuard::With { path } => {
                path.is_empty()
                    || path == target_value_path
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
            | ConditionalGuard::ContainsTruthyMember { .. }
            | ConditionalGuard::ContainsEquals { .. }
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
            | ConditionalGuard::ContainsTruthyMember { .. }
            | ConditionalGuard::ContainsEquals { .. }
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
    values_default_sources: &BTreeSet<helm_schema_core::ValuesDefaultSource>,
    values_yaml_doc: &YamlValue,
    absence: crate::condition_encoding::AbsenceDefaults<'_>,
) {
    append_values_default_source_absence_clauses(root_schema, clauses, values_default_sources);
    // A deleted dependency values root is not the document minus a key:
    // helm recreates the table from the SUBCHART's own values, so a clause
    // whose guards all hold against that refill terminates every document
    // missing the root — the half a clause anchored inside the root cannot
    // reach. One clause per root states it.
    let mut deleted_roots = BTreeSet::new();
    for guards in clauses {
        if let Some(root) =
            crate::condition_encoding::deleted_dependency_root_terminates(guards, absence)
        {
            deleted_roots.insert(root);
        }
    }
    for root in deleted_roots {
        let Some(condition) = crate::condition_encoding::dependency_root_gone_condition(root)
        else {
            continue;
        };
        root_schema.append_conditional(&[], condition, SchemaNode::foreign(Value::Bool(false)));
    }
    for guards in clauses {
        let shared_ancestor = shared_guard_ancestor_segments(guards);
        let all_vacuous = guards.iter().all(guard_holds_vacuously);
        // Keep present values attributed to their nearest shared object.
        // A separate root clause evaluates the original formula only while
        // that object is missing; retaining every guard matters because a
        // negated presence test can make the formula false there.
        let split_vacuous_ancestor = all_vacuous
            && !shared_ancestor.is_empty()
            && !shared_ancestor.iter().any(|segment| segment == "*");
        let ancestor_segments = if split_vacuous_ancestor {
            shared_ancestor.clone()
        } else if all_vacuous {
            Vec::new()
        } else {
            shared_ancestor
        };
        if !guards
            .iter()
            .all(|guard| guard_encodes_fully(guard, &ancestor_segments, values_yaml_doc, absence))
        {
            continue;
        }
        let condition = SchemaNode::all_of(build_condition_clauses(
            guards,
            &ancestor_segments,
            values_yaml_doc,
            absence,
            crate::condition_encoding::ConditionPolarity::Narrow,
        ));
        root_schema.append_conditional(
            &ancestor_segments,
            condition,
            SchemaNode::foreign(Value::Bool(false)),
        );
        // `deeper_stage` is the effective subtree when the document supplies
        // no ancestor. A definitively false original formula makes the
        // missing-ancestor companion unreachable; uncertainty stays open.
        if split_vacuous_ancestor
            && evaluate_guard_set_on_values(guards, absence.deeper_stage) != Some(false)
        {
            let path = helm_schema_core::join_value_path(&ancestor_segments);
            let absent = ConditionalGuard::Absent { path };
            let root = Vec::new();
            let mut missing_ancestor_guards = guards.clone();
            missing_ancestor_guards.push(absent);
            if missing_ancestor_guards
                .iter()
                .all(|guard| guard_encodes_fully(guard, &root, values_yaml_doc, absence))
            {
                let condition = SchemaNode::all_of(build_condition_clauses(
                    &missing_ancestor_guards,
                    &root,
                    values_yaml_doc,
                    absence,
                    crate::condition_encoding::ConditionPolarity::Narrow,
                ));
                root_schema.append_conditional(
                    &root,
                    condition,
                    SchemaNode::foreign(Value::Bool(false)),
                );
            }
        }
    }
}

fn append_values_default_source_absence_clauses(
    root_schema: &mut SchemaDocument,
    clauses: &[Vec<ConditionalGuard>],
    values_default_sources: &BTreeSet<helm_schema_core::ValuesDefaultSource>,
) {
    for guards in clauses {
        let [ConditionalGuard::Absent { path }] = guards.as_slice() else {
            continue;
        };
        let Some(source_path) = unique_values_default_source_path(path, values_default_sources)
        else {
            continue;
        };
        let Some(target_absent) = crate::condition_encoding::input_path_absent_condition(path)
        else {
            continue;
        };
        let Some(source_absent) =
            crate::condition_encoding::input_path_absent_condition(&source_path)
        else {
            continue;
        };
        root_schema.append_conditional(
            &[],
            SchemaNode::all_of(vec![target_absent, source_absent]),
            SchemaNode::foreign(Value::Bool(false)),
        );
    }
}

fn unique_values_default_source_path(
    effective_path: &str,
    sources: &BTreeSet<helm_schema_core::ValuesDefaultSource>,
) -> Option<String> {
    let effective_segments = split_value_path(effective_path);
    let mut source_paths = sources
        .iter()
        .filter_map(|source| {
            let target_segments = split_value_path(&source.target_path);
            let suffix = effective_segments.strip_prefix(target_segments.as_slice())?;
            let mut source_segments = split_value_path(&source.source_path);
            source_segments.extend(suffix.iter().cloned());
            Some(helm_schema_core::join_value_path(&source_segments))
        })
        .collect::<BTreeSet<_>>();
    if source_paths.len() == 1 {
        source_paths.pop_first()
    } else {
        None
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
        | ConditionalGuard::ContainsTruthyMember { .. }
        | ConditionalGuard::ContainsEquals { .. }
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
                    prefix.get(..len).unwrap_or_default().to_vec()
                }
            });
        }
    }
    shared.unwrap_or_default()
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ConditionalHostPreparation {
    relaxed_host_paths: BTreeSet<Vec<String>>,
}

impl ConditionalHostPreparation {
    pub(crate) fn apply(&self, root_schema: &mut SchemaDocument) {
        for path in &self.relaxed_host_paths {
            root_schema.relax_host_object_type(path);
        }
    }
}

#[tracing::instrument(skip_all)]
pub(crate) fn prepare_conditional_hosts(
    conditionals: &[LoweredConjunct],
) -> ConditionalHostPreparation {
    let mut preparation = ConditionalHostPreparation::default();
    // Nil-safe member hosts drop the structural `type: object` their
    // descendants materialized. This is policy-free support: every
    // projection starts from the same relaxed base, whether or not it keeps
    // the presence-guarded arm carrying the exact contract.
    for conditional in conditionals {
        if conditional.carrier.relax_untyped_host
            && !crate::schema_model::is_empty_schema(&conditional.schema)
        {
            let mut segments = conditional.carrier.ancestor_segments.clone();
            segments.extend(conditional.carrier.relative_target_segments.iter().cloned());
            preparation.relaxed_host_paths.insert(segments);
        }
    }
    preparation
}

#[tracing::instrument(skip_all)]
#[expect(
    clippy::too_many_lines,
    reason = "keeping conditional grouping and emission together makes the equivalence rewrites auditable"
)]
pub(crate) fn append_selected_constraints(
    root_schema: &mut SchemaDocument,
    conditionals: Vec<LoweredConjunct>,
    values_yaml_doc: &YamlValue,
    absence: crate::condition_encoding::AbsenceDefaults<'_>,
    report: &mut EmissionReport,
) {
    let mut condition_cache = crate::condition_encoding::ConditionFragmentCache::new();
    // Conditionals sharing one guard set and scope conjoin into one if/then:
    // `allOf [{if G then A}, {if G then B}]` is `{if G then A ∧ B}`, and the
    // repeated `if` blocks dominate emitted size on charts with many guarded
    // blocks. Distinct targets merge disjointly; a leaf collision falls back
    // to its own conditional.
    let mut grouped: BTreeMap<(Vec<String>, Vec<ConditionalGuard>), Vec<LoweredConjunct>> =
        BTreeMap::new();
    for conditional in conditionals {
        // Schema-less conditionals carry base ownership established by a
        // transform or by a separate implication that already emits the
        // complete runtime domain; they have no schema arm to append.
        if crate::schema_model::is_empty_schema(&conditional.schema) {
            if matches!(conditional.class, EmissionClass::Mandatory) {
                report.mandatory_outcomes.redundant += 1;
            }
            continue;
        }
        grouped
            .entry((
                conditional.carrier.ancestor_segments.clone(),
                conditional.outer_guards().to_vec(),
            ))
            .or_default()
            .push(conditional);
    }
    struct ContentGroup {
        fragment: Value,
        guard_sets: Vec<Vec<ConditionalGuard>>,
        facts: usize,
        mandatory_facts: usize,
    }
    let mut by_content: BTreeMap<(Vec<String>, String), ContentGroup> = BTreeMap::new();
    for ((ancestor_segments, guards), group) in grouped {
        let mut merged: Option<(Value, usize, usize)> = None;
        let mut separate = Vec::new();
        for conditional in group {
            let mandatory = usize::from(matches!(conditional.class, EmissionClass::Mandatory));
            let Some(fragment) = build_scoped_target_fragment(
                &conditional,
                values_yaml_doc,
                absence,
                &mut condition_cache,
            ) else {
                report.mandatory_outcomes.fallback += mandatory;
                continue;
            };
            match &mut merged {
                None => merged = Some((fragment, 1, mandatory)),
                Some((target, facts, mandatory_facts)) => {
                    if merge_disjoint_property_fragment(target, fragment.clone()) {
                        *facts += 1;
                        *mandatory_facts += mandatory;
                    } else {
                        separate.push((fragment, 1, mandatory));
                    }
                }
            }
        }
        for (fragment, facts, mandatory_facts) in merged.into_iter().chain(separate) {
            // Conditionals with identical content under one scope disjoin
            // their guards: `if G1 then X` and `if G2 then X` is
            // `if anyOf [G1, G2] then X`, and X (often a repeated provider
            // schema) is the dominant emitted size.
            let content = by_content
                .entry((ancestor_segments.clone(), fragment.to_string()))
                .or_insert_with(|| ContentGroup {
                    fragment,
                    guard_sets: Vec::new(),
                    facts: 0,
                    mandatory_facts: 0,
                });
            content.guard_sets.push(guards.clone());
            content.facts += facts;
            content.mandatory_facts += mandatory_facts;
        }
    }
    // Arms sharing one scope and one encoded condition conjoin their
    // contents: `if C then A` beside `if C then B` is `if C then A ∧ B`,
    // and the repeated condition trees dominate emitted size on charts
    // whose lanes share a few big gates (temporal's per-service config).
    // Coalesced arms keep the FIRST occurrence's position so unaffected
    // documents keep their emission order; trivially-true fragments have
    // no if-block to save and land as their own conjuncts unchanged.
    struct PendingEmission {
        ancestor_segments: Vec<String>,
        condition: SchemaNode,
        contents: Vec<SchemaNode>,
        facts: usize,
        mandatory_facts: usize,
    }
    let mut emissions = Vec::<PendingEmission>::new();
    let mut emission_index: BTreeMap<(Vec<String>, String), usize> = BTreeMap::new();
    for ((ancestor_segments, _), group) in by_content {
        // An empty guard set is trivially true: the fragment applies
        // unconditionally (an unguarded fail implication).
        if group.guard_sets.iter().any(Vec::is_empty) {
            emissions.push(PendingEmission {
                ancestor_segments,
                condition: SchemaNode::empty(),
                contents: vec![SchemaNode::foreign(group.fragment)],
                facts: group.facts,
                mandatory_facts: group.mandatory_facts,
            });
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
                        absence,
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
                if let Some(emission) = emissions.get_mut(*entry.get()) {
                    emission.contents.push(SchemaNode::foreign(group.fragment));
                    emission.facts += group.facts;
                    emission.mandatory_facts += group.mandatory_facts;
                }
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(emissions.len());
                emissions.push(PendingEmission {
                    ancestor_segments,
                    condition,
                    contents: vec![SchemaNode::foreign(group.fragment)],
                    facts: group.facts,
                    mandatory_facts: group.mandatory_facts,
                });
            }
        }
    }
    for mut emission in emissions {
        let content = if emission.contents.len() == 1 {
            emission.contents.remove(0)
        } else {
            SchemaNode::all_of(emission.contents)
        };
        report.carriers.grouping_fan_in = report.carriers.grouping_fan_in.max(emission.facts);
        report.mandatory_outcomes.fallback += emission.mandatory_facts;
        root_schema.append_conditional(&emission.ancestor_segments, emission.condition, content);
    }
}

fn build_scoped_target_fragment(
    conditional: &LoweredConjunct,
    values_yaml_doc: &YamlValue,
    absence: crate::condition_encoding::AbsenceDefaults<'_>,
    condition_cache: &mut crate::condition_encoding::ConditionFragmentCache,
) -> Option<Value> {
    let mut target_segments = conditional.carrier.ancestor_segments.clone();
    target_segments.extend(conditional.carrier.relative_target_segments.iter().cloned());
    let mut current_anchor = target_segments;
    let mut content = SchemaNode::foreign(conditional.schema.clone());

    for scope in conditional.nested_guard_scopes().iter().rev() {
        let relative = current_anchor.strip_prefix(scope.ancestor_segments.as_slice())?;
        let then_schema = build_target_fragment(relative, content);
        if !scope.guards.iter().all(|guard| {
            guard_encodes_fully(guard, &scope.ancestor_segments, values_yaml_doc, absence)
        }) {
            return None;
        }
        let condition =
            SchemaNode::all_of(crate::condition_encoding::build_condition_clauses_cached(
                &scope.guards,
                &scope.ancestor_segments,
                values_yaml_doc,
                absence,
                condition_cache,
            ));
        let condition = condition.into_value();
        content = if crate::schema_model::is_empty_schema(&condition) {
            then_schema
        } else {
            SchemaNode::foreign(serde_json::json!({
                "if": condition,
                "then": then_schema.into_value(),
            }))
        };
        current_anchor = scope.ancestor_segments.clone();
    }

    let relative = current_anchor.strip_prefix(conditional.carrier.ancestor_segments.as_slice())?;
    Some(build_target_fragment(relative, content).into_value())
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
