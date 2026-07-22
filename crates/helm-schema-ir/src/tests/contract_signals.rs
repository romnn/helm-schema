use std::collections::BTreeSet;

use crate::contract::{ContractIr, ContractUse};
use crate::{Guard, GuardValue, ResourceRef, SymbolicIrContext, ValueKind, YamlPath};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{ConditionalGuard, ContractSchemaSignals, MetadataFieldKind};
use test_util::prelude::sim_assert_eq;

#[derive(Debug, Clone, PartialEq, Eq)]
struct FlattenedConditionalOverlay {
    target_value_path: String,
    guards: Vec<ConditionalGuard>,
    evidence: helm_schema_core::ConditionalOverlayEvidence,
    preserve_base_schema: bool,
}

fn signals_for(uses: Vec<ContractUse>) -> ContractSchemaSignals {
    ContractIr::from_contract_uses(uses)
        .finalize()
        .into_schema_signals()
}

fn signals_for_template(source: &str) -> ContractSchemaSignals {
    let defines = DefineIndex::new();
    SymbolicIrContext::new(&defines)
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals()
}

fn nullable_paths_for(signals: &ContractSchemaSignals) -> BTreeSet<String> {
    signals
        .schema_evidence_by_value_path()
        .iter()
        .filter(|(_, evidence)| evidence.facts.is_nullable)
        .map(|(path, _)| path.clone())
        .collect()
}

fn provider_schema_uses_for(signals: &ContractSchemaSignals) -> Vec<&crate::ProviderSchemaUse> {
    signals
        .schema_evidence_by_value_path()
        .values()
        .flat_map(|evidence| evidence.provider_schema_uses.iter())
        .collect()
}

fn conditional_overlays_for(signals: &ContractSchemaSignals) -> Vec<FlattenedConditionalOverlay> {
    signals
        .schema_evidence_by_value_path()
        .iter()
        .flat_map(|(target_value_path, evidence)| {
            evidence
                .conditional_overlays
                .iter()
                .cloned()
                .map(|overlay| FlattenedConditionalOverlay {
                    target_value_path: target_value_path.clone(),
                    guards: overlay.guards,
                    evidence: overlay.evidence,
                    preserve_base_schema: overlay.preserve_base_schema,
                })
        })
        .collect()
}

