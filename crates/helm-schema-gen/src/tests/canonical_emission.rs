use color_eyre::eyre;
use indoc::indoc;
use serde_json::{Value, json};
use serde_yaml::Value as YamlValue;
use test_util::prelude::sim_assert_eq;

use crate::schema_node::SchemaNode;
use crate::schema_tree::{
    CanonicalConstraintApplication, CanonicalConstraintOutcome, SchemaDocument,
};

#[test]
fn canonical_property_slot_rewrites_are_exhaustively_equivalent() -> eyre::Result<()> {
    let constraints = [
        json!({ "type": "object" }),
        json!({ "not": { "type": "null" } }),
    ];
    let base_slots = [
        json!({ "type": ["object", "null"] }),
        json!({ "type": ["string", "null"] }),
        json!({ "anyOf": [{ "type": "object" }, { "type": "string" }] }),
    ];
    let values = [
        json!(null),
        json!(false),
        json!(0),
        json!(1.5),
        json!(""),
        json!("v1"),
        json!([]),
        json!({}),
        json!({ "member": "value" }),
    ];

    for base_slot in base_slots {
        for constraint in &constraints {
            let (legacy, canonical, outcome) = rewrite_pair(
                &["value"],
                SchemaNode::foreign(base_slot.clone()),
                constraint,
            );
            assert!(
                !matches!(outcome, CanonicalConstraintOutcome::NotApplicable),
                "existing direct slots must use the canonical path"
            );
            assert_equivalent(
                &legacy,
                &canonical,
                std::iter::once(json!({}))
                    .chain(values.iter().map(|value| json!({ "value": value.clone() }))),
            )?;
        }
    }
    Ok(())
}

#[test]
fn canonical_presence_rewrites_required_and_not_null_shapes_equivalently() -> eyre::Result<()> {
    let mut base = SchemaDocument::new_root_object();
    base.insert_path_schema(&["value".to_string()], SchemaNode::unknown_object());
    base.insert_path_schema(
        &["value".to_string(), "member".to_string()],
        SchemaNode::foreign(json!({ "type": ["string", "null"] })),
    );
    let mut canonical = base.clone();
    let required = json!({ "type": "object", "required": ["value"] });
    let not_null = json!({ "not": { "type": "null" } });
    sim_assert_eq!(
        have: canonical.canonicalize_constraint_at_path(&[], &required),
        want: CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
    );
    sim_assert_eq!(
        have: canonical.canonicalize_constraint_at_path(
            &["value".to_string(), "member".to_string()],
            &not_null,
        ),
        want: CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
    );

    let mut legacy = base.into_value();
    if let Some(object) = legacy.as_object_mut() {
        object.insert(
            "allOf".to_string(),
            json!([
                required,
                { "properties": { "value": { "properties": { "member": not_null } } } },
            ]),
        );
    }
    let canonical = canonical.into_value();
    let values = [json!(null), json!(false), json!(0), json!(""), json!("set")];
    let probes = std::iter::once(json!({})).chain(values.iter().map(|member| {
        json!({
            "value": { "member": member.clone() },
        })
    }));
    assert_equivalent(&legacy, &canonical, probes)
}

#[test]
fn canonicalization_falls_back_without_mutating_a_missing_closed_root_slot() {
    let mut schema = SchemaDocument::new_root_object();
    schema.insert_path_schema(&["known".to_string()], SchemaNode::type_named("string"));
    let before = schema.clone().into_value();

    let outcome = schema.canonicalize_constraint_at_path(
        &["missing".to_string()],
        &json!({ "not": { "type": "null" } }),
    );

    sim_assert_eq!(have: outcome, want: CanonicalConstraintOutcome::NotApplicable);
    sim_assert_eq!(have: schema.into_value(), want: before);
}

#[test]
fn canonicalization_proves_redundant_not_null_constraints() {
    let mut schema = SchemaDocument::new_root_object();
    schema.insert_path_schema(&["value".to_string()], SchemaNode::type_named("string"));
    let before = schema.clone().into_value();

    let outcome = schema.canonicalize_constraint_at_path(
        &["value".to_string()],
        &json!({ "not": { "type": "null" } }),
    );

    sim_assert_eq!(
        have: outcome,
        want: CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Redundant)
    );
    sim_assert_eq!(have: schema.into_value(), want: before);
}

#[test]
fn canonicalization_conjoins_an_existing_empty_slot() -> eyre::Result<()> {
    let constraint = json!({ "not": { "type": "null" } });
    let (legacy, canonical, outcome) = rewrite_pair(&["value"], SchemaNode::empty(), &constraint);

    sim_assert_eq!(
        have: outcome,
        want: CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
    );
    sim_assert_eq!(
        have: canonical.pointer("/properties/value"),
        want: Some(&json!({ "allOf": [{}, constraint] }))
    );
    assert_equivalent(
        &legacy,
        &canonical,
        [
            json!({}),
            json!({ "value": null }),
            json!({ "value": "set" }),
        ],
    )
}

