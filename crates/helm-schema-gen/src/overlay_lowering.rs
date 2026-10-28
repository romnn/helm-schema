use std::collections::{BTreeMap, BTreeSet};

use helm_schema_core::{
    ConditionalGuard, ConditionalPathOverlay, ContractSchemaSignals, GuardValue,
    ProviderSchemaFragment, ValuesPath,
};
use serde_json::Value;
use serde_yaml::Value as YamlValue;

use crate::common_prefix_len;
use crate::condition_encoding::{
    build_condition_clauses, evaluate_guard_set_on_values, guard_encodes_fully,
};
use crate::emission_policy::{
    ConditionalFlavor, EmissionClass, EmissionOrigin, GuardScopes, NestedGuardScope, TerminalWhen,
};
use crate::emission_report::{EmissionReport, InsertionAbstentionCounts};
use crate::generation_decisions::{
    ContainmentCheck, ContainmentDecision, ContainmentShortCircuit, GenerationDecisions,
    ImplicationRef, OverlayResolution,
};
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_resolution::ProviderSchemaResolutions;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{
    ConditionalSchemaAcceptanceMemo, ConditionalTargetContext, conditional_target_schema,
};
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::values_yaml::yaml_value_at_values_path;

/// How a lowered conditional conjunct participates in base ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionalBaseEffect {
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
    pub(crate) target_value_path: ValuesPath,
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
    pub(crate) schema: SchemaNode,
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
        target_value_path: ValuesPath,
        ancestor_segments: Vec<String>,
        relative_target_segments: Vec<String>,
        guards: Vec<ConditionalGuard>,
        nested_guard_scopes: Vec<NestedGuardScope>,
        target_schema: SchemaNode,
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
            origin: EmissionOrigin::RequirementImplication,
            carrier: ConjunctCarrier {
                target_value_path: ValuesPath::default(),
                ancestor_segments: Vec::new(),
                relative_target_segments: Vec::new(),
                base_effect: ConditionalBaseEffect::None,
                relax_untyped_host: false,
            },
            schema: SchemaNode::foreign(Value::Bool(false)),
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
    runtime_defaults_doc: &YamlValue,
    provider_resolutions: &ProviderSchemaResolutions,
    declared_types: crate::DeclaredTypes,
    decisions: &mut GenerationDecisions,
) -> (Vec<LoweredConjunct>, InsertionAbstentionCounts) {
    let mut insertion_abstentions = InsertionAbstentionCounts::default();
    let mut synthesized_implications =
        crate::provider_requirement_synthesis::synthesized_required_source_implications(
            contract_schema_signals,
            values_yaml_doc,
            runtime_defaults_doc,
            provider_resolutions,
        );
    for (path, split_implications) in
        crate::provider_requirement_synthesis::synthesized_split_segment_implications(
            contract_schema_signals,
            provider_resolutions,
        )
        .into_iter()
        .chain(
            crate::provider_requirement_synthesis::synthesized_range_key_implications(
                contract_schema_signals,
                provider_resolutions,
            ),
        )
        .chain(
            crate::provider_requirement_synthesis::synthesized_ranged_member_required_implications(
                contract_schema_signals,
                runtime_defaults_doc,
                provider_resolutions,
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
        .map(|resolved| (&resolved.value_path, resolved))
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
    let mut acceptance_memo = ConditionalSchemaAcceptanceMemo::default();

    let root_path = ValuesPath::default();
    if let Some(root_implications) = synthesized_implications.get(&root_path) {
        for implication in root_implications {
            if !implication.outer_guards.is_empty()
                && !implication_guards_supported(
                    &implication.outer_guards,
                    &root_path,
                    &resolved_by_path,
                )
            {
                continue;
            }
            let (target_schema, abstentions) =
                crate::path_resolver::fail_requirement_schema(std::iter::once(implication));
            insertion_abstentions.requirement_target += abstentions;
            if crate::schema_model::is_empty_schema(&target_schema) {
                continue;
            }
            conditionals.push(LoweredConjunct::schema(
                EmissionOrigin::Backprojection,
                ConditionalFlavor::Ordinary,
                root_path.clone(),
                Vec::new(),
                Vec::new(),
                implication.outer_guards.clone(),
                Vec::new(),
                SchemaNode::from_value(target_schema),
                None,
                ConditionalBaseEffect::None,
                false,
            ));
        }
    }

    for (target_value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let Some(resolved_target) = resolved_by_path.get(target_value_path) else {
            continue;
        };
        let has_unconditional_self_presence_contract = evidence
            .conditional_overlays
            .iter()
            .any(|overlay| is_unconditional_self_presence_overlay(target_value_path, overlay));

        // Contract requirements hold wherever their outer guards hold. They
        // are runtime-hard, so each requirement rides an `allOf` arm —
        // property-level union lanes (declared defaults, range alternatives,
        // carrier variants) must never bypass it. An empty guard set means the
        // requirement is unconditional and the arm's condition is trivially
        // true.
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
                .requirement_implications
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
        for (implication, origin, index) in
            evidence
                .requirement_implications
                .iter()
                .enumerate()
                .map(|(index, implication)| {
                    (implication, EmissionOrigin::RequirementImplication, index)
                })
                .chain(synthesized.iter().enumerate().map(|(index, implication)| {
                    (implication, EmissionOrigin::Backprojection, index)
                }))
        {
            if is_bare_iterable_implication(implication)
                && member_implication_covers_range_domain(
                    &evidence.requirement_implications,
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
            let (mut target_schema, abstentions) =
                crate::path_resolver::fail_requirement_schema(std::iter::once(implication));
            insertion_abstentions.requirement_target += abstentions;
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
                && let Some(default) = yaml_value_at_values_path(values_yaml_doc, target_value_path)
            {
                let mut typed_schema = SchemaNode::from_value(target_schema);
                typed_schema.relax_required_members_supplied_by_default(default);
                target_schema = typed_schema.into_value();
            }
            let target_segments = target_value_path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
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
                &mut insertion_abstentions.conditional_member_projection,
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
            let short_circuit = |short_circuit_reason| ContainmentCheck::NotEvaluated {
                short_circuit_reason,
            };
            let (check, preserve_base_schema) = if member_host_only && !member_host_complete_domain
            {
                (
                    short_circuit(ContainmentShortCircuit::IncompleteMemberHost),
                    true,
                )
            } else if implication.outer_guards.is_empty() {
                (
                    short_circuit(ContainmentShortCircuit::UnguardedRequirement),
                    true,
                )
            } else if implication_has_self_truthy_guard(implication, target_value_path) {
                (
                    short_circuit(ContainmentShortCircuit::SelfTruthyGuard),
                    false,
                )
            } else if presence_scoped_type_arm {
                (
                    short_circuit(ContainmentShortCircuit::SelfPresenceTypeArm),
                    false,
                )
            } else {
                let check =
                    structural_domain_containment(&resolved_target.structural_schema, implication);
                let contains = matches!(check, ContainmentCheck::Evaluated { contains: true, .. });
                (check, contains)
            };
            let base_effect = if !preserve_base_schema {
                ConditionalBaseEffect::Own
            } else if member_host_only && !member_host_complete_domain {
                ConditionalBaseEffect::Require
            } else {
                ConditionalBaseEffect::None
            };
            decisions
                .path_mut(target_value_path)
                .containment_checks
                .push(ContainmentDecision {
                    implication: ImplicationRef { origin, index },
                    check,
                    base_effect,
                });
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
                SchemaNode::from_value(target_schema),
                None,
                base_effect,
                member_host_complete_domain && all_member_hosts_presence_scoped,
            ));
        }

        let mut type_permissions = Vec::new();
        for (overlay_index, overlay) in evidence.conditional_overlays.iter().enumerate() {
            let flavor = match overlay.flavor {
                helm_schema_core::ConditionalOverlayFlavor::Ordinary => ConditionalFlavor::Ordinary,
                helm_schema_core::ConditionalOverlayFlavor::KindBranch => {
                    ConditionalFlavor::KindPartition
                }
            };
            if is_unconditional_self_presence_overlay(target_value_path, overlay) {
                continue;
            }
            if !guards_supported_for_conditional_lowering(
                &overlay.guards,
                &resolved_by_path,
                values_yaml_doc,
            ) {
                continue;
            }

            let target_segments = target_value_path
                .segments()
                .map(helm_schema_core::Segment::encode_component)
                .collect::<Vec<_>>();
            let Some((outer_guards, nested_guard_scopes)) =
                partition_guard_scopes(&target_segments, &overlay.guards)
            else {
                continue;
            };
            if is_read_type_permission(target_value_path, overlay) {
                type_permissions.push(overlay.guards.clone());
                continue;
            }
            let ancestor_segments = nested_guard_scopes
                .first()
                .filter(|_| outer_guards.is_empty())
                .map(|scope| {
                    let mut parent = scope.ancestor_segments.clone();
                    parent.pop();
                    parent
                })
                .unwrap_or_else(|| conditional_ancestor_segments(&target_segments, &outer_guards));
            let active_by_defaults = evaluate_guard_set_on_values(&overlay.guards, values_yaml_doc);
            let (resolved_overlay, overlay_resolution) =
                resolve_overlay_target_schema(target_value_path, overlay, provider_resolutions);
            // The range header supplies the branch's complete runtime
            // domain. Its declared sample shape cannot remain as an
            // unconditional base without deleting valid map or integer
            // lanes while the range is active.
            let preserve_overlay_base = !overlay.evidence.facts.is_ranged_source
                && (overlay.preserve_base_schema || has_unconditional_self_presence_contract);
            let overlay_base_effect = if preserve_overlay_base {
                ConditionalBaseEffect::Preserve
            } else {
                ConditionalBaseEffect::Own
            };
            let mut overlay_decision = OverlayResolution {
                overlay: overlay_index,
                evaluation: overlay_resolution.effective,
                base_effect: Some(overlay_base_effect),
            };
            // A ranged branch's runtime domain is structural evidence, not
            // a declared-default placeholder. Add it before conditional
            // policy so a fixed map default cannot reintroduce literal
            // member typing that the loop body erased (for example through
            // `quote`).
            let member_implication_owns_range_domain = overlay.evidence.facts.is_ranged_source
                && crate::schema_model::is_empty_schema(&resolved_overlay.schema)
                && member_implication_covers_range_domain(
                    &evidence.requirement_implications,
                    &overlay.guards,
                );
            if member_implication_owns_range_domain {
                // The requirement implication already carries the branch's
                // complete runtime domain. Keep only this empty ownership
                // marker; passing an evidence-free overlay through
                // conditional policy would substitute its values.yaml
                // sample shape and re-type members the range accepts.
                conditionals.push(LoweredConjunct::schema(
                    EmissionOrigin::Overlay,
                    flavor,
                    target_value_path.clone(),
                    ancestor_segments.clone(),
                    target_segments
                        .get(ancestor_segments.len()..)
                        .unwrap_or_default()
                        .to_vec(),
                    outer_guards.clone(),
                    nested_guard_scopes.clone(),
                    SchemaNode::empty(),
                    None,
                    overlay_base_effect,
                    false,
                ));
                decisions
                    .path_mut(target_value_path)
                    .overlays
                    .push(overlay_decision);
                continue;
            }
            let range_allows_integer = overlay
                .evidence
                .range_domain
                .is_some_and(helm_schema_core::RangeDomain::allows_integer);
            let mut range_domain = crate::runtime_iterable_schema(range_allows_integer);
            let mut member_schemas = Vec::new();
            if let Some(member_schema) = member_descendant_projection(
                target_segments.as_slice(),
                member_descendants
                    .get(target_segments.as_slice())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &mut insertion_abstentions.conditional_member_projection,
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
            let mut context = ConditionalTargetContext {
                values_yaml_doc,
                acceptance_memo: &mut acceptance_memo,
                declared_types,
                provider_resolutions,
            };
            let target_schema = conditional_target_schema(
                target_value_path,
                overlay,
                &mut context,
                branch_schema,
                &resolved_target.values_yaml_schema,
                resolved_target.schema.clone(),
                active_by_defaults,
            );
            let implied_by_guard = overlay
                .guards
                .iter()
                .any(|guard| type_guard_implies_schema(guard, target_value_path, &target_schema));
            let target_schema = if implied_by_guard {
                crate::schema_model::empty_schema()
            } else {
                target_schema
            };
            if crate::schema_model::is_empty_schema(&target_schema) {
                // A vacuous assertion still owns its branch's base policy. Serialized arms
                // retain the same empty marker; independently resolved siblings keep their
                // stricter contracts.
                if implied_by_guard
                    || overlay.evidence.facts.used_as_serialized
                    || overlay.evidence.facts.used_as_yaml_serialized
                {
                    conditionals.push(LoweredConjunct::schema(
                        EmissionOrigin::Overlay,
                        flavor,
                        target_value_path.clone(),
                        ancestor_segments.clone(),
                        target_segments
                            .get(ancestor_segments.len()..)
                            .unwrap_or_default()
                            .to_vec(),
                        outer_guards.clone(),
                        nested_guard_scopes.clone(),
                        SchemaNode::from_value(target_schema),
                        None,
                        overlay_base_effect,
                        false,
                    ));
                } else {
                    overlay_decision.base_effect = None;
                }
                decisions
                    .path_mut(target_value_path)
                    .overlays
                    .push(overlay_decision);
                continue;
            }
            let provider_schema_candidate = resolved_overlay
                .provider_schema_candidate
                .filter(|candidate| candidate.survives_as(&target_schema));

            conditionals.push(LoweredConjunct::schema(
                EmissionOrigin::Overlay,
                flavor,
                target_value_path.clone(),
                ancestor_segments.clone(),
                target_segments
                    .get(ancestor_segments.len()..)
                    .unwrap_or_default()
                    .to_vec(),
                outer_guards,
                nested_guard_scopes,
                SchemaNode::from_value(target_schema),
                provider_schema_candidate,
                overlay_base_effect,
                false,
            ));
            decisions
                .path_mut(target_value_path)
                .overlays
                .push(overlay_decision);
        }
        append_type_permission_fallback(
            &mut conditionals,
            target_value_path,
            resolved_target,
            type_permissions,
            declared_types,
        );
    }

    append_merge_shadow_arms(
        &mut conditionals,
        contract_schema_signals,
        provider_resolutions,
    );
    append_omitted_member_arms(
        &mut conditionals,
        contract_schema_signals,
        provider_resolutions,
    );
    (conditionals, insertion_abstentions)
}

fn is_read_type_permission(target: &ValuesPath, overlay: &ConditionalPathOverlay) -> bool {
    let evidence = &overlay.evidence;
    !evidence.facts.has_render_use
        && !evidence.facts.has_string_contract
        && !evidence.facts.is_ranged_source
        && !evidence.facts.used_as_serialized
        && !evidence.facts.used_as_yaml_serialized
        && evidence.range_domain.is_none()
        && evidence.provider_schema_uses.is_empty()
        && evidence.type_hints.is_empty()
        && evidence.metadata_field_kinds.is_empty()
        && overlay
            .guards
            .iter()
            .any(|guard| positive_self_type(guard, target))
}

fn positive_self_type(guard: &ConditionalGuard, target: &ValuesPath) -> bool {
    match guard {
        ConditionalGuard::TypeIs {
            path,
            schema_type: _,
        } => path == target,
        ConditionalGuard::AllOf(guards) => {
            guards.iter().any(|guard| positive_self_type(guard, target))
        }
        ConditionalGuard::Truthy { path: _ }
        | ConditionalGuard::With { path: _ }
        | ConditionalGuard::Eq { path: _, value: _ }
        | ConditionalGuard::NotEq { path: _, value: _ }
        | ConditionalGuard::Absent { path: _ }
        | ConditionalGuard::MatchesPattern {
            path: _,
            pattern: _,
        }
        | ConditionalGuard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | ConditionalGuard::MatchesValue {
            path: _,
            pattern: _,
        }
        | ConditionalGuard::IntGt { path: _, bound: _ }
        | ConditionalGuard::IntLt { path: _, bound: _ }
        | ConditionalGuard::HasKey { path: _, key: _ }
        | ConditionalGuard::ContainsMemberEquals {
            path: _,
            member: _,
            value: _,
        }
        | ConditionalGuard::ContainsTruthyMember { path: _, member: _ }
        | ConditionalGuard::ContainsEquals { path: _, value: _ }
        | ConditionalGuard::AtMostOneMember { path: _ }
        | ConditionalGuard::MinMembers { path: _, bound: _ }
        | ConditionalGuard::Not(_)
        | ConditionalGuard::AnyOf(_) => false,
    }
}

fn type_guard_implies_schema(
    guard: &ConditionalGuard,
    target: &ValuesPath,
    schema: &Value,
) -> bool {
    match guard {
        ConditionalGuard::TypeIs { path, schema_type } => {
            path == target && schema_contains_type(schema, schema_type)
        }
        ConditionalGuard::AllOf(guards) => guards
            .iter()
            .any(|guard| type_guard_implies_schema(guard, target, schema)),
        ConditionalGuard::AnyOf(guards) => guards
            .iter()
            .all(|guard| type_guard_implies_schema(guard, target, schema)),
        ConditionalGuard::Truthy { path: _ }
        | ConditionalGuard::With { path: _ }
        | ConditionalGuard::Eq { path: _, value: _ }
        | ConditionalGuard::NotEq { path: _, value: _ }
        | ConditionalGuard::Absent { path: _ }
        | ConditionalGuard::MatchesPattern {
            path: _,
            pattern: _,
        }
        | ConditionalGuard::CollectionPattern {
            path: _,
            domain: _,
            quantifier: _,
            pattern: _,
        }
        | ConditionalGuard::MatchesValue {
            path: _,
            pattern: _,
        }
        | ConditionalGuard::IntGt { path: _, bound: _ }
        | ConditionalGuard::IntLt { path: _, bound: _ }
        | ConditionalGuard::HasKey { path: _, key: _ }
        | ConditionalGuard::ContainsMemberEquals {
            path: _,
            member: _,
            value: _,
        }
        | ConditionalGuard::ContainsTruthyMember { path: _, member: _ }
        | ConditionalGuard::ContainsEquals { path: _, value: _ }
        | ConditionalGuard::AtMostOneMember { path: _ }
        | ConditionalGuard::MinMembers { path: _, bound: _ }
        | ConditionalGuard::Not(_) => false,
    }
}

fn schema_contains_type(schema: &Value, schema_type: &str) -> bool {
    if schema == &Value::Bool(true) || crate::schema_model::is_empty_schema(schema) {
        return true;
    }
    let Some(object) = schema.as_object().filter(|object| object.len() == 1) else {
        return false;
    };
    // Only complete type arms discharge an assertion; constrained or exclusive arms cannot.
    if let Some(value) = object.get("type") {
        let covers = |value: &Value| {
            value.as_str().is_some_and(|name| {
                name == schema_type || name == "number" && schema_type == "integer"
            })
        };
        return value
            .as_array()
            .map_or_else(|| covers(value), |types| types.iter().any(covers));
    }
    if let Some(arms) = object.get("anyOf").and_then(Value::as_array) {
        return arms
            .iter()
            .any(|arm| schema_contains_type(arm, schema_type));
    }
    object
        .get("allOf")
        .and_then(Value::as_array)
        .is_some_and(|arms| {
            arms.iter()
                .all(|arm| schema_contains_type(arm, schema_type))
        })
}

fn append_type_permission_fallback(
    conditionals: &mut Vec<LoweredConjunct>,
    target: &ValuesPath,
    resolved: &ResolvedPathSchema,
    permissions: Vec<Vec<ConditionalGuard>>,
    declared_types: crate::DeclaredTypes,
) {
    if permissions.is_empty()
        || (crate::schema_model::is_empty_schema(&resolved.schema)
            && resolved.independent_base_contract.is_none())
        || conditionals.iter().any(|conditional| {
            conditional.carrier.target_value_path == *target
                && matches!(
                    conditional.carrier.base_effect,
                    ConditionalBaseEffect::Own | ConditionalBaseEffect::Preserve
                )
        })
    {
        return;
    }
    // Only declaration supplies the fallback; guarded runtime hints are not unconditional contracts.
    // An empty fallback still owns the base, while independent consumers remain below.
    // Serialization already discarded the declaration's type; the fallback cannot restore it.
    let declaration = if resolved.used_as_serialized {
        crate::schema_model::empty_schema()
    } else {
        crate::values_yaml::declared_shape_alone(
            declared_types,
            resolved.values_yaml_schema.clone(),
        )
    };
    let guards = vec![ConditionalGuard::Not(Box::new(ConditionalGuard::AnyOf(
        permissions
            .into_iter()
            .map(ConditionalGuard::AllOf)
            .collect(),
    )))];
    let target_segments = &resolved.path_segments;
    let Some((outer_guards, nested_guard_scopes)) =
        partition_guard_scopes(target_segments, &guards)
    else {
        return;
    };
    let ancestor_segments = conditional_ancestor_segments(target_segments, &outer_guards);
    conditionals.push(LoweredConjunct::schema(
        EmissionOrigin::Overlay,
        ConditionalFlavor::Ordinary,
        target.clone(),
        ancestor_segments.clone(),
        target_segments
            .get(ancestor_segments.len()..)
            .unwrap_or_default()
            .to_vec(),
        outer_guards,
        nested_guard_scopes,
        SchemaNode::from_value(declaration),
        None,
        ConditionalBaseEffect::Own,
        false,
    ));
    if let Some(independent) = &resolved.independent_base_contract {
        conditionals.push(LoweredConjunct::schema(
            EmissionOrigin::Overlay,
            ConditionalFlavor::Ordinary,
            target.clone(),
            Vec::new(),
            target_segments.clone(),
            Vec::new(),
            Vec::new(),
            independent.schema(),
            None,
            ConditionalBaseEffect::None,
            false,
        ));
    }
}

mod conditional_constraints;
mod member_projection;

#[cfg(test)]
#[path = "tests/overlay_type_implication.rs"]
mod tests;

pub(crate) use conditional_constraints::{
    ConditionalHostPreparation, append_selected_constraints, append_terminal_clauses,
    prepare_conditional_hosts,
};
use conditional_constraints::{
    conditional_ancestor_segments, guards_supported_for_conditional_lowering,
    implication_guards_supported, partition_guard_scopes, resolve_overlay_target_schema,
};
pub(crate) use member_projection::member_descendant_projection;
use member_projection::{
    append_merge_shadow_arms, append_omitted_member_arms, implication_has_self_presence_guard,
    implication_has_self_truthy_guard, is_bare_iterable_implication,
    is_unconditional_self_presence_overlay, member_implication_covers_range_domain,
    structural_collection_member_projection, structural_domain_containment,
};
