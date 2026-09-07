use crate::{
    ContractProvenance, ContractUse, Guard, GuardValue, ResourceRef, SourceSpan, ValueKind,
    YamlPath,
};
use std::collections::BTreeSet;
use test_util::prelude::sim_assert_eq;

use super::{
    canonicalize_contract_uses, contract_predicates, drop_self_truthy_subsumed_duplicates,
    extra_predicates_are_truthy_parents, normalize_contract_uses, render_site,
};

#[test]
fn normalization_deduplicates_expanded_rows_before_subsumption() {
    let row = ContractUse::new(
        helm_schema_core::ValuesPath::parse("feature.enabled"),
        YamlPath(vec!["spec".to_string(), "enabled".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("feature.enabled"),
        }],
        None,
    );
    let uses = normalize_contract_uses(vec![row.clone(), row], Vec::new(), &[]);

    sim_assert_eq!(have: uses.len(), want: 1);
}

#[test]
fn canonicalization_merges_provenance_for_semantically_identical_uses() {
    let mut uses = vec![
        ContractUse {
            source_expr: helm_schema_core::ValuesPath::parse("image.tag"),
            path: YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: None,
            provenance: vec![ContractProvenance::new(
                "templates/a.yaml",
                SourceSpan::new(10, 20),
                Vec::new(),
            )],
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
        ContractUse {
            source_expr: helm_schema_core::ValuesPath::parse("image.tag"),
            path: YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: None,
            provenance: vec![ContractProvenance::new(
                "templates/b.yaml",
                SourceSpan::new(30, 40),
                vec!["helper.render".to_string()],
            )],
            stringified: false,
            template_supplied_member_keys: std::collections::BTreeSet::default(),
            split_segment: None,
            merge_layers: None,
            range_key: false,
            nil_omitting: false,
            omitted_members: std::collections::BTreeMap::default(),
            digest: false,
            merge_operand: false,
        },
    ];

    canonicalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(have: uses[0].provenance.len(), want: 2);
}

#[test]
fn canonicalization_keeps_range_key_and_value_rows_distinct() {
    let value_row = ContractUse::new(
        helm_schema_core::ValuesPath::parse("config.*"),
        YamlPath(vec!["data".to_string()]),
        ValueKind::PartialScalar,
        vec![Guard::Range {
            path: helm_schema_core::ValuesPath::parse("config"),
        }],
        None,
    );
    let mut key_row = value_row.clone();
    key_row.range_key = true;
    let mut uses = vec![key_row, value_row];

    canonicalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 2);
    sim_assert_eq!(
        have: uses.iter().map(|row| row.range_key).collect::<Vec<_>>(),
        want: vec![false, true]
    );
}

#[test]
fn canonicalization_merges_complementary_conditions_across_render_sites() {
    let mut uses = vec![
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("image.tag"),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("feature.enabled"),
            }],
            None,
            vec![ContractProvenance::new(
                "templates/a.yaml",
                SourceSpan::new(10, 20),
                Vec::new(),
            )],
        ),
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("image.tag"),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: helm_schema_core::ValuesPath::parse("feature.enabled"),
            }],
            None,
            vec![ContractProvenance::new(
                "templates/b.yaml",
                SourceSpan::new(30, 40),
                Vec::new(),
            )],
        ),
    ];

    canonicalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(have: uses[0].condition.guard_conjunctions(), want: vec![vec![]]);
    sim_assert_eq!(have: uses[0].provenance.len(), want: 2);
}

#[test]
fn canonicalization_collapses_conditions_from_the_same_render_site() {
    let provenance = ContractProvenance::new(
        "templates/deployment.yaml",
        SourceSpan::new(10, 20),
        vec!["helper.render".to_string()],
    );
    let mut uses = vec![
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("image.tag"),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: helm_schema_core::ValuesPath::parse("feature.enabled"),
            }],
            None,
            vec![provenance.clone()],
        ),
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("image.tag"),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: helm_schema_core::ValuesPath::parse("feature.enabled"),
            }],
            None,
            vec![provenance],
        ),
    ];

    canonicalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(have: uses[0].condition.guard_conjunctions(), want: vec![vec![]]);
}

