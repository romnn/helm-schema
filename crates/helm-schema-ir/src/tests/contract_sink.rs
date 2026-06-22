use super::*;
use test_util::prelude::sim_assert_eq;

#[test]
fn contract_use_context_attaches_chart_default_only_to_rendered_paths() {
    let guards = Vec::new();
    let chart_value_defaults = BTreeSet::from(["serviceAccount.name".to_string()]);
    let context = ContractUseContext::new(
        &guards,
        &chart_value_defaults,
        false,
        None,
        None,
        Vec::new(),
    );

    let rendered = context.contract_use(
        "serviceAccount.name".to_string(),
        YamlPath(vec!["metadata".to_string(), "name".to_string()]),
        ValueKind::Scalar,
        &[],
        None,
    );
    sim_assert_eq!(
        have: rendered.guards,
        want: vec![Guard::Default {
            path: "serviceAccount.name".to_string(),
        }]
    );

    let pathless =
        context.pathless_contract_use("serviceAccount.name".to_string(), ValueKind::Scalar, &[]);
    assert!(pathless.guards.is_empty());
}

#[test]
fn contract_use_context_lowers_pathless_partial_scalar_to_scalar() {
    let guards = Vec::new();
    let chart_value_defaults = BTreeSet::new();
    let context = ContractUseContext::new(
        &guards,
        &chart_value_defaults,
        false,
        None,
        None,
        Vec::new(),
    );

    let contract_use =
        context.pathless_contract_use("image.tag".to_string(), ValueKind::PartialScalar, &[]);

    sim_assert_eq!(have: contract_use.kind, want: ValueKind::Scalar);
}