#[test]
fn contract_ir_nullable_paths_include_range_only_collection() {
    let signals = signals_for(vec![ContractUse::new(
        "snapshot".to_string(),
        YamlPath(vec!["data".to_string(), "command".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Range {
            path: "snapshots".to_string(),
        }],
        None,
    )]);
    let nullable_paths = nullable_paths_for(&signals);

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
    let signals = signals_for(vec![
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
    ]);
    let nullable_paths = nullable_paths_for(&signals);

    assert!(
        !nullable_paths.contains("serviceAccount.name"),
        "one guarded render use must not make a bare render site nullable: {nullable_paths:?}",
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete evidence scenario is clearest as one contiguous test"
)]
fn contract_ir_path_evidence_collects_references_and_typed_guard_predicates() {
    let signals = signals_for(vec![
        ContractUse::new(
            "podLabels".to_string(),
            YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            ValueKind::Fragment,
            vec![
                Guard::Eq {
                    path: "mode".to_string(),
                    value: GuardValue::string("prod"),
                },
                Guard::TypeIs {
                    path: "extraConfig".to_string(),
                    schema_type: "string".to_string(),
                },
                Guard::Truthy {
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
                value: GuardValue::string("ignored"),
            }],
            None,
        ),
    ]);
    let evidence = signals.schema_evidence_by_value_path();

    sim_assert_eq!(
        have: evidence
            .iter()
            .filter(|(_, evidence)| evidence.is_referenced_value_path)
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>(),
        want: BTreeSet::from([
            "extraConfig".to_string(),
            "extraEnv".to_string(),
            "image.tag".to_string(),
            "mode".to_string(),
            "podLabels".to_string(),
            "podName".to_string(),
            "podNamespace".to_string(),
        ]),
    );
    sim_assert_eq!(
        have: evidence
            .iter()
            .filter(|(_, evidence)| evidence.facts.is_ranged_source)
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>(),
        want: BTreeSet::new(),
        "a control guard does not prove the path itself was the iterable",
    );
    sim_assert_eq!(
        have: evidence
            .iter()
            .filter(|(_, evidence)| evidence.facts.used_as_fragment)
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>(),
        want: BTreeSet::from(["podLabels".to_string()]),
    );
    sim_assert_eq!(
        have: evidence
            .iter()
            .filter(|(_, evidence)| evidence.facts.is_partial_scalar_value_path)
            .map(|(path, _)| path.clone())
            .collect::<BTreeSet<_>>(),
        want: BTreeSet::from(["image.tag".to_string()]),
    );
    sim_assert_eq!(
        have: evidence
            .get("podLabels")
            .map(|evidence| &evidence.metadata_field_kinds),
        want: Some(&BTreeSet::new()),
        "guarded metadata typing must not bind the unconditional path",
    );
    assert!(
        evidence.get("podLabels").is_some_and(|evidence| {
            evidence.conditional_overlays.iter().any(|overlay| {
                overlay
                    .evidence
                    .metadata_field_kinds
                    .contains(&MetadataFieldKind::StringMap)
            })
        }),
        "guarded metadata typing should stay on its conditional overlay",
    );
    sim_assert_eq!(
        have: evidence
            .get("podName")
            .map(|evidence| &evidence.metadata_field_kinds),
        want: Some(&BTreeSet::from([MetadataFieldKind::Name])),
    );
    sim_assert_eq!(
        have: evidence
            .get("podNamespace")
            .map(|evidence| &evidence.metadata_field_kinds),
        want: Some(&BTreeSet::from([MetadataFieldKind::Namespace])),
    );
    sim_assert_eq!(
        have: evidence
            .get("mode")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::Eq {
            path: "mode".to_string(),
            value: GuardValue::string("prod"),
        }]),
    );
    sim_assert_eq!(
        have: evidence
            .get("extraConfig")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::TypeIs {
            path: "extraConfig".to_string(),
            schema_type: "string".to_string(),
        }]),
    );
    assert!(
        !evidence
            .get("ignored.guard")
            .is_some_and(|evidence| evidence.is_referenced_value_path),
        "empty-source inspection rows should not seed schema paths",
    );
    assert!(
        !evidence.contains_key(""),
        "empty-source inspection rows should not seed metadata facts",
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete evidence scenario is clearest as one contiguous test"
)]
fn contract_ir_path_evidence_preserves_values_decidable_guard_predicate_shapes() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::With {
                path: "feature.config".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: "feature.disabled".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::NotEq {
                path: "feature.mode".to_string(),
                value: GuardValue::string("off"),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Absent {
                path: "feature.name".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Or {
                paths: vec![
                    "feature.primary".to_string(),
                    "feature.secondary".to_string(),
                ],
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::AnyOf {
                alternatives: vec![
                    vec![
                        Guard::Truthy {
                            path: "feature.managed".to_string(),
                        },
                        Guard::Eq {
                            path: "feature.tier".to_string(),
                            value: GuardValue::string("prod"),
                        },
                    ],
                    vec![Guard::Not {
                        path: "feature.skip".to_string(),
                    }],
                ],
            }],
            None,
        ),
    ]);
    let evidence = signals.schema_evidence_by_value_path();

    sim_assert_eq!(
        have: evidence
            .get("feature.enabled")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::Truthy {
            path: "feature.enabled".to_string(),
        }]),
    );
    sim_assert_eq!(
        have: evidence
            .get("feature.config")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::With {
            path: "feature.config".to_string(),
        }]),
    );
    sim_assert_eq!(
        have: evidence
            .get("feature.disabled")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::Not(Box::new(
            ConditionalGuard::Truthy {
                path: "feature.disabled".to_string(),
            },
        ))]),
    );
    sim_assert_eq!(
        have: evidence
            .get("feature.mode")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::NotEq {
            path: "feature.mode".to_string(),
            value: GuardValue::string("off"),
        }]),
    );
    sim_assert_eq!(
        have: evidence
            .get("feature.name")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![ConditionalGuard::Absent {
            path: "feature.name".to_string(),
        }]),
    );
    let disjunction = ConditionalGuard::AnyOf(vec![
        ConditionalGuard::Truthy {
            path: "feature.primary".to_string(),
        },
        ConditionalGuard::Truthy {
            path: "feature.secondary".to_string(),
        },
    ]);
    sim_assert_eq!(
        have: evidence
            .get("feature.primary")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![disjunction.clone()]),
    );
    sim_assert_eq!(
        have: evidence
            .get("feature.secondary")
            .map(|evidence| &evidence.guard_predicates),
        want: Some(&vec![disjunction]),
    );
    let nested_disjunction = ConditionalGuard::AnyOf(vec![
        ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
            path: "feature.skip".to_string(),
        })),
        ConditionalGuard::AllOf(vec![
            ConditionalGuard::Truthy {
                path: "feature.managed".to_string(),
            },
            ConditionalGuard::Eq {
                path: "feature.tier".to_string(),
                value: GuardValue::string("prod"),
            },
        ]),
    ]);
    for path in ["feature.managed", "feature.tier", "feature.skip"] {
        sim_assert_eq!(
            have: evidence
                .get(path)
                .map(|evidence| &evidence.guard_predicates),
            want: Some(&vec![nested_disjunction.clone()]),
            "expected the full nested predicate to be preserved for {path}",
        );
    }
}