#[test]
fn normalization_drops_same_site_branch_subsumed_by_self_truthy_branch() {
    let provenance = ContractProvenance::new(
        "<inline:utils>",
        SourceSpan::new(1195, 1576),
        vec!["common.utils.getValueFromKey".to_string()],
    );
    let resource = Some(ResourceRef::concrete(
        "v1".to_string(),
        "Secret".to_string(),
    ));
    let base_guards = vec![Guard::NotEq {
        path: helm_schema_core::ValuesPath::parse("auth.username"),
        value: GuardValue::string("postgres"),
    }];
    let mut self_truthy_guards = base_guards.clone();
    self_truthy_guards.insert(
        0,
        Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("auth.password"),
        },
    );
    let mut uses = vec![
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("auth.password"),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            base_guards,
            resource.clone(),
            vec![provenance.clone()],
        ),
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("auth.password"),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            self_truthy_guards,
            resource,
            vec![provenance],
        ),
    ];

    uses = normalize_contract_uses(uses, Vec::new(), &[]);

    sim_assert_eq!(have: uses.len(), want: 1);
    assert!(uses[0].single_guard_conjunction().iter().any(|guard| {
        matches!(guard, Guard::Truthy { path } if path.encode() == "auth.password")
    }));
}

#[test]
fn normalization_drops_subsumed_truthy_branch_across_provenance_sites() {
    let resource = Some(ResourceRef::concrete(
        "v1".to_string(),
        "Secret".to_string(),
    ));
    let base_guards = vec![Guard::NotEq {
        path: helm_schema_core::ValuesPath::parse("auth.username"),
        value: GuardValue::string("postgres"),
    }];
    let mut self_truthy_guards = base_guards.clone();
    self_truthy_guards.push(Guard::Truthy {
        path: helm_schema_core::ValuesPath::parse("auth.password"),
    });
    let mut uses = vec![
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("auth.password"),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            base_guards.clone(),
            resource.clone(),
            vec![ContractProvenance::new(
                "templates/first.yaml",
                SourceSpan::new(10, 20),
                Vec::new(),
            )],
        ),
        ContractUse::with_provenances(
            helm_schema_core::ValuesPath::parse("auth.password"),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            self_truthy_guards,
            resource,
            vec![ContractProvenance::new(
                "templates/second.yaml",
                SourceSpan::new(30, 40),
                Vec::new(),
            )],
        ),
    ];

    uses = normalize_contract_uses(uses, Vec::new(), &[]);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(
        have: uses[0].single_guard_conjunction(),
        want: base_guards
    );
    sim_assert_eq!(have: uses[0].provenance.len(), want: 2);
}

#[test]
fn self_truthy_posting_index_matches_reference_scan_exhaustively() {
    // Every predicate-set pair is crossed with empty, equal, and distinct
    // provenance and with pathless and concrete-resource render sites.
    for left_mask in 0..32 {
        for right_mask in 0..32 {
            for left_provenance in 0..3 {
                for right_provenance in 0..3 {
                    for with_resource in [false, true] {
                        let uses = vec![
                            exhaustive_contract_use(left_mask, left_provenance, with_resource),
                            exhaustive_contract_use(right_mask, right_provenance, with_resource),
                        ];
                        let mut expected = uses.clone();
                        reference_drop_self_truthy_subsumed_duplicates(&mut expected);
                        let mut actual = uses;
                        drop_self_truthy_subsumed_duplicates(&mut actual);

                        sim_assert_eq!(
                            have: actual,
                            want: expected,
                            "predicate masks {left_mask:#07b}/{right_mask:#07b}, provenance \
                             {left_provenance}/{right_provenance}, resource {with_resource}"
                        );
                    }
                }
            }
        }
    }

    // Mixed render sites prove that no posting list leaks across path, kind,
    // or resource buckets.
    let mut uses = vec![
        exhaustive_contract_use(0, 1, true),
        exhaustive_contract_use(1, 1, true),
        exhaustive_contract_use(0, 1, true),
        exhaustive_contract_use(0, 1, true),
        exhaustive_contract_use(0, 1, false),
    ];
    uses[2].path = YamlPath(vec!["different".to_string()]);
    uses[3].kind = ValueKind::Fragment;
    let mut expected = uses.clone();
    reference_drop_self_truthy_subsumed_duplicates(&mut expected);
    drop_self_truthy_subsumed_duplicates(&mut uses);

    sim_assert_eq!(have: uses, want: expected);
}

