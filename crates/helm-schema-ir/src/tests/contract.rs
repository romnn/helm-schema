use crate::{
    ContractIr, ContractProvenance, ContractUse, Guard, GuardValue, ResourceRef, SourceSpan,
    SymbolicIrContext, ValueKind, YamlPath,
};
use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_ast::DefineIndex;
use helm_schema_core::{MergeLayer, MergeLayerTransform, MergeLayersUse};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

fn conditional_path(value: &str) -> helm_schema_core::ValuesPath {
    helm_schema_core::ValuesPath::parse(value)
}

fn absorb_captures(
    contract: &mut ContractIr,
    captures: impl IntoIterator<Item = crate::eval_effect::FailCapture>,
) {
    let mut observed_facts = crate::observed_facts::ObservedFacts::default();
    observed_facts.captures.extend(captures);
    contract.absorb_observed_facts(&observed_facts);
}

#[test]
fn contract_ir_finalization_keeps_default_guarded_render_site_over_bare_duplicate() {
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        helm_schema_core::ValuesPath::parse("serviceAccount.name"),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    ));
    contract.push(ContractUse::new(
        helm_schema_core::ValuesPath::parse("serviceAccount.name"),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Default {
            path: helm_schema_core::ValuesPath::parse("serviceAccount.name"),
        }],
        None,
    ));

    let value_uses = contract.finalize();
    let value_uses = value_uses.uses();

    sim_assert_eq!(have: value_uses.len(), want: 1);
    sim_assert_eq!(
        have: value_uses.first().map(ContractUse::single_guard_conjunction),
        want: Some(vec![Guard::Default {
            path: helm_schema_core::ValuesPath::parse("serviceAccount.name"),
        }])
    );
}

