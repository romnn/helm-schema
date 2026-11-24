//! Service port demands reach the original subject of an aliased split.

use color_eyre::eyre;
use helm_schema_ast::DefineIndex;
use helm_schema_gen::{PreparedValuesDocuments, ValuesSchemaInput, generate_values_schema};
use helm_schema_ir::SymbolicIrContext;
use helm_schema_k8s::Chain;
use helm_schema_test_support::generate::bundled_k8s_provider;
use indoc::indoc;
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;

fn service_schema(source: &str) -> eyre::Result<Value> {
    let signals = SymbolicIrContext::new(&DefineIndex::new())
        .generate_contract_ir(source)
        .finalize()
        .into_schema_signals();
    let provider = Chain::new(vec![Box::new(bundled_k8s_provider(
        "v1.29.0-standalone-strict",
    ))]);
    let defaults = PreparedValuesDocuments::new(
        serde_yaml::from_str("endpoint: host:16")?,
        serde_yaml::Value::Null,
    );
    Ok(generate_values_schema(
        ValuesSchemaInput::new(&signals, &provider).with_values_documents(&defaults),
    ))
}

fn expected_service_schemas() -> [Value; 2] {
    let present_endpoint = |constraint: Value| {
        json!({
            "properties": {"endpoint": constraint},
            "required": ["endpoint"],
            "type": "object"
        })
    };
    let string_contract = json!({
        "additionalProperties": {},
        "properties": {"endpoint": {"type": "string"}}
    });
    let strict_call_abort = json!({
        "if": {"anyOf": [
            {"anyOf": [
                {"not": present_endpoint(json!({}))},
                present_endpoint(json!({"enum": [null]}))
            ]},
            {"$ref": "#/definitions/helm-condition-0"}
        ]},
        "then": false
    });
    let wrong_input_type = json!({"not": present_endpoint(json!({"type": "string"}))});
    let complete_schema = |endpoint: Value, all_of: Vec<Value>, definitions: Value| {
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": all_of,
            "definitions": definitions,
            "properties": {"endpoint": endpoint, "global": {}},
            "required": ["endpoint"],
            "type": "object"
        })
    };

    let numeric_endpoint = json!({
        "type": "string",
        "not": {"pattern": r"(^|:)[ \t]*([A-Za-z_][A-Za-z0-9_./+-]*[ \t]*)?$"}
    });
    let truthy_suffix_contract = json!({
        "if": present_endpoint(json!({"$ref": "#/definitions/t"})),
        "then": {"additionalProperties": {}, "properties": {"endpoint": numeric_endpoint}}
    });
    [
        complete_schema(
            numeric_endpoint,
            vec![
                truthy_suffix_contract,
                string_contract.clone(),
                strict_call_abort.clone(),
            ],
            json!({
                "helm-condition-0": wrong_input_type,
                "t": {"anyOf": [
                    {"const": true},
                    {"type": "number", "not": {"const": 0}},
                    {"type": "string", "minLength": 1},
                    {"type": "array", "minItems": 1},
                    {"type": "object", "minProperties": 1}
                ]}
            }),
        ),
        complete_schema(
            json!({"allOf": [{}, {"not": {"type": "null"}}]}),
            vec![string_contract, strict_call_abort],
            json!({"helm-condition-0": wrong_input_type}),
        ),
    ]
}

#[test]
fn split_alias_retains_the_original_source_and_respects_the_destination() -> eyre::Result<()> {
    let source = indoc! {r#"
        {{- $parts := regexSplit ":" .Values.endpoint -1 }}
        {{- $alias := $parts }}
        {{- $_ := set .Values "endpoint" "host:16" }}
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            - port: 80
              targetPort: {{ last $alias }}
        ---
        apiVersion: v1
        kind: Service
        metadata:
          name: numeric
        spec:
          ports:
            - port: {{ last $alias }}
    "#};
    let string_sink = indoc! {r#"
        {{- $parts := regexSplit ":" .Values.endpoint -1 }}
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            - port: 80
              targetPort: {{ last $parts }}
    "#};

    let numeric = service_schema(source)?;
    let named = service_schema(string_sink)?;
    for (schema, endpoint, accepted) in [
        (&numeric, "host:audit", false),
        (&numeric, "host:0x10", true),
        (&named, "host:audit", true),
    ] {
        sim_assert_eq!(
            have: jsonschema::validator_for(schema)?.is_valid(&json!({"endpoint": endpoint})),
            want: accepted,
            "{endpoint}",
        );
    }

    let [expected_numeric, expected_named] = expected_service_schemas();
    sim_assert_eq!(have: numeric, want: expected_numeric);
    sim_assert_eq!(have: named, want: expected_named);
    Ok(())
}