#[test]
fn contract_ir_provider_schema_uses_are_rendered_resource_claims_only() {
    let resource = ResourceRef::concrete("apps/v1".to_string(), "Deployment".to_string());
    let signals = signals_for(vec![
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
    ]);
    let requests = provider_schema_uses_for(&signals);

    sim_assert_eq!(have: requests.len(), want: 2, "{requests:#?}");
    sim_assert_eq!(have: requests[0].value_path, want: "containers");
    sim_assert_eq!(have: requests[0].kind, want: ValueKind::Fragment);
    assert!(!requests[0].is_self_range_collection);
    sim_assert_eq!(have: requests[1].value_path, want: "ports");
    sim_assert_eq!(have: requests[1].kind, want: ValueKind::Scalar);
    assert!(requests[1].is_self_range_collection);
}

#[test]
fn contract_ir_schema_signals_bundle_core_generation_facts() {
    let resource = ResourceRef::concrete("apps/v1".to_string(), "Deployment".to_string());
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

    sim_assert_eq!(
        have: signals
            .evidence_for("podLabels")
            .map(|evidence| &evidence.metadata_field_kinds),
        want: Some(&BTreeSet::from([MetadataFieldKind::StringMap])),
    );
    assert!(
        signals
            .evidence_for("serviceAccount.name")
            .is_some_and(|evidence| evidence.facts.is_nullable),
        "default-guarded render use should surface as nullable contract evidence",
    );
    assert!(
        signals
            .evidence_for("serviceAccount")
            .is_some_and(|evidence| evidence.facts.has_referenced_descendants),
        "contract schema signals should own descendant path topology",
    );
    assert!(
        signals
            .evidence_for("serviceAccount.name")
            .is_some_and(|evidence| evidence.facts.has_render_use
                && evidence.facts.all_render_uses_self_guarded),
        "contract value-path facts should own render-use evidence",
    );
    assert!(
        signals
            .evidence_for("serviceAccount")
            .is_some_and(|evidence| evidence.facts.has_referenced_descendants),
        "contract value-path facts should own descendant path topology",
    );
    assert!(
        signals
            .evidence_for("serviceAccount.name")
            .is_some_and(|evidence| evidence.facts.has_render_use
                && evidence.facts.all_render_uses_self_guarded
                && evidence.facts.is_nullable),
        "contract value-path facts should bundle nullable render-use evidence",
    );
    let pod_labels_evidence = signals
        .evidence_for("podLabels")
        .expect("podLabels evidence");
    sim_assert_eq!(have: pod_labels_evidence.value_path, want: "podLabels");
    sim_assert_eq!(
        have: pod_labels_evidence.metadata_field_kinds,
        want: BTreeSet::from([MetadataFieldKind::StringMap]),
        "path evidence should carry metadata lowering facts",
    );
    sim_assert_eq!(
        have: pod_labels_evidence.provider_schema_uses.len(),
        want: 1,
        "path evidence should carry provider-schema requests for that path only",
    );
    let service_account_evidence = signals
        .evidence_for("serviceAccount.name")
        .expect("serviceAccount.name evidence");
    assert!(service_account_evidence.is_referenced_value_path);
    assert!(
        service_account_evidence.facts.has_render_use
            && service_account_evidence.facts.all_render_uses_self_guarded
            && service_account_evidence.facts.is_nullable,
        "path evidence should carry render/nullability facts",
    );
    let service_account_parent_evidence = signals
        .evidence_for("serviceAccount")
        .expect("serviceAccount parent evidence");
    assert!(
        !service_account_parent_evidence.is_referenced_value_path,
        "ancestor-only fact rows must not become schema subjects",
    );
    sim_assert_eq!(have: provider_schema_uses_for(&signals).len(), want: 2);
}

