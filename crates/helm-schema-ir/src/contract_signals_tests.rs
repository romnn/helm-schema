use std::collections::BTreeSet;

use crate::contract::{ContractIr, ContractUse};
use crate::contract_signals::{ContractSchemaSignals, GuardConstraint, MetadataFieldKind};
use crate::{Guard, ResourceRef, ValueKind, YamlPath};

fn signals_for(uses: Vec<ContractUse>) -> ContractSchemaSignals {
    ContractIr::from_contract_uses(uses).into_schema_signals()
}

#[test]
fn contract_ir_nullable_paths_include_range_only_collection() {
    let nullable_paths = signals_for(vec![ContractUse::new(
        "snapshot".to_string(),
        YamlPath(vec!["data".to_string(), "command".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Range {
            path: "snapshots".to_string(),
        }],
        None,
    )])
    .nullable_value_paths;

    assert!(
        nullable_paths.contains("snapshots"),
        "range sources are null-tolerant because Helm treats nil range inputs as empty: {nullable_paths:?}",
    );
    assert!(
        !nullable_paths.contains("snapshot"),
        "range item values should not inherit collection nullability: {nullable_paths:?}",
    );
}

#[test]
fn contract_ir_nullable_paths_require_every_render_use_to_be_tolerant() {
    let nullable_paths = signals_for(vec![
        ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "namespace".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
    ])
    .nullable_value_paths;

    assert!(
        !nullable_paths.contains("serviceAccount.name"),
        "one guarded render use must not make a bare render site nullable: {nullable_paths:?}",
    );
}

#[test]
fn contract_ir_path_signals_collect_references_and_typed_guard_constraints() {
    let signals = signals_for(vec![
        ContractUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            ValueKind::Fragment,
            vec![
                Guard::Eq {
                    path: "mode".to_string(),
                    value: "prod".to_string(),
                },
                Guard::TypeIs {
                    path: "extraConfig".to_string(),
                    schema_type: "string".to_string(),
                },
                Guard::Range {
                    path: "extraEnv".to_string(),
                },
            ],
            None,
        ),
        ContractUse::new(
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "image".to_string()]),
            ValueKind::PartialScalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            "podName".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            "podNamespace".to_string(),
            YamlPath(vec!["metadata".to_string(), "namespace".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            String::new(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Eq {
                path: "ignored.guard".to_string(),
                value: "ignored".to_string(),
            }],
            None,
        ),
    ])
    .path_signals;

    assert_eq!(
        signals.referenced_value_paths,
        BTreeSet::from([
            "extraConfig".to_string(),
            "extraEnv".to_string(),
            "image.tag".to_string(),
            "mode".to_string(),
            "podLabels".to_string(),
            "podName".to_string(),
            "podNamespace".to_string(),
        ]),
    );
    assert_eq!(
        signals.ranged_value_paths,
        BTreeSet::from(["extraEnv".to_string()]),
    );
    assert_eq!(
        signals.value_paths_used_as_fragment,
        BTreeSet::from(["podLabels".to_string()]),
    );
    assert_eq!(
        signals.partial_scalar_value_paths,
        BTreeSet::from(["image.tag".to_string()]),
    );
    assert_eq!(
        signals.metadata_fields_by_value_path.get("podLabels"),
        Some(&BTreeSet::from([MetadataFieldKind::StringMap])),
    );
    assert_eq!(
        signals.metadata_fields_by_value_path.get("podName"),
        Some(&BTreeSet::from([MetadataFieldKind::Name])),
    );
    assert_eq!(
        signals.metadata_fields_by_value_path.get("podNamespace"),
        Some(&BTreeSet::from([MetadataFieldKind::Namespace])),
    );
    assert_eq!(
        signals.guard_constraints_by_value_path.get("mode"),
        Some(&vec![GuardConstraint::Eq {
            value: "prod".to_string(),
        }]),
    );
    assert_eq!(
        signals.guard_constraints_by_value_path.get("extraConfig"),
        Some(&vec![GuardConstraint::TypeIs {
            schema_type: "string".to_string(),
        }]),
    );
    assert!(
        !signals.referenced_value_paths.contains("ignored.guard"),
        "empty-source compatibility rows should not seed schema paths",
    );
    assert!(
        !signals.metadata_fields_by_value_path.contains_key(""),
        "empty-source compatibility rows should not seed metadata facts",
    );
}