#[test]
fn contract_ir_finalization_prefers_resource_claim_for_pathless_duplicate() {
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        helm_schema_core::ValuesPath::parse("nameOverride"),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        Vec::new(),
        None,
    ));
    contract.push(ContractUse::new(
        helm_schema_core::ValuesPath::parse("nameOverride"),
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
        path: helm_schema_core::ValuesPath::parse("auth.username"),
        value: GuardValue::string("postgres"),
    }];
    let mut contract = ContractIr::default();
    contract.push_dependency_use(ContractUse::with_provenances(
        helm_schema_core::ValuesPath::parse("auth.password"),
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
        helm_schema_core::ValuesPath::parse("auth.password"),
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
fn contract_ir_maps_value_paths_without_touching_rendered_yaml_path() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    let mut contract_use = ContractUse::new(
        helm_schema_core::ValuesPath::parse("serviceAccount.name"),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![
            Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("serviceAccount.enabled"),
            },
            Guard::Or {
                paths: vec![
                    helm_schema_core::ValuesPath::parse("pod.enabled"),
                    helm_schema_core::ValuesPath::parse("global.enabled"),
                ],
            },
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: helm_schema_core::ValuesPath::parse("serviceAccount.create"),
                    }],
                    vec![Guard::Eq {
                        path: helm_schema_core::ValuesPath::parse("serviceAccount.mode"),
                        value: crate::GuardValue::string("managed"),
                    }],
                ],
            },
        ],
        None,
    );
    contract_use.merge_layers = Some(
        MergeLayersUse::new(
            vec![
                MergeLayer {
                    path: conditional_path("serviceAccount.name"),
                    transform: MergeLayerTransform::ParsedMap,
                },
                MergeLayer {
                    path: conditional_path("global.serviceAccount.name"),
                    transform: MergeLayerTransform::Identity,
                },
            ],
            0,
            true,
        )
        .ok_or_eyre("valid merge-layer position")?,
    );
    contract_use.omitted_members.insert(
        "automountServiceAccountToken".to_string(),
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("serviceAccount.keepAutomount"),
        }],
    );
    contract.push(contract_use);

    contract.map_value_paths(|path| {
        if path
            .segments()
            .next()
            .and_then(helm_schema_core::Segment::literal)
            == Some("global")
        {
            path
        } else {
            helm_schema_core::ValuesPath::from_segments(
                std::iter::once(helm_schema_core::Segment::from("subchart"))
                    .chain(path.segments().cloned()),
            )
        }
    });

    let value_uses = contract.finalize();
    let value_uses = value_uses.uses();
    let value_use = value_uses.first().ok_or_eyre("mapped value use")?;

    sim_assert_eq!(
        have: value_use.source_expr,
        want: conditional_path("subchart.serviceAccount.name")
    );
    sim_assert_eq!(
        have: value_use.path,
        want: YamlPath(vec!["metadata".to_string(), "name".to_string()])
    );
    sim_assert_eq!(
        have: value_use.single_guard_conjunction(),
        want: vec![
            Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("subchart.serviceAccount.enabled"),
            },
            Guard::Or {
                paths: vec![
                    helm_schema_core::ValuesPath::parse("global.enabled"),
                    helm_schema_core::ValuesPath::parse("subchart.pod.enabled"),
                ],
            },
            Guard::AnyOf {
                alternatives: vec![
                    vec![Guard::Truthy {
                        path: helm_schema_core::ValuesPath::parse("subchart.serviceAccount.create"),
                    }],
                    vec![Guard::Eq {
                        path: helm_schema_core::ValuesPath::parse("subchart.serviceAccount.mode"),
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
            .map(|merge| {
                merge
                    .layers()
                    .iter()
                    .map(|layer| layer.path.clone())
                    .collect::<Vec<_>>()
            }),
        want: Some(
            vec![
                conditional_path("subchart.serviceAccount.name"),
                conditional_path("global.serviceAccount.name"),
            ]
        )
    );
    sim_assert_eq!(
        have: value_use
            .omitted_members
            .get("automountServiceAccountToken")
            .map(Vec::as_slice),
        want: Some(
            [Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("subchart.serviceAccount.keepAutomount"),
            }]
            .as_slice()
        )
    );
    Ok(())
}

#[test]
fn dependency_global_projection_keeps_parent_override_and_child_fallback_arms() {
    let mut contract = ContractIr::from_contract_uses(vec![ContractUse::new(
        helm_schema_core::ValuesPath::parse("metrics.global.imageRegistry"),
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
                conditional_path("global.imageRegistry"),
                vec![
                    Guard::NotEq {
                        path: helm_schema_core::ValuesPath::parse("global.imageRegistry"),
                        value: helm_schema_core::GuardValue::Null,
                    },
                    Guard::HasKey {
                        path: helm_schema_core::ValuesPath::parse("global"),
                        key: "imageRegistry".to_string(),
                    },
                ]
            ),
            (
                conditional_path("metrics.global.imageRegistry"),
                vec![Guard::AnyOf {
                    alternatives: vec![
                        vec![Guard::Eq {
                            path: helm_schema_core::ValuesPath::parse("global.imageRegistry"),
                            value: helm_schema_core::GuardValue::Null,
                        }],
                        vec![Guard::NotHasKey {
                            path: helm_schema_core::ValuesPath::parse("global"),
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
        helm_schema_core::ValuesPath::parse("metrics.agent.global.imageRegistry"),
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
                conditional_path("global.imageRegistry"),
                vec![
                    Guard::NotEq {
                        path: helm_schema_core::ValuesPath::parse("global.imageRegistry"),
                        value: helm_schema_core::GuardValue::Null,
                    },
                    Guard::HasKey {
                        path: helm_schema_core::ValuesPath::parse("global"),
                        key: "imageRegistry".to_string(),
                    },
                ]
            ),
            (
                conditional_path("metrics.agent.global.imageRegistry"),
                vec![
                    Guard::AnyOf {
                        alternatives: vec![
                            vec![Guard::Eq {
                                path: helm_schema_core::ValuesPath::parse("global.imageRegistry"),
                                value: helm_schema_core::GuardValue::Null,
                            }],
                            vec![Guard::NotHasKey {
                                path: helm_schema_core::ValuesPath::parse("global"),
                                key: "imageRegistry".to_string(),
                            }],
                        ],
                    },
                    Guard::AnyOf {
                        alternatives: vec![
                            vec![Guard::Eq {
                                path: helm_schema_core::ValuesPath::parse("metrics.global.imageRegistry"),
                                value: helm_schema_core::GuardValue::Null,
                            }],
                            vec![Guard::NotHasKey {
                                path: helm_schema_core::ValuesPath::parse("metrics.global"),
                                key: "imageRegistry".to_string(),
                            }],
                        ],
                    },
                ]
            ),
            (
                conditional_path("metrics.global.imageRegistry"),
                vec![
                    Guard::NotEq {
                        path: helm_schema_core::ValuesPath::parse("metrics.global.imageRegistry"),
                        value: helm_schema_core::GuardValue::Null,
                    },
                    Guard::AnyOf {
                        alternatives: vec![
                            vec![Guard::Eq {
                                path: helm_schema_core::ValuesPath::parse("global.imageRegistry"),
                                value: helm_schema_core::GuardValue::Null,
                            }],
                            vec![Guard::NotHasKey {
                                path: helm_schema_core::ValuesPath::parse("global"),
                                key: "imageRegistry".to_string(),
                            }],
                        ],
                    },
                    Guard::HasKey {
                        path: helm_schema_core::ValuesPath::parse("metrics.global"),
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
    sim_assert_eq!(have: value_uses[0].source_expr, want: conditional_path("extraConfig"));
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

    contract.map_value_paths(|path| {
        helm_schema_core::ValuesPath::from_segments(
            std::iter::once(helm_schema_core::Segment::from("subchart"))
                .chain(path.segments().cloned()),
        )
    });

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(
        have: signals
            .evidence_for(&helm_schema_core::ValuesPath::parse("subchart.image.tag"))
            .map(|evidence| &evidence.type_hints),
        want: Some(&["string".to_string()].into_iter().collect())
    );
    sim_assert_eq!(
        have: signals
            .evidence_for(&helm_schema_core::ValuesPath::parse(
                "subchart.image.pullPolicy",
            ))
            .map(|evidence| &evidence.type_hints),
        want: Some(&["string".to_string()].into_iter().collect())
    );
    assert!(
        signals
            .evidence_for(&helm_schema_core::ValuesPath::parse("subchart.image"))
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
        helm_schema_core::ValuesPath::parse("feature"),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Default {
            path: helm_schema_core::ValuesPath::parse("feature"),
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
    contract.map_value_paths(|path| {
        helm_schema_core::ValuesPath::from_segments(
            ["metrics", "agent"]
                .into_iter()
                .map(helm_schema_core::Segment::from)
                .chain(path.segments().cloned()),
        )
    });
    contract.project_dependency_global_contracts(&["metrics".to_string(), "agent".to_string()]);

    let finalized = contract.finalize();
    sim_assert_eq!(
        have: finalized
            .uses()
            .iter()
            .map(|contract_use| contract_use.source_expr.encode())
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
    contract.map_value_paths(|path| {
        helm_schema_core::ValuesPath::from_segments(
            ["metrics", "agent"]
                .into_iter()
                .map(helm_schema_core::Segment::from)
                .chain(path.segments().cloned()),
        )
    });
    contract.project_dependency_global_contracts(&["metrics".to_string(), "agent".to_string()]);

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(
        have: signals.direct_ranged_value_paths().clone(),
        want: std::collections::BTreeSet::from([
            helm_schema_core::ValuesPath::parse("global"),
            helm_schema_core::ValuesPath::parse("metrics.global"),
            helm_schema_core::ValuesPath::parse("metrics.agent.global"),
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
        .filter(|contract_use| contract_use.source_expr == conditional_path("fallback"))
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
                    paths: vec![
                        helm_schema_core::ValuesPath::parse("fallback"),
                        helm_schema_core::ValuesPath::parse("primary"),
                    ],
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
                        path: helm_schema_core::ValuesPath::parse("fallback"),
                    },
                    Guard::Not {
                        path: helm_schema_core::ValuesPath::parse("primary"),
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
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![
                helm_schema_core::Predicate::truthy_path("auth.enabled"),
                helm_schema_core::Predicate::truthy_path("auth.usePassword"),
            ],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::Fail,
        }],
    );

    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("redis.enabled"),
    }]);

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(
        have: signals.terminal_clauses(),
        want: &[vec![
            helm_schema_core::ConditionalGuard::Truthy {
                path: conditional_path("auth.enabled"),
            },
            helm_schema_core::ConditionalGuard::Truthy {
                path: conditional_path("auth.usePassword"),
            },
            helm_schema_core::ConditionalGuard::Truthy {
                path: conditional_path("redis.enabled"),
            },
        ]]
    );
}

#[test]
fn contract_ir_activation_guards_scope_runtime_string_contracts() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path("image.repository"),
                route: crate::eval_effect::StringRequirementRoute::Direct,
                selection: Vec::new(),
            },
        }],
    );
    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("postgresql.enabled"),
    }]);

    let finalized = contract.finalize();
    sim_assert_eq!(have: finalized.uses().len(), want: 0);

    let evidence = finalized
        .schema_signals()
        .evidence_for(&helm_schema_core::ValuesPath::parse("image.repository"))
        .ok_or_eyre("expected scoped string-contract evidence")?;
    sim_assert_eq!(have: evidence.facts.has_string_contract, want: false);
    sim_assert_eq!(have: evidence.type_hints.contains("string"), want: false);
    sim_assert_eq!(
        have: evidence.requirement_implications.clone(),
        want: vec![helm_schema_core::ContractRequirementImplication {
            outer_guards: vec![helm_schema_core::ConditionalGuard::Truthy {
                path: conditional_path("postgresql.enabled"),
            }],
            target: helm_schema_core::ContractRequirementTarget::Value,
            requirements: vec![helm_schema_core::FailValueRequirement::SchemaType(
                "string".to_string(),
            )],
        }]
    );
    Ok(())
}

#[test]
fn activation_guards_scope_values_default_sources() {
    let mut contract = ContractIr::default();
    let mut facts = crate::observed_facts::ObservedFacts::default();
    facts
        .values_default_sources
        .insert(crate::ValuesDefaultSource {
            target_path: conditional_path("child"),
            source_path: conditional_path("child.defaults"),
        });
    contract.absorb_observed_facts(&facts);
    contract.push(ContractUse::new(
        helm_schema_core::ValuesPath::parse("child.token.value"),
        YamlPath(vec!["data".to_string(), "token".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    ));
    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("child.enabled"),
    }]);

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(have: signals.values_default_sources().len(), want: 0);
    sim_assert_eq!(
        have: signals.guarded_values_default_sources(),
        want: &std::collections::BTreeSet::from([
            helm_schema_core::GuardedValuesDefaultSource {
                outer_guards: vec![helm_schema_core::ConditionalGuard::Truthy {
                    path: conditional_path("child.enabled"),
                }],
                source: crate::ValuesDefaultSource {
                    target_path: conditional_path("child"),
                    source_path: conditional_path("child.defaults"),
                },
            },
        ])
    );
}

#[test]
fn activation_drops_a_default_source_without_same_template_consumers() {
    let mut contract = ContractIr::default();
    let mut facts = crate::observed_facts::ObservedFacts::default();
    facts
        .values_default_sources
        .insert(crate::ValuesDefaultSource {
            target_path: conditional_path("child"),
            source_path: conditional_path("child.defaults"),
        });
    contract.absorb_observed_facts(&facts);
    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("child.enabled"),
    }]);

    let signals = contract.finalize().into_schema_signals();
    sim_assert_eq!(have: signals.values_default_sources().len(), want: 0);
    sim_assert_eq!(
        have: signals.guarded_values_default_sources().len(),
        want: 0
    );
}