#[test]
fn contract_ir_conditional_path_overlays_capture_single_supported_guard_set() {
    let signals = signals_for(vec![ContractUse::new(
        "feature.host".to_string(),
        YamlPath(vec!["spec".to_string(), "host".to_string()]),
        ValueKind::Scalar,
        vec![
            Guard::With {
                path: "feature".to_string(),
            },
            Guard::Truthy {
                path: "feature.enabled".to_string(),
            },
        ],
        None,
    )]);

    let overlays = conditional_overlays_for(&signals);
    let overlay = overlays.first().expect("expected conditional overlay");
    sim_assert_eq!(
        have: overlay.target_value_path,
        want: "feature.host",
        "overlay should stay keyed by the values path being lowered"
    );
    sim_assert_eq!(
        have: overlay.guards,
        want: vec![
            ConditionalGuard::Truthy {
                path: "feature.enabled".to_string(),
            },
            ConditionalGuard::With {
                path: "feature".to_string(),
            },
        ],
    );
    assert!(
        overlay.evidence.provider_schema_uses.is_empty(),
        "non-resource scalar overlays should not invent provider lookups"
    );
    assert!(
        overlay.evidence.metadata_field_kinds.is_empty(),
        "non-metadata target should not carry metadata-field lowering hints"
    );
    sim_assert_eq!(
        have: overlay.evidence.facts.has_render_use,
        want: true,
        "branch-local facts should preserve the target's render-use status"
    );
}

