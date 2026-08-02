use crate::{
    ContractIr, ContractProvenance, ContractUse, Guard, GuardValue, ResourceRef, SourceSpan,
    SymbolicIrContext, ValueKind, YamlPath,
};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use indoc::indoc;
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
        have: value_uses.first().map(ContractUse::single_guard_conjunction),
        want: Some(vec![Guard::Default {
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
#[expect(
    clippy::too_many_lines,
    reason = "the single scenario compares path rewriting across every correlated contract field"
)]
fn contract_ir_maps_value_paths_without_touching_rendered_yaml_path() {
    let mut contract = ContractIr::default();
    let mut contract_use = ContractUse::new(
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
    );
    contract_use.merge_layers = Some(helm_schema_core::MergeLayersUse {
        layers: vec![
            "serviceAccount.name".to_string(),
            "global.serviceAccount.name".to_string(),
        ],
        position: 0,
        transforms: vec![
            helm_schema_core::MergeLayerTransform::ParsedMap,
            helm_schema_core::MergeLayerTransform::Identity,
        ],
        via_binding: true,
    });
    contract_use.omitted_members.insert(
        "automountServiceAccountToken".to_string(),
        vec![Guard::Truthy {
            path: "serviceAccount.keepAutomount".to_string(),
        }],
    );
    contract.push(contract_use);

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
        have: value_use.single_guard_conjunction(),
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
    sim_assert_eq!(
        have: value_use
            .merge_layers
            .as_ref()
            .map(|merge| merge.layers.as_slice()),
        want: Some(
            [
                "subchart.serviceAccount.name".to_string(),
                "global.serviceAccount.name".to_string(),
            ]
            .as_slice()
        )
    );
    sim_assert_eq!(
        have: value_use
            .omitted_members
            .get("automountServiceAccountToken")
            .map(Vec::as_slice),
        want: Some(
            [Guard::Truthy {
                path: "subchart.serviceAccount.keepAutomount".to_string(),
            }]
            .as_slice()
        )
    );
}

#[test]
fn dependency_global_projection_keeps_parent_override_and_child_fallback_arms() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse::new(
        "metrics.global.imageRegistry".to_string(),
        YamlPath(vec!["data".to_string(), "registry".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    )]);

    contract.project_dependency_global_contracts(&["metrics".to_string()]);

    let finalized = contract.finalize();
    sim_assert_eq!(have: finalized.uses().len(), want: 2);
    sim_assert_eq!(
        have: finalized
            .uses()
            .iter()
            .map(|contract_use| (
                contract_use.source_expr.clone(),
                contract_use.single_guard_conjunction()
            ))
            .collect::<Vec<_>>(),
        want: vec![
            (
                "global.imageRegistry".to_string(),
                vec![
                    Guard::NotEq {
                        path: "global.imageRegistry".to_string(),
                        value: helm_schema_core::GuardValue::Null,
                    },
                    Guard::HasKey {
                        path: "global".to_string(),
                        key: "imageRegistry".to_string(),
                    },
                ]
            ),
            (
                "metrics.global.imageRegistry".to_string(),
                vec![Guard::AnyOf {
                    alternatives: vec![
                        vec![Guard::Eq {
                            path: "global.imageRegistry".to_string(),
                            value: helm_schema_core::GuardValue::Null,
                        }],
                        vec![Guard::NotHasKey {
                            path: "global".to_string(),
                            key: "imageRegistry".to_string(),
                        }],
                    ],
                }]
            ),
        ]
    );
}

