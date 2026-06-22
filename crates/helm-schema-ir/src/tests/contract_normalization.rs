use crate::{ContractProvenance, ContractUse, SourceSpan, ValueKind, YamlPath};
use test_util::prelude::sim_assert_eq;

use super::canonicalize_contract_uses;

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