#[test]
fn nested_activation_conjoins_every_default_source_guard() {
    let mut contract = ContractIr::default();
    let mut facts = crate::observed_facts::ObservedFacts::default();
    facts
        .values_default_sources
        .insert(crate::ValuesDefaultSource {
            target_path: conditional_path("mid.leaf"),
            source_path: conditional_path("mid.leaf.defaults"),
        });
    contract.absorb_observed_facts(&facts);
    contract.push(ContractUse::new(
        helm_schema_core::ValuesPath::parse("mid.leaf.token.value"),
        YamlPath(vec!["data".to_string(), "token".to_string()]),
        ValueKind::Scalar,
        Vec::new(),
        None,
    ));
    contract.append_guards_to_all_uses(&[
        Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("mid.enabled"),
        },
        Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("mid.leaf.enabled"),
        },
    ]);

    let signals = contract.finalize().into_schema_signals();
    let activation_paths = signals
        .guarded_values_default_sources()
        .iter()
        .flat_map(|fact| fact.outer_guards.iter())
        .flat_map(helm_schema_core::ConditionalGuard::value_paths)
        .collect::<std::collections::BTreeSet<_>>();
    sim_assert_eq!(
        have: activation_paths,
        want: std::collections::BTreeSet::from([
            helm_schema_core::ValuesPath::parse("mid.enabled"),
            helm_schema_core::ValuesPath::parse("mid.leaf.enabled"),
        ])
    );
}

