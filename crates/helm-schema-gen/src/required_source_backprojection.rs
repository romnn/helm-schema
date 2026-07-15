//! Provider-required output fields require their source leaves.
//!
//! When a conditional branch renders a direct scalar `.Values` hole into a
//! resource field the provider marks `required`, Helm still renders on a
//! missing or null source — emitting an explicit null the provider then
//! rejects (a Service `port`, for example). Wherever the branch's guards
//! hold, the source leaf must therefore be present and non-null.
//!
//! The projection is deliberately bounded to branch overlays with exact
//! decoded guards and to direct scalar holes: serialized, fragment,
//! partial-scalar, defaulted, ranged, and self-guarded uses all render
//! something else (or nothing) on a missing source, so they abstain. The
//! requirements ride the same root-anchored arm machinery as `fail`
//! implications, which also relaxes the presence half for leaves the
//! chart's own defaults supply.

use std::collections::{BTreeMap, HashMap};

use helm_schema_core::{
    ContractFailImplication, ContractRequirementTarget, ContractSchemaSignals,
    FailValueRequirement, ProviderSchemaUse, ResourceSchemaOracle, ValueKind,
};

pub(crate) fn synthesized_required_source_implications(
    contract_schema_signals: &ContractSchemaSignals,
    provider: &dyn ResourceSchemaOracle,
) -> BTreeMap<String, Vec<ContractFailImplication>> {
    let mut implications: BTreeMap<String, Vec<ContractFailImplication>> = BTreeMap::new();
    let mut required_by_use: HashMap<ProviderSchemaUse, bool> = HashMap::new();

    for (value_path, evidence) in contract_schema_signals.schema_evidence_by_value_path() {
        let segments = crate::split_value_path(value_path);
        let Some((leaf_segment, parent_segments)) = segments.split_last() else {
            continue;
        };
        if parent_segments.is_empty() || segments.iter().any(|segment| segment == "*") {
            continue;
        }
        for overlay in &evidence.conditional_overlays {
            if overlay.guards.is_empty() {
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
            if facts.used_as_serialized
                || facts.used_as_yaml_serialized
                || facts.used_as_fragment
                || facts.used_as_pathless_fragment
                || facts.is_partial_scalar_value_path
                || facts.is_nullable
                || facts.has_self_guarded_render_use
                || facts.is_ranged_source
            {
                continue;
            }
            let renders_into_required_field =
                overlay.evidence.provider_schema_uses.iter().any(|use_| {
                    use_.kind == ValueKind::Scalar
                        && !use_.is_self_range_collection
                        && *required_by_use.entry(use_.clone()).or_insert_with(|| {
                            provider
                                .schema_fragment_for_use(use_)
                                .is_some_and(|fragment| fragment.required_in_parent())
                        })
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
                    requirements: vec![FailValueRequirement::HasMember(leaf_segment.clone())],
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
