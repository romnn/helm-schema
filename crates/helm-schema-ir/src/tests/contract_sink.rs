use super::*;
use test_util::prelude::sim_assert_eq;

fn emitted_uses(context: &ContractUseContext<'_>, witness: EmissionWitness) -> Vec<ContractUse> {
    let mut contract = ContractIr::default();
    context.emit(witness, &mut contract);
    contract.finalize().uses().to_vec()
}

fn emitted_guards(context: &ContractUseContext<'_>, witness: EmissionWitness) -> Vec<Vec<Guard>> {
    emitted_uses(context, witness)
        .into_iter()
        .map(|contract_use| contract_use.guards)
        .collect()
}

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
        None,
        Vec::new(),
    );

    let rendered = emitted_guards(
        &context,
        EmissionWitness::new(
            "serviceAccount.name".to_string(),
            Some(YamlPath(vec!["metadata".to_string(), "name".to_string()])),
            ValueKind::Scalar,
            vec![Vec::new()],
        ),
    );
    sim_assert_eq!(
        have: rendered,
        want: vec![vec![Guard::Default {
            path: "serviceAccount.name".to_string(),
        }]]
    );

    let pathless = emitted_guards(
        &context,
        EmissionWitness::new(
            "serviceAccount.name".to_string(),
            None,
            ValueKind::Scalar,
            vec![Vec::new()],
        ),
    );
    sim_assert_eq!(have: pathless, want: vec![Vec::<Guard>::new()]);
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
        None,
        Vec::new(),
    );

    let kinds = emitted_uses(
        &context,
        EmissionWitness::new(
            "image.tag".to_string(),
            None,
            ValueKind::PartialScalar,
            vec![Vec::new()],
        ),
    )
    .into_iter()
    .map(|contract_use| contract_use.kind)
    .collect::<Vec<_>>();

    sim_assert_eq!(have: kinds, want: vec![ValueKind::Scalar]);
}
