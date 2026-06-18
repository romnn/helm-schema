use crate::{ContractIr, ContractUse, Guard, ResourceRef, ValueKind, YamlPath};

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

    let value_uses = contract.project();
    let value_uses = value_uses.uses();

    assert_eq!(value_uses.len(), 1);
    assert_eq!(
        value_uses.first().map(|value_use| &value_use.guards),
        Some(&vec![Guard::Default {
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
        Some(ResourceRef {
            api_version: "v1".to_string(),
            kind: "Service".to_string(),
            api_version_candidates: Vec::new(),
            api_version_branches: Vec::new(),
        }),
    ));

    let value_uses = contract.project();
    let value_uses = value_uses.uses();

    assert_eq!(value_uses.len(), 1);
    assert_eq!(
        value_uses
            .first()
            .and_then(|value_use| value_use.resource.as_ref())
            .map(|resource| (resource.api_version.as_str(), resource.kind.as_str())),
        Some(("v1", "Service"))
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

    let value_uses = contract.project();
    let value_uses = value_uses.uses();
    let value_use = value_uses.first().expect("mapped value use");

    assert_eq!(value_use.source_expr, "subchart.serviceAccount.name");
    assert_eq!(
        value_use.path,
        YamlPath(vec!["metadata".to_string(), "name".to_string()])
    );
    assert_eq!(
        value_use.guards,
        vec![
            Guard::Truthy {
                path: "subchart.serviceAccount.enabled".to_string(),
            },
            Guard::Or {
                paths: vec![
                    "subchart.pod.enabled".to_string(),
                    "global.enabled".to_string()
                ],
            },
        ]
    );
}

#[test]
fn contract_ir_pathless_scalar_seed_projects_without_rendered_path() {
    let mut contract = ContractIr::default();

    contract.push_pathless_scalar("extraConfig");

    let projection = contract.project();
    let value_uses = projection.uses();
    assert_eq!(value_uses.len(), 1);
    assert_eq!(value_uses[0].source_expr, "extraConfig");
    assert_eq!(value_uses[0].path, YamlPath(Vec::new()));
    assert_eq!(value_uses[0].kind, ValueKind::Scalar);
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

    let signals = contract.into_schema_signals();
    assert_eq!(
        signals.type_hints_by_value_path.get("subchart.image.tag"),
        Some(&["string".to_string()].into_iter().collect())
    );
    assert_eq!(
        signals
            .type_hints_by_value_path
            .get("subchart.image.pullPolicy"),
        Some(&["string".to_string()].into_iter().collect())
    );
    assert!(
        signals
            .value_path_facts
            .get("subchart.image")
            .is_some_and(|facts| facts.has_referenced_descendants),
        "declared type hints should still mark ancestor object paths as having referenced descendants"
    );
}