#[test]
fn contract_ir_conditional_path_overlays_ignore_self_default_guards_beside_boolean_guards() {
    let signals = signals_for(vec![ContractUse::new(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![
            Guard::Truthy {
                path: "serviceAccount.create".to_string(),
            },
            Guard::Default {
                path: "serviceAccount.name".to_string(),
            },
        ],
        None,
    )]);

    let overlays = conditional_overlays_for(&signals);
    let overlay = overlays.first().expect("expected conditional overlay");
    sim_assert_eq!(
        have: overlay.guards,
        want: vec![ConditionalGuard::Truthy {
            path: "serviceAccount.create".to_string(),
        }],
        "self-default guards should not suppress an otherwise lowerable boolean branch",
    );
    assert!(
        overlay.evidence.facts.is_nullable,
        "branch-local nullability should still reflect the self-defaulted render use",
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete overlay scenario is clearest as one contiguous test"
)]
fn contract_ir_conditional_path_overlays_preserve_values_decidable_not_and_or() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: "feature.enabled".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "other.host".to_string(),
            YamlPath(vec!["spec".to_string(), "other".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Or {
                paths: vec!["first.enabled".to_string(), "second.enabled".to_string()],
            }],
            None,
        ),
        ContractUse::new(
            "preset.resources".to_string(),
            YamlPath(vec!["spec".to_string(), "resources".to_string()]),
            ValueKind::Fragment,
            vec![Guard::NotEq {
                path: "resourcesPreset".to_string(),
                value: GuardValue::string("none"),
            }],
            None,
        ),
        ContractUse::new(
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "image".to_string()]),
            ValueKind::Scalar,
            vec![Guard::AnyOf {
                alternatives: vec![
                    vec![
                        Guard::Truthy {
                            path: "image.enabled".to_string(),
                        },
                        Guard::Eq {
                            path: "image.mode".to_string(),
                            value: GuardValue::string("managed"),
                        },
                    ],
                    vec![Guard::Not {
                        path: "global.imageDisabled".to_string(),
                    }],
                ],
            }],
            None,
        ),
    ]);

    let overlays = conditional_overlays_for(&signals);
    sim_assert_eq!(have: overlays.len(), want: 4);
    let feature_overlay = overlays
        .iter()
        .find(|overlay| overlay.target_value_path == "feature.host")
        .expect("feature.host overlay");
    let other_overlay = overlays
        .iter()
        .find(|overlay| overlay.target_value_path == "other.host")
        .expect("other.host overlay");
    let preset_overlay = overlays
        .iter()
        .find(|overlay| overlay.target_value_path == "preset.resources")
        .expect("preset.resources overlay");
    let image_overlay = overlays
        .iter()
        .find(|overlay| overlay.target_value_path == "image.tag")
        .expect("image.tag overlay");
    sim_assert_eq!(
        have: feature_overlay.guards,
        want: vec![ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
            path: "feature.enabled".to_string(),
        }))],
    );
    sim_assert_eq!(
        have: other_overlay.guards,
        want: vec![ConditionalGuard::AnyOf(vec![
            ConditionalGuard::Truthy {
                path: "first.enabled".to_string(),
            },
            ConditionalGuard::Truthy {
                path: "second.enabled".to_string(),
            },
        ])],
    );
    sim_assert_eq!(
        have: preset_overlay.guards,
        want: vec![ConditionalGuard::NotEq {
            path: "resourcesPreset".to_string(),
            value: GuardValue::string("none"),
        }],
    );
    sim_assert_eq!(
        have: image_overlay.guards,
        want: vec![ConditionalGuard::AnyOf(vec![
            ConditionalGuard::Not(Box::new(ConditionalGuard::Truthy {
                path: "global.imageDisabled".to_string(),
            })),
            ConditionalGuard::AllOf(vec![
                ConditionalGuard::Truthy {
                    path: "image.enabled".to_string(),
                },
                ConditionalGuard::Eq {
                    path: "image.mode".to_string(),
                    value: GuardValue::string("managed"),
                },
            ]),
        ])],
    );
    assert!(
        overlays.iter().all(|overlay| !overlay.preserve_base_schema),
        "guarded-only branches must not preserve branch-specific evidence on the global base path: {overlays:?}"
    );
}

#[test]
fn contract_ir_conditional_path_overlays_preserve_multiple_guarded_variants_per_path() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.value".to_string(),
            YamlPath(vec!["metadata".to_string(), "name".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("name"),
            }],
            None,
        ),
        ContractUse::new(
            "feature.value".to_string(),
            YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
            ValueKind::Fragment,
            vec![Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("labels"),
            }],
            None,
        ),
    ]);

    let overlays = conditional_overlays_for(&signals);
    sim_assert_eq!(
        have: overlays.len(),
        want: 2,
        "multiple supported guard sets for the same values path should survive as separate overlays"
    );
    assert!(
        overlays.iter().any(|overlay| {
            overlay.guards
                == vec![ConditionalGuard::Eq {
                    path: "mode".to_string(),
                    value: GuardValue::string("name"),
                }]
                && overlay.evidence.metadata_field_kinds
                    == BTreeSet::from([MetadataFieldKind::Name])
        }),
        "expected a metadata.name-targeted branch overlay"
    );
    assert!(
        overlays.iter().any(|overlay| {
            overlay.guards
                == vec![ConditionalGuard::Eq {
                    path: "mode".to_string(),
                    value: GuardValue::string("labels"),
                }]
                && overlay.evidence.metadata_field_kinds
                    == BTreeSet::from([MetadataFieldKind::StringMap])
                && overlay.evidence.facts.used_as_fragment
        }),
        "expected a metadata.labels fragment branch overlay"
    );
}