#[test]
fn nested_dependency_global_projection_partitions_every_ancestor_source() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse::new(
        "metrics.agent.global.imageRegistry".to_string(),
        YamlPath(vec!["data".to_string(), "registry".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    )]);

    contract.project_dependency_global_contracts(&["metrics".to_string(), "agent".to_string()]);

    let finalized = contract.finalize();
    sim_assert_eq!(
        have: finalized
            .uses()
            .iter()
            .map(|contract_use| (
                contract_use.source_expr.clone(),
                contract_use.single_guard_conjunction()
            ))
            .collect::<Vec<_>>(),
        want: vec![
            (
                "global.imageRegistry".to_string(),
                vec![
                    Guard::NotEq {
                        path: "global.imageRegistry".to_string(),
                        value: helm_schema_core::GuardValue::Null,
                    },
                    Guard::HasKey {
                        path: "global".to_string(),
                        key: "imageRegistry".to_string(),
                    },
                ]
            ),
            (
                "metrics.agent.global.imageRegistry".to_string(),
                vec![
                    Guard::AnyOf {
                        alternatives: vec![
                            vec![Guard::Eq {
                                path: "global.imageRegistry".to_string(),
                                value: helm_schema_core::GuardValue::Null,
                            }],
                            vec![Guard::NotHasKey {
                                path: "global".to_string(),
                                key: "imageRegistry".to_string(),
                            }],
                        ],
                    },
                    Guard::AnyOf {
                        alternatives: vec![
                            vec![Guard::Eq {
                                path: "metrics.global.imageRegistry".to_string(),
                                value: helm_schema_core::GuardValue::Null,
                            }],
                            vec![Guard::NotHasKey {
                                path: "metrics.global".to_string(),
                                key: "imageRegistry".to_string(),
                            }],
                        ],
                    },
                ]
            ),
            (
                "metrics.global.imageRegistry".to_string(),
                vec![
                    Guard::NotEq {
                        path: "metrics.global.imageRegistry".to_string(),
                        value: helm_schema_core::GuardValue::Null,
                    },
                    Guard::AnyOf {
                        alternatives: vec![
                            vec![Guard::Eq {
                                path: "global.imageRegistry".to_string(),
                                value: helm_schema_core::GuardValue::Null,
                            }],
                            vec![Guard::NotHasKey {
                                path: "global".to_string(),
                                key: "imageRegistry".to_string(),
                            }],
                        ],
                    },
                    Guard::HasKey {
                        path: "metrics.global".to_string(),
                        key: "imageRegistry".to_string(),
                    },
                ]
            ),
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
    assert!(value_uses[0].single_guard_conjunction().is_empty());
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

#[test]
fn dependency_global_projection_moves_range_members_to_live_sources() {
    let defines = DefineIndex::new();
    let mut contract = SymbolicIrContext::new(&defines).generate_contract_ir(indoc! {"
        {{- range .Values.global.imagePullSecrets }}
        {{ .name }}
        {{- end }}
    "});
    contract.map_value_paths(|path| format!("metrics.agent.{path}"));
    contract.project_dependency_global_contracts(&["metrics".to_string(), "agent".to_string()]);

    let finalized = contract.finalize();
    sim_assert_eq!(
        have: finalized
            .uses()
            .iter()
            .map(|contract_use| contract_use.source_expr.clone())
            .filter(|path| path.contains("imagePullSecrets"))
            .collect::<std::collections::BTreeSet<_>>(),
        want: std::collections::BTreeSet::from([
            "global.imagePullSecrets".to_string(),
            "global.imagePullSecrets.*.name".to_string(),
            "metrics.global.imagePullSecrets".to_string(),
            "metrics.global.imagePullSecrets.*.name".to_string(),
            "metrics.agent.global.imagePullSecrets".to_string(),
            "metrics.agent.global.imagePullSecrets.*.name".to_string(),
        ])
    );
}

#[test]
fn dependency_global_projection_keeps_whole_global_range_modes() {
    let defines = DefineIndex::new();
    let mut contract = SymbolicIrContext::new(&defines).generate_contract_ir(indoc! {"
        {{- range .Values.global }}
        {{ . }}
        {{- end }}
    "});
    contract.map_value_paths(|path| format!("metrics.agent.{path}"));
    contract.project_dependency_global_contracts(&["metrics".to_string(), "agent".to_string()]);

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(
        have: signals.direct_ranged_value_paths().clone(),
        want: std::collections::BTreeSet::from([
            "global".to_string(),
            "metrics.global".to_string(),
            "metrics.agent.global".to_string(),
        ])
    );
}

#[test]
fn with_header_candidates_do_not_inherit_the_body_sink() {
    let defines = DefineIndex::new();
    let finalized = SymbolicIrContext::new(&defines)
        .generate_contract_ir(indoc! {"
            apiVersion: apps/v1
            kind: Deployment
            metadata:
              name: probe
            spec:
              template:
                spec:
                  {{- with .Values.primary | default .Values.fallback }}
                  priorityClassName: {{ . }}
                  {{- end }}
        "})
        .finalize();

    let fallback_uses = finalized
        .uses()
        .iter()
        .filter(|contract_use| contract_use.source_expr == "fallback")
        .map(|contract_use| {
            (
                contract_use.path.clone(),
                contract_use.resource.is_some(),
                contract_use.single_guard_conjunction(),
            )
        })
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: fallback_uses,
        want: vec![
            (
                YamlPath(Vec::new()),
                false,
                vec![Guard::Or {
                    paths: vec!["fallback".to_string(), "primary".to_string()],
                }],
            ),
            (
                YamlPath(vec![
                    "spec".to_string(),
                    "template".to_string(),
                    "spec".to_string(),
                    "priorityClassName".to_string(),
                ]),
                true,
                vec![
                    Guard::Truthy {
                        path: "fallback".to_string(),
                    },
                    Guard::Not {
                        path: "primary".to_string(),
                    },
                ],
            ),
        ]
    );
}

#[test]
fn contract_ir_activation_guards_gate_fail_captures() {
    // A cross-path `fail` conjunction from a dependency chart becomes a
    // document-level terminal clause; the dependency's `condition:`
    // activation guard must survive into that clause, or the validator
    // would reject values documents that keep the dependency disabled.
    let mut contract = ContractIr::default();
    contract.extend_fail_conditions([crate::eval_effect::FailCapture {
        conjunction: vec![
            helm_schema_core::Predicate::truthy_path("auth.enabled"),
            helm_schema_core::Predicate::truthy_path("auth.usePassword"),
        ],
        ranged: crate::range_modes::RangeModes::default(),
        kind: crate::eval_effect::CaptureKind::Fail,
    }]);

    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: "redis.enabled".to_string(),
    }]);

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(
        have: signals.terminal_clauses(),
        want: &[vec![
            helm_schema_core::ConditionalGuard::Truthy {
                path: "auth.enabled".to_string(),
            },
            helm_schema_core::ConditionalGuard::Truthy {
                path: "auth.usePassword".to_string(),
            },
            helm_schema_core::ConditionalGuard::Truthy {
                path: "redis.enabled".to_string(),
            },
        ]]
    );
}

#[test]
fn contract_ir_activation_guards_scope_runtime_string_contracts() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    contract.extend_string_contract_value_paths(["image.repository".to_string()]);
    contract.add_type_hint("image.repository", "string");
    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: "postgresql.enabled".to_string(),
    }]);

    let finalized = contract.finalize();
    sim_assert_eq!(have: finalized.uses().len(), want: 1);
    let use_ = finalized
        .uses()
        .first()
        .ok_or_eyre("expected one scoped string-contract row")?;
    sim_assert_eq!(have: use_.source_expr.as_str(), want: "image.repository");
    sim_assert_eq!(have: use_.path.clone(), want: YamlPath(Vec::new()));
    sim_assert_eq!(have: use_.has_string_contract, want: true);
    sim_assert_eq!(
        have: use_.single_guard_conjunction(),
        want: vec![Guard::Truthy {
            path: "postgresql.enabled".to_string(),
        }]
    );

    let evidence = finalized
        .schema_signals()
        .evidence_for("image.repository")
        .ok_or_eyre("expected scoped string-contract evidence")?;
    sim_assert_eq!(have: evidence.facts.has_string_contract, want: false);
    sim_assert_eq!(have: evidence.type_hints.contains("string"), want: false);
    sim_assert_eq!(have: evidence.conditional_overlays.len(), want: 1);
    let overlay = evidence
        .conditional_overlays
        .first()
        .ok_or_eyre("expected one string-contract overlay")?;
    sim_assert_eq!(
        have: overlay.guards.clone(),
        want: vec![helm_schema_core::ConditionalGuard::Truthy {
            path: "postgresql.enabled".to_string(),
        }]
    );
    sim_assert_eq!(
        have: overlay.evidence.facts.has_string_contract,
        want: true
    );
    sim_assert_eq!(
        have: overlay.evidence.type_hints.contains("string"),
        want: true
    );
    Ok(())
}
