use std::collections::BTreeMap;

use color_eyre::eyre;
use helm_schema_core::{
    ConditionalGuard, ContractPathSchemaEvidence, ContractRequirementImplication,
    ContractRequirementTarget, ContractSchemaSignals, FailValueRequirement, GuardValue,
    RequirementConditionBuilder, ValuesPath,
};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::NoopProvider;
use crate::{
    AuthoringPolicy, DeclaredTypes, PreparedValuesDocuments, RootPolicy, SchemaProfile,
    ValuesSchemaInput, generate_values_schema,
};

fn signals(guards: Vec<ConditionalGuard>) -> ContractSchemaSignals {
    ContractSchemaSignals::new(
        BTreeMap::from([
            (
                ValuesPath::parse("target"),
                ContractPathSchemaEvidence {
                    is_referenced_value_path: true,
                    requirement_implications: vec![ContractRequirementImplication::new(
                        guards,
                        ContractRequirementTarget::Value,
                        vec![FailValueRequirement::SchemaTypeEvenNull(
                            "string".to_owned(),
                        )],
                    )],
                    ..ContractPathSchemaEvidence::default()
                },
            ),
            (
                ValuesPath::parse("enabled"),
                ContractPathSchemaEvidence::default(),
            ),
        ]),
        Vec::new(),
    )
}

/// Lean retains the unconditional proof without retaining a separately guarded proof.
#[test]
fn lean_selects_unconditional_requirement_regions_before_lint() -> eyre::Result<()> {
    let mut builder = RequirementConditionBuilder::default();
    let guarded = builder.guard(ConditionalGuard::Eq {
        path: ValuesPath::parse("enabled"),
        value: GuardValue::Bool(true),
    });
    let root = builder.union([RequirementConditionBuilder::TRUE, guarded]);
    let bundled = signals(vec![ConditionalGuard::Requirement(
        builder.freeze().root(root, true),
    )]);
    // The ordinary mandatory path is the explicit unconditional string-consumer obligation.
    let unconditional = signals(Vec::new());
    let authoring = AuthoringPolicy {
        root: RootPolicy::Open,
        declared_types: DeclaredTypes::Annotate,
    };
    for lint in [false, true] {
        let documents = PreparedValuesDocuments::new(
            serde_yaml::to_value(json!({"target":"default","enabled":false}))?,
            serde_yaml::Value::Null,
        );
        let documents = if lint {
            documents
                .with_uncoalesced_root(serde_yaml::to_value(json!({"target":7,"enabled":false}))?)
        } else {
            documents
        };
        let actual = generate_values_schema(
            ValuesSchemaInput::new(&bundled, &NoopProvider)
                .with_values_documents(&documents)
                .with_authoring_policy(authoring)
                .with_profile(SchemaProfile::Lean),
        );
        let expected = generate_values_schema(
            ValuesSchemaInput::new(&unconditional, &NoopProvider)
                .with_values_documents(&documents)
                .with_authoring_policy(authoring)
                .with_profile(SchemaProfile::Lean),
        );
        sim_assert_eq!(have: &actual, want: &expected);
        let validator = jsonschema::validator_for(&actual)?;
        for enabled in [false, true] {
            sim_assert_eq!(have: validator.is_valid(&json!({"target":7,"enabled":enabled})), want: lint);
            sim_assert_eq!(have: validator.is_valid(&json!({"target":"ok","enabled":enabled})), want: true);
        }
    }
    Ok(())
}