#[test]
fn canonicalization_keeps_foreign_type_unions_intact() {
    let provider_payload = json!({
        "description": "provider-owned",
        "type": ["null", "object"],
    });
    let (_, canonical, outcome) = rewrite_pair(
        &["value"],
        SchemaNode::foreign(provider_payload.clone()),
        &json!({ "type": "object" }),
    );

    sim_assert_eq!(
        have: outcome,
        want: CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
    );
    sim_assert_eq!(
        have: canonical.pointer("/properties/value/allOf/0"),
        want: Some(&provider_payload)
    );
}

#[test]
fn canonical_object_conjunction_survives_missing_default_backfill() -> eyre::Result<()> {
    let provider_payload = json!({
        "description": "provider-owned",
        "type": ["null", "object"],
    });
    let mut schema = SchemaDocument::new_root_object();
    schema.insert_path_schema(
        &["value".to_string()],
        SchemaNode::foreign(provider_payload.clone()),
    );
    sim_assert_eq!(
        have: schema.canonicalize_constraint_at_path(
            &["value".to_string()],
            &json!({ "type": "object" }),
        ),
        want: CanonicalConstraintOutcome::Applied(CanonicalConstraintApplication::Emitted)
    );
    let defaults: YamlValue = serde_yaml::from_str(indoc! {"
        value:
          member: default
    "})?;
    schema.merge_missing_values_yaml_defaults_under_roots(
        &defaults,
        &[Vec::new()],
        &std::collections::BTreeSet::new(),
    );
    let schema = schema.into_value();

    sim_assert_eq!(
        have: schema.pointer("/properties/value/allOf/0"),
        want: Some(&provider_payload)
    );
    sim_assert_eq!(
        have: schema.pointer("/properties/value/properties/member/type"),
        want: Some(&json!("string"))
    );
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| eyre::eyre!("compile canonical schema: {error}"))?;
    assert!(!validator.is_valid(&json!({ "value": "v1" })));
    assert!(validator.is_valid(&json!({ "value": { "member": "configured" } })));
    Ok(())
}

#[test]
fn generator_type_union_constructor_is_exhaustively_equivalent() -> eyre::Result<()> {
    let domains = [
        vec!["null", "string"],
        vec!["array", "null", "object"],
        vec!["boolean", "integer", "null", "number", "string"],
        vec!["array", "integer", "null", "object"],
    ];
    let probes = [
        json!(null),
        json!(false),
        json!(0),
        json!(1.5),
        json!(""),
        json!("value"),
        json!([]),
        json!([{}]),
        json!({}),
        json!({ "member": true }),
    ];

    for domain in domains {
        let legacy = json!({
            "anyOf": domain
                .iter()
                .map(|schema_type| json!({ "type": schema_type }))
                .collect::<Vec<_>>(),
        });
        let canonical = crate::schema_model::type_union_schema(&domain);
        assert_equivalent(&legacy, &canonical, probes.iter().cloned())?;
    }
    sim_assert_eq!(
        have: crate::schema_model::type_union_schema(["string", "null", "string"]),
        want: json!({ "type": ["null", "string"] })
    );
    Ok(())
}

fn rewrite_pair(
    path: &[&str],
    base_slot: SchemaNode,
    constraint: &Value,
) -> (Value, Value, CanonicalConstraintOutcome) {
    let path = path
        .iter()
        .map(|segment| (*segment).to_string())
        .collect::<Vec<_>>();
    let mut base = SchemaDocument::new_root_object();
    base.insert_path_schema(&path, base_slot);
    let mut canonical = base.clone();
    let outcome = canonical.canonicalize_constraint_at_path(&path, constraint);
    let mut legacy = base.into_value();
    let carrier = path.iter().rev().fold(
        constraint.clone(),
        |child, segment| json!({ "properties": { segment: child } }),
    );
    if let Some(object) = legacy.as_object_mut() {
        object.insert("allOf".to_string(), json!([carrier]));
    }
    (legacy, canonical.into_value(), outcome)
}

fn assert_equivalent(
    legacy: &Value,
    canonical: &Value,
    probes: impl IntoIterator<Item = Value>,
) -> eyre::Result<()> {
    let legacy_validator = jsonschema::validator_for(legacy)
        .map_err(|error| eyre::eyre!("compile legacy carrier: {error}"))?;
    let canonical_validator = jsonschema::validator_for(canonical)
        .map_err(|error| eyre::eyre!("compile canonical carrier: {error}"))?;
    for probe in probes {
        sim_assert_eq!(
            have: canonical_validator.is_valid(&probe),
            want: legacy_validator.is_valid(&probe),
            "canonical rewrite changed acceptance for {probe}"
        );
    }
    Ok(())
}
