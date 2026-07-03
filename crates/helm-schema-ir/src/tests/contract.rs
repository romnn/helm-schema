use crate::{
    ContractIr, ContractProvenance, ContractUse, Guard, GuardValue, ResourceRef, SourceSpan,
    ValueKind, YamlPath,
};
use test_util::prelude::sim_assert_eq;

#[test]
fn contract_ir_finalization_keeps_default_guarded_render_site_over_bare_duplicate() {
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    ));
    contract.push(ContractUse::new(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Default {
            path: "serviceAccount.name".to_string(),
        }],
        None,
    ));

    let value_uses = contract.finalize();
    let value_uses = value_uses.uses();

    sim_assert_eq!(have: value_uses.len(), want: 1);
    sim_assert_eq!(
        have: value_uses.first().map(|value_use| &value_use.guards),
        want: Some(&vec![Guard::Default {
            path: "serviceAccount.name".to_string(),
        }])
    );
}

#[test]
fn contract_ir_finalization_prefers_resource_claim_for_pathless_duplicate() {
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        "nameOverride".to_string(),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        Vec::new(),
        None,
    ));
    contract.push(ContractUse::new(
        "nameOverride".to_string(),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        Vec::new(),
        Some(ResourceRef::concrete(
            "v1".to_string(),
            "Service".to_string(),
        )),
    ));

    let value_uses = contract.finalize();
    let value_uses = value_uses.uses();

    sim_assert_eq!(have: value_uses.len(), want: 1);
    sim_assert_eq!(
        have: value_uses
            .first()
            .and_then(|value_use| value_use.resource.as_ref())
            .map(|resource| (resource.api_version.as_str(), resource.kind.as_str())),
        want: Some(("v1", "Service"))
    );
}

#[test]
fn contract_ir_keeps_dependency_use_separate_from_resource_claim() {
    let resource = ResourceRef::concrete("v1".to_string(), "Secret".to_string());
    let guards = vec![Guard::NotEq {
        path: "auth.username".to_string(),
        value: GuardValue::string("postgres"),
    }];
    let mut contract = ContractIr::default();
    contract.push_dependency_use(ContractUse::with_provenances(
        "auth.password".to_string(),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        guards.clone(),
        None,
        vec![ContractProvenance::new(
            "<inline:utils>",
            SourceSpan::new(1844, 2122),
            vec!["common.utils.getKeyFromList".to_string()],
        )],
    ));
    contract.push(ContractUse::new(
        "auth.password".to_string(),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        guards,
        Some(resource),
    ));

    let value_uses = contract.finalize();
    let value_uses = value_uses.uses();

    sim_assert_eq!(have: value_uses.len(), want: 2);
    assert!(value_uses.iter().any(|value_use| {
        value_use.resource.is_none()
            && value_use
                .provenance
                .iter()
                .any(|site| site.helper_chain == vec!["common.utils.getKeyFromList".to_string()])
    }));
    assert!(
        value_uses
            .iter()
            .any(|value_use| value_use.resource.is_some())
    );
}

#[test]
fn contract_ir_maps_value_paths_without_touching_rendered_yaml_path() {
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![
            Guard::Truthy {
                path: "serviceAccount.enabled".to_string(),
            },
            Guard::Or {
                paths: vec!["pod.enabled".to_string(), "global.enabled".to_string()],
            },
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: "serviceAccount.create".to_string(),
                    }],
                    vec![Guard::Eq {
                        path: "serviceAccount.mode".to_string(),
                        value: crate::GuardValue::string("managed"),
                    }],
                ],
            },
        ],
        None,
    ));

    contract.map_value_paths(|path| {
        if path.starts_with("global.") {
            path.to_string()
        } else {
            format!("subchart.{path}")
        }
    });

    let value_uses = contract.finalize();
    let value_uses = value_uses.uses();
    let value_use = value_uses.first().expect("mapped value use");

    sim_assert_eq!(have: value_use.source_expr, want: "subchart.serviceAccount.name");
    sim_assert_eq!(
        have: value_use.path,
        want: YamlPath(vec!["metadata".to_string(), "name".to_string()])
    );
    sim_assert_eq!(
        have: value_use.guards,
        want: vec![
            Guard::Truthy {
                path: "subchart.serviceAccount.enabled".to_string(),
            },
            Guard::Or {
                paths: vec![
                    "global.enabled".to_string(),
                    "subchart.pod.enabled".to_string(),
                ],
            },
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: "subchart.serviceAccount.create".to_string(),
                    }],
                    vec![Guard::Eq {
                        path: "subchart.serviceAccount.mode".to_string(),
                        value: crate::GuardValue::string("managed"),
                    }],
                ],
            },
        ]
    );
}

#[test]
fn contract_ir_pathless_scalar_seed_projects_without_rendered_path() {
    let mut contract = ContractIr::default();

    contract.push_pathless_scalar("extraConfig");

    let finalized = contract.finalize();
    let value_uses = finalized.uses();
    sim_assert_eq!(have: value_uses.len(), want: 1);
    sim_assert_eq!(have: value_uses[0].source_expr, want: "extraConfig");
    sim_assert_eq!(have: value_uses[0].path, want: YamlPath(Vec::new()));
    sim_assert_eq!(have: value_uses[0].kind, want: ValueKind::Scalar);
    assert!(value_uses[0].guards.is_empty());
    assert!(value_uses[0].resource.is_none());
}

#[test]
fn contract_ir_carries_declared_type_hints_through_mapping_and_signal_derivation() {
    let mut contract = ContractIr::default();
    contract.add_type_hint("image.tag", "string");
    contract.add_type_hint("image.tag", "string");
    contract.add_type_hint("image.pullPolicy", "string");

    contract.map_value_paths(|path| format!("subchart.{path}"));

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(
        have: signals
            .evidence_for("subchart.image.tag")
            .map(|evidence| &evidence.type_hints),
        want: Some(&["string".to_string()].into_iter().collect())
    );
    sim_assert_eq!(
        have: signals
            .evidence_for("subchart.image.pullPolicy")
            .map(|evidence| &evidence.type_hints),
        want: Some(&["string".to_string()].into_iter().collect())
    );
    assert!(
        signals
            .evidence_for("subchart.image")
            .is_some_and(|evidence| evidence.facts.has_referenced_descendants),
        "declared type hints should still mark ancestor object paths as having referenced descendants"
    );
}

#[test]
fn contract_ir_declared_type_hints_do_not_project_as_contract_rows() {
    let mut contract = ContractIr::default();
    contract.add_type_hint("image.tag", "string");

    let finalized = contract.finalize();

    assert!(
        finalized.uses().is_empty(),
        "declared type hints should stay internal to the contract artifact: {finalized:#?}"
    );
}

#[test]
fn contract_ir_finalize_derives_projection_and_signals_from_one_normalized_contract() {
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        "feature".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Default {
            path: "feature".to_string(),
        }],
        None,
    ));
    contract.add_type_hint("feature", "string");

    let finalized = contract.clone().finalize();

    sim_assert_eq!(have: finalized.uses(), want: contract.clone().finalize().uses());
    sim_assert_eq!(have: finalized.schema_signals(), want: &contract.finalize().into_schema_signals());
}