#[test]
fn activation_guards_scope_dependency_root_overlay_twins() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path("child.name"),
                route: crate::eval_effect::StringRequirementRoute::Direct,
                selection: Vec::new(),
            },
        }],
    );
    let mut facts = crate::observed_facts::ObservedFacts::default();
    facts
        .values_root_overlays
        .insert(crate::observed_facts::ValuesRootOverlay {
            target_path: helm_schema_core::ValuesPath::parse("child"),
            source_path: helm_schema_core::ValuesPath::parse("child.profile"),
        });
    contract.absorb_observed_facts(&facts);
    contract.append_guards_to_all_uses(&[Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("child.enabled"),
    }]);

    let signals = contract.finalize().into_schema_signals();
    let evidence = signals
        .evidence_for(&helm_schema_core::ValuesPath::parse("child.profile.name"))
        .ok_or_eyre("expected activated root-overlay twin")?;
    sim_assert_eq!(
        have: evidence.requirement_implications.clone(),
        want: vec![helm_schema_core::ContractRequirementImplication {
            outer_guards: vec![helm_schema_core::ConditionalGuard::Truthy {
                path: conditional_path("child.enabled"),
            }],
            target: helm_schema_core::ContractRequirementTarget::Value,
            requirements: vec![helm_schema_core::FailValueRequirement::SchemaType(
                "string".to_string(),
            )],
        }]
    );
    Ok(())
}

