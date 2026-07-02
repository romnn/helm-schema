use helm_schema_k8s::{ProviderOrigin, ProviderSchemaFragment, ProviderSchemaSource};
use serde_json::json;
use test_util::prelude::sim_assert_eq;

use super::*;

fn resolved_path(path: &str, schema: Value) -> ResolvedPathSchema {
    ResolvedPathSchema {
        value_path: path.to_string(),
        path_segments: path
            .split('.')
            .map(std::string::ToString::to_string)
            .collect(),
        provider_schema_candidate: Some(ProviderSchemaCandidate::from_provider_fragment(
            ProviderSchemaFragment::new(schema.clone()),
        )),
        values_yaml_schema: crate::schema_model::empty_schema(),
        schema,
        used_as_pathless_fragment: false,
        accepted_dependency_values_root_fragment: false,
    }
}

fn k8s_source(pointer: &str) -> ProviderSchemaSource {
    ProviderSchemaSource::kubernetes_openapi(
        "default",
        "v1.35.0",
        "io.k8s.api.core.v1.Pod.json",
        pointer,
    )
}

fn sourced_provider_schema_candidate(schema: Value, pointer: &str) -> ProviderSchemaCandidate {
    ProviderSchemaCandidate::from_provider_fragment(
        ProviderSchemaFragment::new(schema).with_source(k8s_source(pointer)),
    )
}

fn sourced_provider_schema_candidate_with_source_schema(
    schema: Value,
    pointer: &str,
    source_schema: Value,
) -> ProviderSchemaCandidate {
    ProviderSchemaCandidate::from_provider_fragment(
        ProviderSchemaFragment::new(schema).with_source_schema(k8s_source(pointer), source_schema),
    )
}

fn sourced_provider_schema_candidate_with_definition_schema(
    schema: Value,
    pointer: &str,
    source_schema: Value,
    definition_schema: Value,
) -> ProviderSchemaCandidate {
    ProviderSchemaCandidate::from_provider_fragment(
        ProviderSchemaFragment::new(schema).with_source_definition_schema(
            k8s_source(pointer),
            source_schema,
            definition_schema,
        ),
    )
}

fn resolved_sourced_path(path: &str, schema: Value, pointer: &str) -> ResolvedPathSchema {
    ResolvedPathSchema {
        value_path: path.to_string(),
        path_segments: path
            .split('.')
            .map(std::string::ToString::to_string)
            .collect(),
        provider_schema_candidate: Some(sourced_provider_schema_candidate(schema.clone(), pointer)),
        values_yaml_schema: crate::schema_model::empty_schema(),
        schema,
        used_as_pathless_fragment: false,
        accepted_dependency_values_root_fragment: false,
    }
}

#[test]
fn repeated_provider_subtrees_move_to_root_definitions() {
    let provider_schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "additionalProperties": false
    });
    let mut paths = vec![
        resolved_path("first", provider_schema.clone()),
        resolved_path("second", provider_schema.clone()),
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(
        have: paths[0].schema,
        want: json!({ "$ref": "#/$defs/providerSchema1" })
    );
    sim_assert_eq!(
        have: paths[1].schema,
        want: json!({ "$ref": "#/$defs/providerSchema1" })
    );
    sim_assert_eq!(
        have: root.pointer("/$defs/providerSchema1"),
        want: Some(&provider_schema)
    );
}

#[test]
fn provider_fragment_source_survives_candidate_lowering() {
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        }
    });
    let candidate = sourced_provider_schema_candidate(schema, "/definitions/Metadata");
    let source = candidate.source().expect("provider source should survive");

    sim_assert_eq!(have: source.origin(), want: ProviderOrigin::KubernetesOpenApi);
    sim_assert_eq!(have: source.source_id(), want: "default");
    sim_assert_eq!(have: source.version(), want: Some("v1.35.0"));
    sim_assert_eq!(have: source.filename(), want: "io.k8s.api.core.v1.Pod.json");
    sim_assert_eq!(have: source.pointer(), want: "/definitions/Metadata");
}

