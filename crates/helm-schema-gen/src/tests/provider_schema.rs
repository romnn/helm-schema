use helm_schema_k8s::ProviderSchemaSource;
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

#[test]
fn candidate_preserves_provider_source_leaf_schema() {
    let source_schema = json!({ "$ref": "#/definitions/StringMap" });
    let fragment = ProviderSchemaFragment::new(json!({
        "type": "object",
        "additionalProperties": { "type": "string" }
    }))
    .with_source_definition_schema(
        ProviderSchemaSource::kubernetes_openapi(
            "default",
            "v1.35.0",
            "source.json",
            "/definitions/Container/properties/env",
        ),
        source_schema.clone(),
        source_schema.clone(),
    );

    let candidate = ProviderSchemaCandidate::from_provider_fragment(fragment);

    sim_assert_eq!(
        have: candidate.source().map(ProviderSchemaSource::pointer),
        want: Some("/definitions/Container/properties/env")
    );
    sim_assert_eq!(
        have: candidate.source_definition_schema(),
        want: None,
        "source leaf refs to provider-document siblings are not self-contained at output root"
    );
}

#[test]
fn candidate_exposes_provider_source_leaf_with_only_internal_refs() {
    let source_schema = json!({
        "type": "object",
        "properties": {
            "labels": { "$ref": "#/$defs/StringMap" }
        },
        "$defs": {
            "StringMap": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });
    let fragment = ProviderSchemaFragment::new(json!({
        "type": "object",
        "properties": {
            "labels": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    }))
    .with_source_definition_schema(
        ProviderSchemaSource::kubernetes_openapi(
            "default",
            "v1.35.0",
            "source.json",
            "/definitions/Metadata",
        ),
        source_schema.clone(),
        source_schema.clone(),
    );

    let candidate = ProviderSchemaCandidate::from_provider_fragment(fragment);

    sim_assert_eq!(have: candidate.source_definition_schema(), want: Some(&source_schema));
}

#[test]
fn rewrites_internal_source_refs_for_root_definition_location() {
    let source_schema = json!({
        "type": "object",
        "properties": {
            "labels": { "$ref": "#/$defs/StringMap" }
        },
        "$defs": {
            "StringMap": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });

    let rewritten =
        rewrite_internal_refs_for_root_definition(&source_schema, "provider/source~name")
            .expect("internal refs can be relocated under a root definition");

    sim_assert_eq!(
        have: rewritten.pointer("/properties/labels/$ref"),
        want: Some(&Value::String(
            "#/$defs/provider~1source~0name/$defs/StringMap".to_string()
        ))
    );
}

#[test]
fn source_ref_rewrite_ignores_ref_shaped_enum_data() {
    let source_schema = json!({
        "type": "object",
        "enum": [
            { "$ref": "#/not/a/schema/ref" }
        ],
        "properties": {
            "name": { "type": "string" }
        }
    });

    let rewritten = rewrite_internal_refs_for_root_definition(&source_schema, "providerSource")
        .expect("ref-shaped enum data is not schema structure");

    sim_assert_eq!(
        have: rewritten.pointer("/enum/0/$ref"),
        want: Some(&Value::String("#/not/a/schema/ref".to_string()))
    );
}

#[test]
fn source_ref_rewrite_treats_property_names_as_schema_map_keys() {
    let source_schema = json!({
        "type": "object",
        "$defs": {
            "StringValue": { "type": "string" }
        },
        "properties": {
            "enum": { "$ref": "#/$defs/StringValue" }
        }
    });

    let rewritten = rewrite_internal_refs_for_root_definition(&source_schema, "providerSource")
        .expect("property schemas are traversed independent of property name");

    sim_assert_eq!(
        have: rewritten.pointer("/properties/enum/$ref"),
        want: Some(&Value::String(
            "#/$defs/providerSource/$defs/StringValue".to_string()
        ))
    );
}