#[test]
fn selected_string_requirement_does_not_retype_a_broader_row() -> eyre::Result<()> {
    let path = "config.value";
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        conditional_path(path),
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("config.enabled"),
        }],
        None,
    ));
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![
                helm_schema_core::Predicate::Guard(Guard::Truthy {
                    path: helm_schema_core::ValuesPath::parse("config.enabled"),
                }),
                helm_schema_core::Predicate::Guard(Guard::TypeIs {
                    path: helm_schema_core::ValuesPath::parse(path),
                    schema_type: "string".to_string(),
                }),
            ],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path(path),
                route: crate::eval_effect::StringRequirementRoute::Scoped,
                selection: Vec::new(),
            },
        }],
    );

    let finalized = contract.finalize();
    let evidence = finalized
        .schema_signals()
        .evidence_for(&helm_schema_core::ValuesPath::parse(path))
        .ok_or_eyre("expected selected string evidence")?;
    sim_assert_eq!(have: evidence.facts.has_string_contract, want: false);
    sim_assert_eq!(have: evidence.type_hints.contains("string"), want: false);
    Ok(())
}

#[test]
fn scoped_string_requirement_suppresses_only_the_matching_provider_route() {
    let path = "config.name";
    let mut contract = ContractIr::default();
    for (gate, slot) in [("first.enabled", "first"), ("second.enabled", "second")] {
        let row = ContractUse::new(
            conditional_path(path),
            YamlPath(vec!["metadata".to_string(), slot.to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse(gate),
            }],
            Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
        );
        contract.push(row);
    }
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![helm_schema_core::Predicate::truthy_path("first.enabled")],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path(path),
                route: crate::eval_effect::StringRequirementRoute::Scoped,
                selection: Vec::new(),
            },
        }],
    );

    let evidence = contract.finalize().into_schema_signals();
    let provider_paths = evidence
        .evidence_for(&helm_schema_core::ValuesPath::parse(path))
        .into_iter()
        .flat_map(|evidence| {
            evidence
                .conditional_overlays
                .iter()
                .flat_map(|overlay| overlay.evidence.provider_schema_uses.iter())
        })
        .map(|provider_use| provider_use.path.clone())
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: provider_paths,
        want: vec![YamlPath(vec!["metadata".to_string(), "second".to_string()])]
    );
}

