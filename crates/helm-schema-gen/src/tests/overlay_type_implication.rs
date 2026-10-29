use helm_schema_core::{ConditionalGuard, ValuesPath};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

use super::{schema_contains_type, type_guard_implies_schema};

#[test]
fn complete_type_arms_do_not_confuse_constraints_or_exclusivity_with_containment() {
    for (schema, schema_type, expected) in [
        (json!({}), "object", true),
        (json!(true), "string", true),
        (json!(false), "string", false),
        (json!({"type": "number"}), "integer", true),
        (json!({"type": "integer"}), "number", false),
        (json!({"type": ["string", "boolean"]}), "string", true),
        (json!({"type": "string", "minLength": 1}), "string", false),
        (
            json!({"type": "object", "additionalProperties": false}),
            "object",
            false,
        ),
        (
            json!({"anyOf": [{"type": "string"}, {"const": 1}]}),
            "string",
            true,
        ),
        (
            json!({"oneOf": [{"type": "string"}, {"const": "x"}]}),
            "string",
            false,
        ),
        (
            json!({"allOf": [{"type": "string"}, {"pattern": "x"}]}),
            "string",
            false,
        ),
        (json!({"allOf": [{"type": "string"}, {}]}), "string", true),
        (
            json!({"anyOf": [{"type": "string"}], "not": {"const": "x"}}),
            "string",
            false,
        ),
    ] {
        sim_assert_eq!(
            have: schema_contains_type(&schema, schema_type),
            want: expected,
            "schema={schema}, type={schema_type}"
        );
    }
}

#[test]
fn guard_proof_requires_every_disjunct_and_the_correct_target() {
    let target = ValuesPath::parse("value");
    let string = ConditionalGuard::TypeIs {
        path: target.clone(),
        schema_type: "string".to_owned(),
    };
    let number = ConditionalGuard::TypeIs {
        path: target.clone(),
        schema_type: "number".to_owned(),
    };
    let unrelated = ConditionalGuard::TypeIs {
        path: ValuesPath::parse("other"),
        schema_type: "string".to_owned(),
    };
    let schema: Value = json!({"type": "string"});
    for (guard, expected) in [
        (string.clone(), true),
        (unrelated.clone(), false),
        (
            ConditionalGuard::AllOf(vec![unrelated.clone(), string.clone()].into()),
            true,
        ),
        (
            ConditionalGuard::AnyOf(vec![unrelated, string.clone()].into()),
            false,
        ),
        (
            ConditionalGuard::AnyOf(vec![number, string.clone()].into()),
            false,
        ),
        (ConditionalGuard::Not(std::sync::Arc::new(string)), false),
    ] {
        sim_assert_eq!(
            have: type_guard_implies_schema(&guard, &target, &schema),
            want: expected,
            "guard={guard:?}"
        );
    }
}

#[path = "overlay_type_permission_fallback.rs"]
mod type_permission_fallback;
