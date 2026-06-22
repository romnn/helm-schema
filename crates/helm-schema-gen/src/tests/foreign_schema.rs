use serde_json::json;
use test_util::prelude::sim_assert_eq;

use crate::schema_node::JsonSchemaType;

use super::{ForeignSchemaObject, ForeignSchemaRestriction, ForeignSchemaTypeField};

#[test]
fn allows_array_from_explicit_type_array() {
    let schema = ForeignSchemaObject::from_value(json!({
        "type": ["array", "object"]
    }))
    .expect("object schema");

    assert!(schema.allows_type(JsonSchemaType::Array));
    assert!(schema.allows_type(JsonSchemaType::Object));
    assert!(!schema.allows_type(JsonSchemaType::String));
}

#[test]
fn allows_array_from_array_keywords_without_type_field() {
    let schema = ForeignSchemaObject::from_value(json!({
        "items": { "type": "string" }
    }))
    .expect("object schema");

    assert!(schema.allows_type(JsonSchemaType::Array));
    assert!(!schema.allows_type(JsonSchemaType::Object));
}

#[test]
fn annotations_only_drops_structural_keywords() {
    let schema = ForeignSchemaObject::from_value(json!({
        "description": "provider leaf",
        "type": "array",
        "items": { "type": "string" }
    }))
    .expect("object schema");

    sim_assert_eq!(
        have: schema.into_annotations_only().into_value(),
        want: json!({
            "description": "provider leaf"
        })
    );
}

#[test]
fn type_field_reports_supported_variants() {
    let schema = ForeignSchemaObject::from_value(json!({
        "type": ["string", "null"]
    }))
    .expect("object schema");

    sim_assert_eq!(
        have: schema.type_field(),
        want: ForeignSchemaTypeField::Multiple(vec![
            JsonSchemaType::String,
            JsonSchemaType::Null
        ])
    );
}

#[test]
fn type_field_rejects_non_string_type_entries() {
    let schema = ForeignSchemaObject::from_value(json!({
        "type": ["string", 7]
    }))
    .expect("object schema");

    sim_assert_eq!(have: schema.type_field(), want: ForeignSchemaTypeField::Unsupported);
}

#[test]
fn scalar_restriction_keeps_only_scalar_union_variants_and_annotations() {
    let schema = json!({
        "description": "provider leaf",
        "anyOf": [
            { "type": "string" },
            { "type": "object", "properties": { "name": { "type": "string" } } }
        ]
    });

    sim_assert_eq!(
        have: ForeignSchemaRestriction::Scalar.apply(schema),
        want: Some(json!({
            "description": "provider leaf",
            "anyOf": [{ "type": "string" }]
        })),
    );
}

#[test]
fn scalar_collection_restriction_requires_array_and_restricts_items() {
    let schema = json!({
        "type": ["array", "object"],
        "properties": { "name": { "type": "string" } },
        "items": {
            "anyOf": [
                { "type": "string" },
                { "type": "object" }
            ]
        }
    });

    sim_assert_eq!(
        have: ForeignSchemaRestriction::ScalarCollection.apply(schema),
        want: Some(json!({
            "type": "array",
            "items": {
                "anyOf": [{ "type": "string" }]
            }
        })),
    );
}