#[test]
fn scoped_string_requirement_matches_a_logically_implied_disjunction() {
    let path = "config.name";
    let mut contract = ContractIr::default();
    contract.push(ContractUse::new(
        conditional_path(path),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("selected"),
        }],
        Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
    ));
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![helm_schema_core::Predicate::Or(vec![
                helm_schema_core::Predicate::truthy_path("selected"),
                helm_schema_core::Predicate::truthy_path("fallback"),
            ])],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path(path),
                route: crate::eval_effect::StringRequirementRoute::Scoped,
                selection: Vec::new(),
            },
        }],
    );

    let signals = contract.finalize().into_schema_signals();
    let evidence = signals
        .evidence_for(&helm_schema_core::ValuesPath::parse(path))
        .expect("config.name evidence");
    assert!(
        evidence.provider_schema_uses.is_empty(),
        "the selected disjunction arm proves that the scoped string consumer owns this provider route: {evidence:#?}"
    );
}

#[test]
fn direct_string_requirement_suppresses_only_transformed_provider_preimages() {
    let path = "config.name";
    let mut contract = ContractIr::default();
    for (slot, stringified) in [("transformed", true), ("raw", false)] {
        let mut row = ContractUse::new(
            conditional_path(path),
            YamlPath(vec!["metadata".to_string(), slot.to_string()]),
            ValueKind::Scalar,
            Vec::new(),
            Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
        );
        row.stringified = stringified;
        contract.push(row);
    }
    contract.push(ContractUse::new(
        conditional_path(path),
        YamlPath::default(),
        ValueKind::YamlSerialized,
        Vec::new(),
        None,
    ));
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path(path),
                route: crate::eval_effect::StringRequirementRoute::Direct,
                selection: Vec::new(),
            },
        }],
    );

    let evidence = contract.finalize().into_schema_signals();
    let provider_paths = evidence
        .evidence_for(&helm_schema_core::ValuesPath::parse(path))
        .into_iter()
        .flat_map(|evidence| evidence.provider_schema_uses.iter())
        .map(|provider_use| provider_use.path.clone())
        .collect::<Vec<_>>();
    sim_assert_eq!(
        have: provider_paths,
        want: vec![YamlPath(vec!["metadata".to_string(), "raw".to_string()])]
    );
}

#[test]
fn scoped_string_requirement_projects_recursive_merge_fallback_rows() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    let mut row = ContractUse::new(
        helm_schema_core::ValuesPath::parse("workers"),
        YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
        ValueKind::YamlSerialized,
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("workers.enabled"),
        }],
        Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
    );
    row.merge_layers = Some(
        MergeLayersUse::new(
            [
                "workers.celery.sets.*.query",
                "workers.celery.query",
                "workers",
            ]
            .into_iter()
            .map(|path| MergeLayer {
                path: conditional_path(path),
                transform: MergeLayerTransform::Identity,
            })
            .collect(),
            2,
            false,
        )
        .ok_or_eyre("valid merge-layer position")?,
    );
    contract.push(row);
    absorb_captures(
        &mut contract,
        [
            crate::eval_effect::FailCapture {
                conjunction: vec![helm_schema_core::Predicate::truthy_path("workers.enabled")],
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::StringRequirement {
                    path: conditional_path("workers.celery.query"),
                    route: crate::eval_effect::StringRequirementRoute::Scoped,
                    selection: Vec::new(),
                },
            },
            crate::eval_effect::FailCapture {
                conjunction: vec![helm_schema_core::Predicate::truthy_path("workers.enabled")],
                ranged: crate::range_modes::RangeModes::default(),
                kind: crate::eval_effect::CaptureKind::StringRequirement {
                    path: conditional_path("workers.celery.sets.*.query"),
                    route: crate::eval_effect::StringRequirementRoute::Scoped,
                    selection: Vec::new(),
                },
            },
        ],
    );

    let finalized = contract.finalize();
    sim_assert_eq!(
        have: finalized
            .uses()
            .first()
            .map(|row| (row.source_expr.clone(), row.kind)),
        want: Some((conditional_path("workers.query"), ValueKind::YamlSerialized))
    );
    Ok(())
}

