use crate::{
    ContractProvenance, ContractUse, Guard, GuardValue, ResourceRef, SourceSpan, ValueKind,
    YamlPath,
};
use test_util::prelude::sim_assert_eq;

use super::{canonicalize_contract_uses, normalize_contract_uses};

#[test]
fn canonicalization_merges_provenance_for_semantically_identical_uses() {
    let mut uses = vec![
        ContractUse {
            source_expr: "image.tag".to_string(),
            path: YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            kind: ValueKind::Scalar,
            guards: Vec::new(),
            resource: None,
            provenance: vec![ContractProvenance::new(
                "templates/a.yaml",
                SourceSpan::new(10, 20),
                Vec::new(),
            )],
        },
        ContractUse {
            source_expr: "image.tag".to_string(),
            path: YamlPath(vec!["spec".to_string(), "tag".to_string()]),
            kind: ValueKind::Scalar,
            guards: Vec::new(),
            resource: None,
            provenance: vec![ContractProvenance::new(
                "templates/b.yaml",
                SourceSpan::new(30, 40),
                vec!["helper.render".to_string()],
            )],
        },
    ];

    canonicalize_contract_uses(&mut uses);

    sim_assert_eq!(have: uses.len(), want: 1);
    sim_assert_eq!(have: uses[0].provenance.len(), want: 2);
    assert!(
        uses[0]
            .provenance
            .iter()
            .any(|provenance| provenance.template_path == "templates/a.yaml")
    );
    assert!(
        uses[0]
            .provenance
            .iter()
            .any(|provenance| provenance.template_path == "templates/b.yaml"
                && provenance.helper_chain == vec!["helper.render".to_string()])
    );
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
            .guards
            .iter()
            .any(|guard| { matches!(guard, Guard::Truthy { path } if path == "auth.password") })
    );
}
