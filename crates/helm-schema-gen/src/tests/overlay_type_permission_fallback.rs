use helm_schema_core::{
    ConditionalGuard, ContractPathSchemaEvidence, ContractSchemaSignals, ProviderSchemaFragment,
    ProviderSchemaUse, ResourceSchemaOracle, ValuesPath,
};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::super::{ConditionalBaseEffect, append_type_permission_fallback};
use crate::DeclaredTypes;
use crate::path_resolver::PathSchemaResolver;
use crate::provider_resolution::ProviderSchemaResolutions;

#[derive(Debug)]
struct NoProvider;

impl ResourceSchemaOracle for NoProvider {
    fn schema_fragment_for_use(&self, _use_: &ProviderSchemaUse) -> Option<ProviderSchemaFragment> {
        None
    }
}

#[test]
fn permission_fallback_uses_declaration_instead_of_a_runtime_type_hint() {
    let target = ValuesPath::parse("subject");
    let evidence = ContractPathSchemaEvidence {
        type_hints: ["boolean".to_owned()].into(),
        ..ContractPathSchemaEvidence::default()
    };
    let provider =
        ProviderSchemaResolutions::resolve(&ContractSchemaSignals::default(), &NoProvider);
    let (mut resolved, _) =
        PathSchemaResolver::resolve_single_path_evidence(&target, &evidence, &provider);
    resolved.schema = json!({"type": "boolean"});
    let permission = vec![vec![ConditionalGuard::TypeIs {
        path: target.clone(),
        schema_type: "boolean".to_owned(),
    }]];
    for (declaration, policy, serialized, want) in [
        (json!({}), DeclaredTypes::Assert, false, json!({})),
        (
            json!({"type": "number"}),
            DeclaredTypes::Assert,
            false,
            json!({"type": "number"}),
        ),
        (
            json!({"type": "number"}),
            DeclaredTypes::Annotate,
            false,
            json!({}),
        ),
        (
            json!({"type": "string"}),
            DeclaredTypes::Assert,
            true,
            json!({}),
        ),
    ] {
        resolved.used_as_serialized = serialized;
        resolved.values_yaml_schema = declaration;
        let mut conditionals = Vec::new();
        append_type_permission_fallback(
            &mut conditionals,
            &target,
            &resolved,
            permission.clone(),
            policy,
        );
        sim_assert_eq!(have: conditionals.len(), want: 1);
        for conditional in conditionals {
            sim_assert_eq!(have: conditional.schema.into_value(), want: want);
            assert!(matches!(
                conditional.carrier.base_effect,
                ConditionalBaseEffect::Own
            ));
        }
    }
}

#[test]
fn permission_fallback_preserves_an_independent_strict_consumer() {
    let target = ValuesPath::parse("subject");
    let mut evidence = ContractPathSchemaEvidence::default();
    evidence.facts.has_string_contract = true;
    evidence.facts.has_non_self_guarded_string_contract = true;
    let provider =
        ProviderSchemaResolutions::resolve(&ContractSchemaSignals::default(), &NoProvider);
    let (resolved, _) =
        PathSchemaResolver::resolve_single_path_evidence(&target, &evidence, &provider);
    assert!(resolved.independent_base_contract.is_some());
    let permission = vec![vec![ConditionalGuard::TypeIs {
        path: target.clone(),
        schema_type: "boolean".to_owned(),
    }]];
    let mut conditionals = Vec::new();
    append_type_permission_fallback(
        &mut conditionals,
        &target,
        &resolved,
        permission,
        DeclaredTypes::Assert,
    );
    sim_assert_eq!(have: conditionals.iter().map(|conditional| conditional.schema.clone().into_value()).collect::<Vec<_>>(), want: vec![json!({}), json!({"type": "string"})]);
}
