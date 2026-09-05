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
use crate::path_resolver::{PathSchemaResolver, ResolvedPathSchema};
use crate::provider_resolution::ProviderSchemaResolutions;
use crate::provider_schema::ProviderSchemaCandidate;
use crate::resolve_policy::{
    ConditionalSchemaAcceptanceMemo, ConditionalTargetContext, conditional_target_schema,
};
use crate::schema_node::SchemaNode;
use crate::schema_tree::SchemaDocument;
use crate::values_yaml::yaml_value_at_values_path;

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
    subchart_defaults_doc: &YamlValue,
    provider_resolutions: &ProviderSchemaResolutions,
) -> (Vec<LoweredConjunct>, InsertionAbstentionCounts) {
    let mut insertion_abstentions = InsertionAbstentionCounts::default();
    let mut synthesized_implications =
        crate::provider_requirement_synthesis::synthesized_required_source_implications(
            contract_schema_signals,
            values_yaml_doc,
            subchart_defaults_doc,
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
                subchart_defaults_doc,
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
        for (implication, origin) in evidence
            .requirement_implications
            .iter()
            .map(|implication| (implication, EmissionOrigin::RequirementImplication))
            .chain(
                synthesized
                    .iter()
                    .map(|implication| (implication, EmissionOrigin::Backprojection)),
            )
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
                SchemaNode::from_value(target_schema),
                None,
                base_effect,
                member_host_complete_domain && all_member_hosts_presence_scoped,
            ));
        }

        for source_overlay in &evidence.conditional_overlays {
            let overlay = source_overlay;
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
            let resolved_overlay =
                resolve_overlay_target_schema(target_value_path, overlay, provider_resolutions);
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
                    if preserve_overlay_base {
                        ConditionalBaseEffect::Preserve
                    } else {
                        ConditionalBaseEffect::Own
                    },
                    false,
                ));
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
                if preserve_overlay_base {
                    ConditionalBaseEffect::Preserve
                } else {
                    ConditionalBaseEffect::Own
                },
                false,
            ));
        }
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

mod conditional_constraints;
mod member_projection;

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
    resolved_schema_admits_fail_requirement_domain, structural_collection_member_projection,
};
