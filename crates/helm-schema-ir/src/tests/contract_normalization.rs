use crate::{
    ContractProvenance, ContractUse, Guard, GuardValue, ResourceRef, SourceSpan, ValueKind,
    YamlPath,
};
use test_util::prelude::sim_assert_eq;

use super::{canonicalize_contract_uses, expand_condition_disjuncts, normalize_contract_uses};

#[test]
fn disjunct_expansion_deduplicates_identical_rows_before_subsumption() {
    let use_ = ContractUse::new(
        "feature.enabled".to_string(),
        YamlPath(vec!["spec".to_string(), "enabled".to_string()]),
        ValueKind::Scalar,
        vec![Guard::Truthy {
            path: "feature.enabled".to_string(),
        }],
        None,
    );
    let mut uses = vec![use_.clone(), use_];

    expand_condition_disjuncts(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
}

#[test]
fn canonicalization_merges_provenance_for_semantically_identical_uses() {
    let mut uses = vec![
        ContractUse {
            source_expr: "image.tag".to_string(),
            path: YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: None,
            provenance: vec![ContractProvenance::new(
                "templates/a.yaml",
                SourceSpan::new(10, 20),
                Vec::new(),
            )],
            has_string_contract: false,
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
        },
        ContractUse {
            source_expr: "image.tag".to_string(),
            path: YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            kind: ValueKind::Scalar,
            condition: helm_schema_core::GuardDnf::from_guards(Vec::new()),
            resource: None,
            provenance: vec![ContractProvenance::new(
                "templates/b.yaml",
                SourceSpan::new(30, 40),
                vec!["helper.render".to_string()],
            )],
            has_string_contract: false,
            template_supplied_member_keys: Default::default(),
            split_segment: None,
            merge_layers: None,
        },
    ];

    canonicalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(have: uses[0].provenance.len(), want: 2);
}

#[test]
fn canonicalization_merges_complementary_conditions_across_render_sites() {
    let mut uses = vec![
        ContractUse::with_provenances(
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }],
            None,
            vec![ContractProvenance::new(
                "templates/a.yaml",
                SourceSpan::new(10, 20),
                Vec::new(),
            )],
        ),
        ContractUse::with_provenances(
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: "feature.enabled".to_string(),
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
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Truthy {
                path: "feature.enabled".to_string(),
            }],
            None,
            vec![provenance.clone()],
        ),
        ContractUse::with_provenances(
            "image.tag".to_string(),
            YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            ValueKind::Scalar,
            vec![Guard::Not {
                path: "feature.enabled".to_string(),
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
        path: "auth.username".to_string(),
        value: GuardValue::string("postgres"),
    }];
    let mut self_truthy_guards = base_guards.clone();
    self_truthy_guards.insert(
        0,
        Guard::Truthy {
            path: "auth.password".to_string(),
        },
    );
    let mut uses = vec![
        ContractUse::with_provenances(
            "auth.password".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            base_guards,
            resource.clone(),
            vec![provenance.clone()],
        ),
        ContractUse::with_provenances(
            "auth.password".to_string(),
            YamlPath(Vec::new()),
            ValueKind::Scalar,
            self_truthy_guards,
            resource,
            vec![provenance],
        ),
    ];

    normalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    assert!(
        uses[0]
            .single_guard_conjunction()
            .iter()
            .any(|guard| { matches!(guard, Guard::Truthy { path } if path == "auth.password") })
    );
}

#[test]
fn normalization_drops_subsumed_truthy_branch_across_provenance_sites() {
    let resource = Some(ResourceRef::concrete(
        "v1".to_string(),
        "Secret".to_string(),
    ));
    let base_guards = vec![Guard::NotEq {
        path: "auth.username".to_string(),
        value: GuardValue::string("postgres"),
    }];
    let mut self_truthy_guards = base_guards.clone();
    self_truthy_guards.push(Guard::Truthy {
        path: "auth.password".to_string(),
    });
    let mut uses = vec![
        ContractUse::with_provenances(
            "auth.password".to_string(),
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
            "auth.password".to_string(),
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

    normalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(
        have: uses[0].single_guard_conjunction(),
        want: base_guards
    );
    sim_assert_eq!(have: uses[0].provenance.len(), want: 2);
}