#[test]
fn self_truthy_posting_index_preserves_large_bucket_survivor_order() {
    let source = helm_schema_core::ValuesPath::parse("large.value");
    let provenance = vec![ContractProvenance::new(
        "templates/large.yaml",
        SourceSpan::new(10, 20),
        Vec::new(),
    )];
    let resource = Some(ResourceRef::concrete(
        "v1".to_string(),
        "ConfigMap".to_string(),
    ));
    let mut uses = Vec::with_capacity(4_000);
    let mut expected = Vec::with_capacity(2_000);
    for pair in 0..2_000 {
        let selector = Guard::NotEq {
            path: helm_schema_core::ValuesPath::parse("large.selector"),
            value: GuardValue::string(pair.to_string()),
        };
        let base = ContractUse::with_provenances(
            source.clone(),
            YamlPath(vec!["data".to_string(), "value".to_string()]),
            ValueKind::Scalar,
            vec![selector.clone()],
            resource.clone(),
            provenance.clone(),
        );
        let guarded = ContractUse::with_provenances(
            source.clone(),
            YamlPath(vec!["data".to_string(), "value".to_string()]),
            ValueKind::Scalar,
            vec![
                selector,
                Guard::Truthy {
                    path: source.clone(),
                },
            ],
            resource.clone(),
            provenance.clone(),
        );
        uses.push(base);
        uses.push(guarded.clone());
        expected.push(guarded);
    }

    drop_self_truthy_subsumed_duplicates(&mut uses);

    sim_assert_eq!(have: uses, want: expected);
}

fn exhaustive_contract_use(
    predicate_mask: u8,
    provenance_variant: u8,
    with_resource: bool,
) -> ContractUse {
    let source = helm_schema_core::ValuesPath::parse("root.value");
    let guards = [
        Guard::Truthy {
            path: source.clone(),
        },
        Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("root"),
        },
        Guard::Truthy {
            path: helm_schema_core::ValuesPath::parse("root.value.child"),
        },
        Guard::NotEq {
            path: helm_schema_core::ValuesPath::parse("mode"),
            value: GuardValue::string("disabled"),
        },
        Guard::Default {
            path: source.clone(),
        },
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(bit, guard)| (predicate_mask & (1 << bit) != 0).then_some(guard))
    .collect();
    let provenance = match provenance_variant {
        0 => Vec::new(),
        1 => vec![ContractProvenance::new(
            "templates/a.yaml",
            SourceSpan::new(10, 20),
            Vec::new(),
        )],
        _ => vec![ContractProvenance::new(
            "templates/b.yaml",
            SourceSpan::new(30, 40),
            Vec::new(),
        )],
    };
    let resource =
        with_resource.then(|| ResourceRef::concrete("v1".to_string(), "ConfigMap".to_string()));
    ContractUse::with_provenances(
        source,
        YamlPath(Vec::new()),
        ValueKind::Scalar,
        guards,
        resource,
        provenance,
    )
}

fn reference_drop_self_truthy_subsumed_duplicates(uses: &mut Vec<ContractUse>) {
    let empty_predicates = BTreeSet::new();
    let keep = uses
        .iter()
        .map(|contract_use| {
            let source_path = &contract_use.source_expr;
            let predicates = contract_predicates(contract_use).unwrap_or(&empty_predicates);
            let has_self_truthy = predicates.iter().any(
                |predicate| matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == source_path),
            );
            if predicates.iter().any(
                |predicate| matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Default { path }) if path == source_path),
            ) {
                return true;
            }

            !uses.iter().any(|other| {
                let other_predicates = contract_predicates(other).unwrap_or(&empty_predicates);
                render_site(other) == render_site(contract_use)
                    && other_predicates.len() > predicates.len()
                    && !other.provenance.is_empty()
                    && ((contract_use.provenance.is_empty()
                        && contract_use.resource.is_some())
                        || other.provenance == contract_use.provenance)
                    && predicates.is_subset(other_predicates)
                    && ((!has_self_truthy
                        && other_predicates.iter().any(|predicate| {
                            matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) if path == source_path)
                        }))
                        || extra_predicates_are_truthy_parents(predicates, other_predicates))
            })
        })
        .collect::<Vec<_>>();
    let mut keep = keep.into_iter();
    uses.retain(|_| keep.next().unwrap_or(true));
}