#[test]
fn contract_ir_unconditional_use_subsumes_matching_guarded_overlay() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            "other.path".to_string(),
            YamlPath(vec!["spec".to_string(), "other".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Range {
                path: "other.items".to_string(),
            }],
            None,
        ),
    ]);

    let overlays = conditional_overlays_for(&signals);
    sim_assert_eq!(
        have: overlays.len(),
        want: 0,
        "the guarded use adds no evidence beyond the identical unconditional use: {:?}",
        overlays
    );
    assert!(
        signals
            .schema_evidence_by_value_path()
            .get("feature.host")
            .is_some_and(|evidence| evidence.facts.has_unconditional_render_use),
        "the surviving use should remain unconditional",
    );
    assert!(
        !overlays
            .iter()
            .any(|overlay| overlay.target_value_path == "other.path"),
        "unsupported range-guarded paths must still stay on the wide/base path: {overlays:?}"
    );
}

#[test]
fn contract_ir_conditional_path_overlays_drop_base_only_for_complete_boolean_partition() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![
                Guard::Truthy {
                    path: "feature.enabled".to_string(),
                },
                Guard::Truthy {
                    path: "app.enabled".to_string(),
                },
            ],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![
                Guard::Truthy {
                    path: "feature.enabled".to_string(),
                },
                Guard::Not {
                    path: "app.enabled".to_string(),
                },
            ],
            None,
        ),
    ]);

    let overlays = conditional_overlays_for(&signals);
    // Equal-evidence complementary branches resolve into their shared key:
    // T under (A && B) or (A && !B) is T under A.
    sim_assert_eq!(
        have: overlays.len(),
        want: 1,
        "complementary equal-evidence branches should resolve into one: {:?}",
        overlays
    );
    sim_assert_eq!(
        have: overlays[0].guards,
        want: vec![ConditionalGuard::Truthy {
            path: "feature.enabled".to_string(),
        }],
    );
    assert!(
        overlays.iter().all(|overlay| !overlay.preserve_base_schema),
        "a complete truthy/not partition should be allowed to replace the base schema entirely: {overlays:?}"
    );
}

#[test]
fn contract_ir_conditional_path_overlays_drop_base_for_partition_with_common_prefix_branch() {
    let signals = signals_for(vec![
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![
                Guard::Truthy {
                    path: "feature.enabled".to_string(),
                },
                Guard::Truthy {
                    path: "app.enabled".to_string(),
                },
            ],
            None,
        ),
        ContractUse::new(
            "feature.host".to_string(),
            YamlPath(vec!["spec".to_string(), "host".to_string()]),
            ValueKind::Scalar,
            vec![
                Guard::Truthy {
                    path: "feature.enabled".to_string(),
                },
                Guard::Not {
                    path: "app.enabled".to_string(),
                },
            ],
            None,
        ),
    ]);

    let overlays = conditional_overlays_for(&signals);
    // The complementary sub-branches resolve into the broad branch they
    // partition, leaving one branch keyed on the shared condition.
    sim_assert_eq!(
        have: overlays.len(),
        want: 1,
        "the partition should collapse into the broad shared branch: {:?}",
        overlays
    );
    sim_assert_eq!(
        have: overlays[0].guards,
        want: vec![ConditionalGuard::Truthy {
            path: "feature.enabled".to_string(),
        }],
    );
    assert!(
        overlays.iter().all(|overlay| !overlay.preserve_base_schema),
        "a shared broad branch plus a truthy/not partition should still replace the base schema: {overlays:?}"
    );
}