#[test]
fn unrelated_string_requirement_keeps_recursive_merge_source() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    let mut row = ContractUse::new(
        helm_schema_core::ValuesPath::parse("ports"),
        YamlPath(vec!["spec".to_string(), "ports".to_string()]),
        ValueKind::YamlSerialized,
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("deployment.enabled"),
        }],
        Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
    );
    row.merge_layers = Some(
        MergeLayersUse::new(
            ["ports.overrides.*.port", "ports.port", "ports"]
                .into_iter()
                .map(|path| MergeLayer {
                    path: conditional_path(path),
                    transform: MergeLayerTransform::Identity,
                })
                .collect(),
            2,
            false,
        )
        .ok_or_eyre("valid merge-layer position")?,
    );
    contract.push(row);
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![helm_schema_core::Predicate::truthy_path(
                "deployment.enabled",
            )],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path("ports.*.protocol"),
                route: crate::eval_effect::StringRequirementRoute::Selected,
                selection: vec![helm_schema_core::Predicate::truthy_path("ports.*.protocol")],
            },
        }],
    );

    let finalized = contract.finalize();
    sim_assert_eq!(
        have: finalized.uses().first().map(|row| row.source_expr.clone()),
        want: Some(conditional_path("ports"))
    );
    Ok(())
}

#[test]
fn dormant_string_requirement_keeps_recursive_merge_fallback_source() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    let mut row = ContractUse::new(
        helm_schema_core::ValuesPath::parse("workers"),
        YamlPath(vec!["metadata".to_string(), "labels".to_string()]),
        ValueKind::YamlSerialized,
        vec![Guard::Not {
            path: helm_schema_core::ValuesPath::parse("workers.enabled"),
        }],
        Some(ResourceRef::concrete("v1".to_string(), "Pod".to_string())),
    );
    row.merge_layers = Some(
        MergeLayersUse::new(
            [
                "workers.celery.sets.*.query",
                "workers.celery.query",
                "workers",
            ]
            .into_iter()
            .map(|path| MergeLayer {
                path: conditional_path(path),
                transform: MergeLayerTransform::Identity,
            })
            .collect(),
            2,
            false,
        )
        .ok_or_eyre("valid merge-layer position")?,
    );
    contract.push(row);
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![helm_schema_core::Predicate::truthy_path("workers.enabled")],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path("workers.celery.query"),
                route: crate::eval_effect::StringRequirementRoute::Scoped,
                selection: Vec::new(),
            },
        }],
    );

    let finalized = contract.finalize();
    sim_assert_eq!(
        have: finalized.uses().first().map(|row| row.source_expr.clone()),
        want: Some(conditional_path("workers"))
    );
    Ok(())
}

#[test]
fn propagated_wildcard_string_requirement_needs_its_range_scope() {
    let mut contract = ContractIr::default();
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: vec![helm_schema_core::Predicate::truthy_path(
                "workers.celery.enabled",
            )],
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path("workers.*"),
                route: crate::eval_effect::StringRequirementRoute::Selected,
                selection: Vec::new(),
            },
        }],
    );

    let finalized = contract.finalize();
    let signals = finalized.schema_signals();
    assert!(
        signals
            .evidence_for(&helm_schema_core::ValuesPath::parse("workers"))
            .is_none_or(|evidence| {
                evidence.requirement_implications.iter().all(|implication| {
                    !matches!(
                        implication.target,
                        helm_schema_core::ContractRequirementTarget::Members { .. }
                    )
                })
            }),
        "a helper-propagated wildcard identity must not classify every member without its range: {signals:#?}"
    );
}

#[test]
fn ranged_wildcard_string_requirement_keeps_its_member_contract() -> eyre::Result<()> {
    let mut contract = ContractIr::default();
    absorb_captures(
        &mut contract,
        [crate::eval_effect::FailCapture {
            conjunction: Vec::new(),
            ranged: crate::range_modes::RangeModes::default(),
            kind: crate::eval_effect::CaptureKind::StringRequirement {
                path: conditional_path("workers.*"),
                route: crate::eval_effect::StringRequirementRoute::Selected,
                selection: vec![helm_schema_core::Predicate::Guard(Guard::Range {
                    path: helm_schema_core::ValuesPath::parse("workers"),
                })],
            },
        }],
    );

    let finalized = contract.finalize();
    let evidence = finalized
        .schema_signals()
        .evidence_for(&helm_schema_core::ValuesPath::parse("workers"))
        .ok_or_eyre("expected ranged worker evidence")?;
    assert!(
        evidence.requirement_implications.iter().any(|implication| {
            matches!(
                implication.target,
                helm_schema_core::ContractRequirementTarget::Members { .. }
            ) && implication.requirements
                == vec![helm_schema_core::FailValueRequirement::SchemaType(
                    "string".to_string(),
                )]
        }),
        "the exact member range must retain its string contract: {evidence:#?}"
    );
    Ok(())
}
