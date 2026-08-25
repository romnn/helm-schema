use helm_schema_core::{
    ContractRequirementImplication, ContractRequirementTarget, FailValueRequirement, GuardValue,
};
use test_util::prelude::sim_assert_eq;

use crate::requirement_domain::{
    IntegerRangeConstraint, JsonValueKind, RequirementPosition, admitted_json_value_kinds,
    integer_range_constraint,
};

#[test]
fn number_domain_contains_both_numeric_json_kinds() {
    let kinds = admitted_json_value_kinds(
        &[FailValueRequirement::SchemaType("number".to_string())],
        RequirementPosition::Member,
    );

    sim_assert_eq!(
        have: kinds,
        want: std::collections::BTreeSet::from([
            JsonValueKind::Integer,
            JsonValueKind::NonIntegerNumber,
        ])
    );
}

#[test]
fn integer_range_constraint_uses_generated_member_values() {
    for (requirement, want) in [
        (
            FailValueRequirement::SchemaType("number".to_string()),
            IntegerRangeConstraint::Any,
        ),
        (
            FailValueRequirement::TruthyImpliesSchemaType("string".to_string()),
            IntegerRangeConstraint::Maximum(1),
        ),
        (
            FailValueRequirement::HelmTruthy,
            IntegerRangeConstraint::Maximum(0),
        ),
        (
            FailValueRequirement::HelmFalsy,
            IntegerRangeConstraint::Maximum(1),
        ),
        (
            FailValueRequirement::NotEquals(GuardValue::Int(2)),
            IntegerRangeConstraint::Maximum(2),
        ),
        (
            FailValueRequirement::NotEquals(GuardValue::Int(-1)),
            IntegerRangeConstraint::Any,
        ),
    ] {
        sim_assert_eq!(
            have: integer_range_constraint(&[requirement]),
            want: Some(want)
        );
    }
}

#[test]
fn member_number_requirement_emits_an_unbounded_integer_count_lane() {
    let implication = ContractRequirementImplication {
        outer_guards: Vec::new(),
        target: ContractRequirementTarget::Members {
            allow_integer: true,
        },
        requirements: vec![FailValueRequirement::SchemaType("number".to_string())],
    };
    let (schema, abstentions) =
        crate::path_resolver::fail_requirement_schema(std::iter::once(&implication));

    sim_assert_eq!(have: abstentions, want: 0);
    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "anyOf": [
                { "type": "array", "items": { "type": "number" } },
                { "type": "object", "additionalProperties": { "type": "number" } },
                { "type": "integer" },
                { "type": "null" },
            ]
        })
    );
}

#[test]
fn key_requirement_uses_the_same_value_sensitive_member_sequence() {
    let implication = ContractRequirementImplication {
        outer_guards: Vec::new(),
        target: ContractRequirementTarget::Keys,
        requirements: vec![FailValueRequirement::HelmFalsy],
    };
    let (schema, abstentions) =
        crate::path_resolver::fail_requirement_schema(std::iter::once(&implication));

    sim_assert_eq!(have: abstentions, want: 0);
    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "anyOf": [
                { "type": "object" },
                { "type": "array", "maxItems": 1 },
                { "type": "null" },
            ]
        })
    );
}
