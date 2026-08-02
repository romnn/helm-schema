//! Provider-presence projections over structurally transformed sources.

use std::collections::{BTreeMap, BTreeSet};

use color_eyre::eyre;
use helm_schema_core::{
    ConditionalGuard, ContractFailImplication, ContractPathSchemaEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MergeLayerTransform, MergeLayersUse, ProviderSchemaUse, ResourceRef,
    ValueKind, YamlPath,
};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

use super::provider;

fn hpa_merge_layer_use(
    value_path: &str,
    layers: Vec<String>,
    outer_guards: Vec<ConditionalGuard>,
) -> ProviderSchemaUse {
    ProviderSchemaUse {
        value_path: value_path.to_string(),
        path: YamlPath(vec!["spec".to_string(), "maxReplicas".to_string()]),
        kind: ValueKind::Scalar,
        stringified: false,
        resource: ResourceRef::concrete(
            "autoscaling/v2".to_string(),
            "HorizontalPodAutoscaler".to_string(),
        ),
        is_self_range_collection: false,
        source_null_tolerant: false,
        template_supplied_member_keys: BTreeSet::new(),
        split_segment: None,
        merge_layers: Some(MergeLayersUse {
            transforms: vec![MergeLayerTransform::Identity; layers.len()],
            layers,
            position: 0,
            via_binding: false,
        }),
        range_key: false,
        nil_omitting: false,
        omitted_members: BTreeMap::new(),
        outer_guards,
    }
}

/// A provider reads the merged value, so an absent preferred leaf selects
/// the fallback instead of rendering null. Neither the direct nor ranged
/// source-presence lane may require that preferred leaf independently.
#[test]
fn merge_layer_presence_belongs_to_the_combined_result() -> eyre::Result<()> {
    let direct_path = "preferred.maxReplicaCount";
    let member_path = "sets.*.maxReplicaCount";
    let evidence = BTreeMap::from([
        (
            direct_path.to_string(),
            ContractPathSchemaEvidence {
                value_path: direct_path.to_string(),
                is_referenced_value_path: true,
                facts: ContractValuePathFacts {
                    has_unconditional_render_use: true,
                    ..ContractValuePathFacts::default()
                },
                provider_schema_uses: vec![hpa_merge_layer_use(
                    direct_path,
                    vec![
                        direct_path.to_string(),
                        "fallback.maxReplicaCount".to_string(),
                    ],
                    Vec::new(),
                )],
                ..ContractPathSchemaEvidence::default()
            },
        ),
        (
            member_path.to_string(),
            ContractPathSchemaEvidence {
                value_path: member_path.to_string(),
                is_referenced_value_path: true,
                provider_schema_uses: vec![hpa_merge_layer_use(
                    member_path,
                    vec![
                        member_path.to_string(),
                        "fallback.maxReplicaCount".to_string(),
                    ],
                    vec![ConditionalGuard::Truthy {
                        path: "sets.*.enabled".to_string(),
                    }],
                )],
                ..ContractPathSchemaEvidence::default()
            },
        ),
    ]);
    let signals = ContractSchemaSignals::new(evidence, Vec::new());
    let values = serde_yaml::from_str(indoc! {"
        preferred:
          maxReplicaCount: 5
    "})?;
    let no_dependency_defaults = serde_yaml::from_str("{}")?;

    let direct = crate::required_source_backprojection::synthesized_required_source_implications(
        &signals,
        &values,
        &no_dependency_defaults,
        &provider(),
    );
    let ranged =
        crate::required_source_backprojection::synthesized_ranged_member_required_implications(
            &signals,
            &no_dependency_defaults,
            &provider(),
        );

    sim_assert_eq!(
        have: direct,
        want: BTreeMap::<String, Vec<ContractFailImplication>>::new()
    );
    sim_assert_eq!(
        have: ranged,
        want: BTreeMap::<String, Vec<ContractFailImplication>>::new()
    );

    Ok(())
}

/// Null tolerance belongs to the provider use, not the whole values path.
/// Another partial-scalar use may make the path aggregate non-nullable while
/// this self-guarded provider slot still cannot require the source.
#[test]
fn null_tolerant_provider_use_does_not_require_source() -> eyre::Result<()> {
    let value_path = "webhook.securePort";
    let mut use_ = hpa_merge_layer_use(value_path, vec![value_path.to_string()], Vec::new());
    use_.merge_layers = None;
    use_.source_null_tolerant = true;
    let evidence = BTreeMap::from([(
        value_path.to_string(),
        ContractPathSchemaEvidence {
            value_path: value_path.to_string(),
            is_referenced_value_path: true,
            facts: ContractValuePathFacts {
                has_unconditional_render_use: true,
                is_nullable: false,
                ..ContractValuePathFacts::default()
            },
            provider_schema_uses: vec![use_],
            ..ContractPathSchemaEvidence::default()
        },
    )]);
    let signals = ContractSchemaSignals::new(evidence, Vec::new());
    let values = serde_yaml::from_str(indoc! {"
        webhook:
          securePort: 10250
    "})?;
    let no_dependency_defaults = serde_yaml::from_str("{}")?;

    let implications =
        crate::required_source_backprojection::synthesized_required_source_implications(
            &signals,
            &values,
            &no_dependency_defaults,
            &provider(),
        );

    sim_assert_eq!(
        have: implications,
        want: BTreeMap::<String, Vec<ContractFailImplication>>::new()
    );

    Ok(())
}

/// A range-key sink runs only once the collection has a member, so the
/// provider slot cannot require the collection itself.
#[test]
fn range_key_provider_presence_does_not_require_collection() -> eyre::Result<()> {
    let value_path = "extraContainers";
    let mut use_ = hpa_merge_layer_use(value_path, vec![value_path.to_string()], Vec::new());
    use_.merge_layers = None;
    use_.range_key = true;
    let evidence = BTreeMap::from([(
        value_path.to_string(),
        ContractPathSchemaEvidence {
            value_path: value_path.to_string(),
            is_referenced_value_path: true,
            facts: ContractValuePathFacts {
                has_unconditional_render_use: true,
                ..ContractValuePathFacts::default()
            },
            provider_schema_uses: vec![use_],
            ..ContractPathSchemaEvidence::default()
        },
    )]);
    let signals = ContractSchemaSignals::new(evidence, Vec::new());
    let values = serde_yaml::from_str("extraContainers: []\n")?;
    let no_dependency_defaults = serde_yaml::from_str("{}")?;

    let implications =
        crate::required_source_backprojection::synthesized_required_source_implications(
            &signals,
            &values,
            &no_dependency_defaults,
            &provider(),
        );

    sim_assert_eq!(
        have: implications,
        want: BTreeMap::<String, Vec<ContractFailImplication>>::new()
    );

    Ok(())
}