#[test]
fn contract_ir_provider_schema_uses_are_rendered_resource_claims_only() {
    let resource = ResourceRef {
        api_version: "apps/v1".to_string(),
        kind: "Deployment".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let requests = signals_for(vec![
        ContractUse::new(
            "containers".to_string(),
            YamlPath(vec![
                "spec".to_string(),
                "template".to_string(),
                "spec".to_string(),
                "containers".to_string(),
            ]),
            ValueKind::Fragment,
            Vec::new(),
            Some(resource.clone()),
        ),
        ContractUse::new(
            "ports".to_string(),
            YamlPath(vec!["spec".to_string(), "ports".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Range {
                path: "ports".to_string(),
            }],
            Some(resource.clone()),
        ),
        ContractUse::new(
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "image".to_string()]),
            ValueKind::PartialScalar,
            Vec::new(),
            Some(resource.clone()),
        ),
        ContractUse::new(
            "pathless".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            Some(resource.clone()),
        ),
        ContractUse::new(
            "noResource".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            String::new(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            Some(resource),
        ),
    ])
    .provider_schema_uses;

    assert_eq!(requests.len(), 2, "{requests:#?}");
    assert_eq!(requests[0].value_path, "containers");
    assert_eq!(requests[0].kind, ValueKind::Fragment);
    assert!(!requests[0].is_self_range_collection);
    assert_eq!(requests[1].value_path, "ports");
    assert_eq!(requests[1].kind, ValueKind::Scalar);
    assert!(requests[1].is_self_range_collection);
}

#[test]
fn contract_ir_schema_signals_bundle_core_generation_facts() {
    let resource = ResourceRef {
        api_version: "apps/v1".to_string(),
        kind: "Deployment".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let signals = signals_for(vec![
        ContractUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            ValueKind::Fragment,
            Vec::new(),
            Some(resource.clone()),
        ),
        ContractUse::new(
            "serviceAccount.name".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Default {
                path: "serviceAccount.name".to_string(),
            }],
            Some(resource),
        ),
    ]);

    assert_eq!(
        signals
            .path_signals
            .metadata_fields_by_value_path
            .get("podLabels"),
        Some(&BTreeSet::from([MetadataFieldKind::StringMap])),
    );
    assert!(
        signals.nullable_value_paths.contains("serviceAccount.name"),
        "default-guarded render use should surface as nullable contract evidence",
    );
    assert!(
        signals
            .paths_with_referenced_descendants
            .contains("serviceAccount"),
        "contract schema signals should own descendant path topology",
    );
    assert!(
        signals
            .value_path_facts
            .get("serviceAccount.name")
            .is_some_and(|fact| fact.has_render_use && fact.all_render_uses_self_guarded),
        "contract value-path facts should own render-use evidence",
    );
    assert!(
        signals
            .value_path_facts
            .get("serviceAccount")
            .is_some_and(|fact| fact.has_referenced_descendants),
        "contract value-path facts should own descendant path topology",
    );
    assert!(
        signals
            .value_path_facts
            .get("serviceAccount.name")
            .is_some_and(|fact| fact.has_render_use
                && fact.all_render_uses_self_guarded
                && fact.is_nullable),
        "contract value-path facts should bundle nullable render-use evidence",
    );
    assert_eq!(signals.provider_schema_uses.len(), 2);
}

#[test]
fn contract_ir_derives_schema_signals_without_projection_detour() {
    let resource = ResourceRef {
        api_version: "v1".to_string(),
        kind: "ServiceAccount".to_string(),
        api_version_candidates: Vec::new(),
        api_version_branches: Vec::new(),
    };
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        Some(resource.clone()),
    ));
    contract.push(ContractUse::new(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Default {
            path: "serviceAccount.name".to_string(),
        }],
        Some(resource),
    ));
    contract.push(ContractUse::new(
        "podLabels".to_string(),
        YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
        ValueKind::Fragment,
        Vec::new(),
        None,
    ));

    let direct_signals = contract.into_schema_signals();

    assert!(
        direct_signals
            .nullable_value_paths
            .contains("serviceAccount.name"),
        "semantic finalization should keep the default-guarded render claim",
    );
    assert_eq!(direct_signals.provider_schema_uses.len(), 1);
    assert!(
        direct_signals
            .path_signals
            .metadata_fields_by_value_path
            .contains_key("podLabels"),
    );
}

#[test]
fn contract_ir_required_inference_signals_are_typed_header_facts() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.enabled".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            "mode".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            vec![Guard::Eq {
                path: "mode".to_string(),
                value: "strict".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "optional".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: "optional".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "either.primary".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Or {
                paths: vec!["either.primary".to_string(), "either.fallback".to_string()],
            }],
            None,
        ),
        ContractUse::new(
            "ranged".to_string(),
            YamlPath(vec!["spec".to_string(), "ports".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Range {
                path: "ranged".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "defaulted".to_string(),
            YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Default {
                path: "defaulted".to_string(),
            }],
            None,
        ),
    ])
    .required_inference_signals;

    assert_eq!(
        signals.positive_header_paths,
        BTreeSet::from(["feature.enabled".to_string(), "mode".to_string()])
    );
    assert_eq!(
        signals.conditionally_optional_paths,
        BTreeSet::from([
            "optional".to_string(),
            "either.primary".to_string(),
            "either.fallback".to_string(),
        ])
    );
    assert_eq!(
        signals.default_fallback_paths,
        BTreeSet::from(["defaulted".to_string()])
    );
}