#[test]
fn repeated_provider_subtrees_with_one_source_use_source_stable_definition_name() {
    let provider_schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "additionalProperties": false
    });
    let source = k8s_source("/definitions/Metadata");
    let definition_name = source_definition_name(&source);
    let mut paths = vec![
        resolved_sourced_path("first", provider_schema.clone(), source.pointer()),
        resolved_sourced_path("second", provider_schema.clone(), source.pointer()),
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(
        have: paths[0].schema,
        want: json!({ "$ref": format!("#/$defs/{definition_name}") })
    );
    sim_assert_eq!(
        have: paths[1].schema,
        want: json!({ "$ref": format!("#/$defs/{definition_name}") })
    );
    sim_assert_eq!(
        have: root.pointer(&format!("/$defs/{definition_name}")),
        want: Some(&provider_schema)
    );
}

#[test]
fn repeated_provider_subtrees_emit_relocated_source_leaf_schema() {
    let provider_schema = json!({
        "type": "object",
        "properties": {
            "labels": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });
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
    let source = k8s_source("/definitions/Metadata");
    let definition_name = source_definition_name(&source);
    let mut paths = vec![
        ResolvedPathSchema {
            value_path: "first".to_string(),
            path_segments: vec!["first".to_string()],
            provider_schema_candidate: Some(sourced_provider_schema_candidate_with_source_schema(
                provider_schema.clone(),
                source.pointer(),
                source_schema.clone(),
            )),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
        ResolvedPathSchema {
            value_path: "second".to_string(),
            path_segments: vec!["second".to_string()],
            provider_schema_candidate: Some(sourced_provider_schema_candidate_with_source_schema(
                provider_schema,
                source.pointer(),
                source_schema.clone(),
            )),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: json!({
                "type": "object",
                "properties": {
                    "labels": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }
                }
            }),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);
    let expected_definition = json!({
        "type": "object",
        "properties": {
            "labels": { "$ref": format!("#/$defs/{definition_name}/$defs/StringMap") }
        },
        "$defs": {
            "StringMap": {
                "type": "object",
                "additionalProperties": { "type": "string" }
            }
        }
    });

    sim_assert_eq!(
        have: paths[0].schema,
        want: json!({ "$ref": format!("#/$defs/{definition_name}") })
    );
    sim_assert_eq!(
        have: root.pointer(&format!("/$defs/{definition_name}")),
        want: Some(&expected_definition)
    );
}

#[test]
fn provider_subtrees_with_provider_local_source_refs_emit_bundled_source_schema() {
    let provider_schema = json!({
        "type": "object",
        "additionalProperties": { "type": "string" }
    });
    let provider_local_ref_schema = json!({ "$ref": "#/definitions/StringMap" });
    let bundled_source_schema = json!({
        "type": "object",
        "additionalProperties": { "$ref": "#/$defs/StringMap" },
        "$defs": {
            "StringMap": {
                "type": "string"
            }
        }
    });
    let source = k8s_source("/definitions/Container/properties/env");
    let definition_name = source_definition_name(&source);
    let mut paths = vec![
        ResolvedPathSchema {
            value_path: "first".to_string(),
            path_segments: vec!["first".to_string()],
            provider_schema_candidate: Some(
                sourced_provider_schema_candidate_with_definition_schema(
                    provider_schema.clone(),
                    source.pointer(),
                    provider_local_ref_schema.clone(),
                    bundled_source_schema.clone(),
                ),
            ),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
        ResolvedPathSchema {
            value_path: "second".to_string(),
            path_segments: vec!["second".to_string()],
            provider_schema_candidate: Some(
                sourced_provider_schema_candidate_with_definition_schema(
                    provider_schema.clone(),
                    source.pointer(),
                    provider_local_ref_schema,
                    bundled_source_schema,
                ),
            ),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(
        have: root.pointer(&format!("/$defs/{definition_name}")),
        want: Some(&json!({
            "type": "object",
            "additionalProperties": {
                "$ref": format!("#/$defs/{definition_name}/$defs/StringMap")
            },
            "$defs": {
                "StringMap": {
                    "type": "string"
                }
            }
        })),
    );
}

#[test]
fn provider_subtrees_require_every_use_to_have_same_definition_schema() {
    let provider_schema = json!({
        "type": "object",
        "additionalProperties": { "type": "string" }
    });
    let internal_ref_source_schema = json!({
        "type": "object",
        "$defs": {
            "StringValue": { "type": "string" }
        },
        "additionalProperties": { "$ref": "#/$defs/StringValue" }
    });
    let provider_local_ref_schema = json!({ "$ref": "#/definitions/StringMap" });
    let source = k8s_source("/definitions/Container/properties/env");
    let definition_name = source_definition_name(&source);
    let mut paths = vec![
        ResolvedPathSchema {
            value_path: "first".to_string(),
            path_segments: vec!["first".to_string()],
            provider_schema_candidate: Some(
                sourced_provider_schema_candidate_with_definition_schema(
                    provider_schema.clone(),
                    source.pointer(),
                    internal_ref_source_schema.clone(),
                    internal_ref_source_schema,
                ),
            ),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
        ResolvedPathSchema {
            value_path: "second".to_string(),
            path_segments: vec!["second".to_string()],
            provider_schema_candidate: Some(sourced_provider_schema_candidate_with_source_schema(
                provider_schema.clone(),
                source.pointer(),
                provider_local_ref_schema,
            )),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(
        have: root.pointer(&format!("/$defs/{definition_name}")),
        want: Some(&provider_schema),
        "mixed self-contained and provider-document-local source leaves must fall back together"
    );
}

#[test]
fn structurally_equal_provider_schemas_share_even_with_different_sources() {
    let provider_schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "additionalProperties": false
    });
    let mut paths = vec![
        ResolvedPathSchema {
            value_path: "first".to_string(),
            path_segments: vec!["first".to_string()],
            provider_schema_candidate: Some(sourced_provider_schema_candidate(
                provider_schema.clone(),
                "/definitions/First",
            )),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
        ResolvedPathSchema {
            value_path: "second".to_string(),
            path_segments: vec!["second".to_string()],
            provider_schema_candidate: Some(sourced_provider_schema_candidate(
                provider_schema.clone(),
                "/definitions/Second",
            )),
            values_yaml_schema: crate::schema_model::empty_schema(),
            schema: provider_schema.clone(),
            used_as_pathless_fragment: false,
            accepted_dependency_values_root_fragment: false,
        },
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(
        have: paths[0].schema,
        want: json!({ "$ref": "#/$defs/providerSchema1" })
    );
    sim_assert_eq!(
        have: paths[1].schema,
        want: json!({ "$ref": "#/$defs/providerSchema1" })
    );
    sim_assert_eq!(
        have: root.pointer("/$defs/providerSchema1"),
        want: Some(&provider_schema)
    );
}

#[test]
fn scalar_provider_schemas_stay_inline() {
    let provider_schema = json!({ "type": "string" });
    let mut paths = vec![
        resolved_path("first", provider_schema.clone()),
        resolved_path("second", provider_schema.clone()),
    ];

    let definitions = extract_provider_definitions(&mut paths, &BTreeMap::new());
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(have: paths[0].schema, want: provider_schema);
    assert!(root.pointer("/$defs").is_none());
}

#[test]
fn described_provider_subtrees_stay_inline_even_when_other_paths_share_definition() {
    let provider_schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "additionalProperties": false
    });
    let mut paths = vec![
        resolved_path("first", provider_schema.clone()),
        resolved_path("second", provider_schema.clone()),
        resolved_path("third", provider_schema.clone()),
    ];
    let descriptions =
        BTreeMap::from([("first.name".to_string(), "chart-authored name".to_string())]);

    let definitions = extract_provider_definitions(&mut paths, &descriptions);
    let mut root = json!({ "type": "object", "properties": {} });
    insert_definitions_into_root(&mut root, definitions);

    sim_assert_eq!(have: paths[0].schema, want: provider_schema);
    sim_assert_eq!(
        have: paths[1].schema,
        want: json!({ "$ref": "#/$defs/providerSchema1" })
    );
    sim_assert_eq!(
        have: paths[2].schema,
        want: json!({ "$ref": "#/$defs/providerSchema1" })
    );
    sim_assert_eq!(
        have: root.pointer("/$defs/providerSchema1"),
        want: Some(&provider_schema)
    );
}