#[test]
fn contract_ir_derives_schema_signals_without_projection_detour() {
    let resource = ResourceRef::concrete("v1".to_string(), "ServiceAccount".to_string());
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

    let direct_signals = contract.finalize().into_schema_signals();

    assert!(
        direct_signals
            .evidence_for("serviceAccount.name")
            .is_some_and(|evidence| evidence.facts.is_nullable),
        "semantic finalization should keep the default-guarded render claim",
    );
    sim_assert_eq!(have: provider_schema_uses_for(&direct_signals).len(), want: 1);
    assert!(
        direct_signals
            .evidence_for("podLabels")
            .is_some_and(|evidence| evidence
                .metadata_field_kinds
                .contains(&MetadataFieldKind::StringMap)),
    );
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete requiredness scenario is clearest as one contiguous test"
)]
fn contract_ir_requiredness_evidence_is_path_local() {
    let signals = ContractIr::from_contract_uses(vec![
        ContractUse::new(
            "feature.enabled".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }],
            None,
        ),
        ContractUse::new(
            "mode".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            vec![Guard::Eq {
                path: "mode".to_string(),
                value: GuardValue::string("strict"),
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
            "resourcesPreset".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            vec![Guard::NotEq {
                path: "resourcesPreset".to_string(),
                value: GuardValue::string("none"),
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
    .finalize()
    .into_schema_signals();

    let evidence = signals.schema_evidence_by_value_path();

    assert!(
        evidence
            .get("feature.enabled")
            .is_some_and(|evidence| evidence.requiredness.is_positive_header)
    );
    assert!(
        evidence
            .get("mode")
            .is_some_and(|evidence| evidence.requiredness.is_positive_header)
    );
    assert!(
        evidence
            .get("optional")
            .is_some_and(|evidence| evidence.requiredness.is_conditionally_optional)
    );
    assert!(
        evidence
            .get("resourcesPreset")
            .is_some_and(|evidence| evidence.requiredness.is_conditionally_optional)
    );
    assert!(
        evidence
            .get("either.primary")
            .is_some_and(|evidence| evidence.requiredness.is_conditionally_optional)
    );
    assert!(
        evidence
            .get("either.fallback")
            .is_some_and(|evidence| evidence.requiredness.is_conditionally_optional)
    );
    assert!(
        evidence
            .get("defaulted")
            .is_some_and(|evidence| evidence.requiredness.has_default_fallback)
    );
    assert!(
        evidence
            .get("ranged")
            .is_some_and(|evidence| !evidence.requiredness.is_positive_header)
    );
}

#[test]
fn contract_ir_requiredness_evidence_ignores_pathless_scalar_non_headers() {
    let signals = ContractIr::from_contract_uses(vec![
        ContractUse::new(
            "rendered.value".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            Vec::new(),
            None,
        ),
        ContractUse::new(
            "helper.dependency".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            vec![Guard::With {
                path: "helper.scope".to_string(),
            }],
            None,
        ),
    ])
    .finalize()
    .into_schema_signals();

    assert!(
        signals
            .schema_evidence_by_value_path()
            .values()
            .all(|evidence| !evidence.requiredness.is_positive_header),
        "plain pathless scalar uses must not be treated as positive header facts: {:#?}",
        signals.schema_evidence_by_value_path()
    );
}

#[test]
fn unsupported_conditional_row_does_not_promote_sink_evidence() {
    let signals = signals_for_template(
        r#"
{{- if semverCompare ">=1.0.0" .Values.version }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ .Values.name }}
{{- end }}
"#,
    );
    assert!(
        signals
            .evidence_for("name")
            .is_none_or(|evidence| evidence.provider_schema_uses.is_empty()),
        "a sink hidden behind an unlowerable condition cannot constrain the global path: {:#?}",
        signals.schema_evidence_by_value_path(),
    );
    assert!(
        signals
            .evidence_for("name")
            .is_none_or(|evidence| evidence.metadata_field_kinds.is_empty()),
        "branch-local metadata typing cannot escape an unlowerable condition: {:#?}",
        signals.schema_evidence_by_value_path(),
    );
    assert!(
        signals
            .evidence_for("name")
            .is_none_or(|evidence| evidence.conditional_overlays.is_empty()),
        "an unlowerable condition cannot be represented as a conditional overlay: {:#?}",
        signals.schema_evidence_by_value_path(),
    );
}

#[test]
fn foreign_range_does_not_globalize_strict_consumer() {
    let signals = signals_for_template(
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
data:
  keys: |
    {{- range .Values.items }}
    {{ keys $.Values.config | join "," }}
    {{- end }}
"#,
    );
    let evidence = signals
        .evidence_for("config")
        .expect("strict consumer evidence");

    assert!(
        !evidence.fail_implications.is_empty()
            && evidence.fail_implications.iter().all(|implication| {
                implication.outer_guards.iter().any(|guard| {
                    matches!(
                        guard,
                        helm_schema_core::ConditionalGuard::Truthy { path } if path == "items"
                    )
                })
            }),
        "a strict call that only executes inside a foreign range binds only behind \
         the iteration's liveness (the body executed, so the direct collection is \
         truthy), never globally: {evidence:#?}",
    );
}

#[test]
fn nested_member_range_abstains_under_unlowerable_outer_guard() {
    let signals = signals_for_template(
        r#"
{{- if semverCompare ">=1.0.0" .Values.version }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
data:
  values: |
    {{- range $group := .Values.groups }}
    {{- range $item := $group }}
    {{ $item }}
    {{- end }}
    {{- end }}
{{- end }}
"#,
    );
    assert!(
        signals.evidence_for("groups").is_none_or(|evidence| {
            !evidence.fail_implications.iter().any(|implication| {
                matches!(
                    implication.target,
                    helm_schema_core::ContractRequirementTarget::Members { .. }
                )
            })
        }),
        "a nested range cannot impose a member contract after its outer guard was lost: {:#?}",
        signals.schema_evidence_by_value_path(),
    );
}

#[test]
fn unlowerable_mixed_guard_retains_its_values_path_reference() {
    let signals = signals_for_template(
        r#"
{{- if .Values.alertmanager.enabled }}
{{- if .Values.alertmanager.ingress.enabled }}
{{- if and .Values.alertmanager.ingress.className (semverCompare ">=1.18-0" .Capabilities.KubeVersion.GitVersion) }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
data:
  class: {{ .Values.alertmanager.ingress.className }}
{{- end }}
{{- end }}
{{- end }}
"#,
    );

    let evidence = signals
        .evidence_for("alertmanager.ingress.className")
        .unwrap_or_else(|| panic!("mixed guard path reference disappeared: {signals:#?}"));
    assert!(
        evidence.provider_schema_uses.is_empty(),
        "the opaque semver arm must still block provider typing: {evidence:#?}"
    );
}

#[test]
fn member_row_without_direct_range_identity_does_not_seed_schema_paths() {
    let signals = signals_for(vec![ContractUse::new(
        "$sentinel.*".to_string(),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        vec![Guard::Range {
            path: "$sentinel".to_string(),
        }],
        None,
    )]);

    assert!(signals.evidence_for("$sentinel").is_none());
    assert!(signals.evidence_for("$sentinel.*").is_none());
}

#[test]
fn direct_ranged_nested_sentinel_retains_its_member_contract() {
    let signals = signals_for_template(
        r#"
apiVersion: v1
kind: ConfigMap
metadata:
  name: test
data:
  rendered: |
    {{- range $entry := .Values.entries }}
    {{ tpl (get $entry "$tplYaml") $ }}
    {{- end }}
"#,
    );

    let evidence = signals
        .evidence_for("entries.*.$tplYaml")
        .unwrap_or_else(|| {
            panic!(
                "direct ranged sentinel member must survive: {:#?}",
                signals.schema_evidence_by_value_path()
            )
        });
    assert!(
        evidence.facts.has_string_contract,
        "tpl must retain its strict string contract on the nested sentinel: {evidence:#?}"
    );
    assert!(signals.evidence_for("$tplYaml").is_none());
    assert!(signals.evidence_for("$tplYaml.*").is_none());
}

#[test]
fn get_on_destructured_range_value_requires_object_members() {
    let signals = signals_for_template(
        r#"
{{- range $name, $context := .Values.contexts }}
{{- $_ := get $context "creds" }}
{{- end }}
"#,
    );
    let evidence = signals
        .evidence_for("contexts")
        .expect("direct range evidence");

    assert!(evidence.fail_implications.iter().any(|implication| {
        matches!(
            implication.target,
            helm_schema_core::ContractRequirementTarget::Members {
                allow_integer: false
            }
        ) && implication.requirements
            == vec![helm_schema_core::FailValueRequirement::SchemaType(
                "object".to_string(),
            )]
    }));
}
