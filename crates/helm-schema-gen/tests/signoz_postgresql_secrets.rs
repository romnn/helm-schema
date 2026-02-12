#![recursion_limit = "512"]

use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_gen::generate_values_schema_with_values_yaml;
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;

const TEMPLATE_PATH: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/secrets.yaml";
const VALUES_PATH: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/values.yaml";
const HELPERS_PATH: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/templates/_helpers.tpl";
const COMMON_TEMPLATES_DIR: &str =
    "charts/signoz-signoz/charts/signoz-otel-gateway/charts/postgresql/charts/common/templates";

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(parser, &test_util::read_testdata(HELPERS_PATH));
    for src in test_util::read_testdata_dir(COMMON_TEMPLATES_DIR, "tpl") {
        let _ = idx.add_source(parser, &src);
    }
    idx
}

#[test]
#[allow(clippy::too_many_lines)]
fn schema_fused_rust() {
    let src = test_util::read_testdata(TEMPLATE_PATH);
    let values_yaml = test_util::read_testdata(VALUES_PATH);
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = SymbolicIrGenerator.generate(&src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.35.0").with_allow_download(true);
    let schema = generate_values_schema_with_values_yaml(&ir, &provider, Some(&values_yaml));

    let actual: serde_json::Value = schema;

    if std::env::var("SCHEMA_DUMP").is_ok() {
        eprintln!(
            "{}",
            serde_json::to_string_pretty(&actual).expect("pretty json")
        );
    }

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "architecture": {
                "anyOf": [
                    { "enum": ["replication"] },
                    { "type": "string" }
                ]
            },
            "auth": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enablePostgresUser": { "type": "boolean" },
                    "password": { "type": "string" },
                    "postgresPassword": { "type": "string" },
                    "secretKeys": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "adminPasswordKey": { "type": "string" },
                            "replicationPasswordKey": { "type": "string" },
                            "userPasswordKey": { "type": "string" }
                        }
                    }
                }
            },
            "commonAnnotations": {
                "type": "object",
                "additionalProperties": { "type": "string" },
                "description": "Annotations is an unstructured key value map stored with a resource that may be set by external tools to store and retrieve arbitrary metadata. They are not queryable and should be preserved when modifying objects. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/annotations"
            },
            "commonLabels": {
                "type": "object",
                "additionalProperties": { "type": "string" },
                "description": "Map of string keys and values that can be used to organize and categorize (scope and select) objects. May match selectors of replication controllers and services. More info: https://kubernetes.io/docs/concepts/overview/working-with-objects/labels"
            },
            "global": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "postgresql": {
                        "type": "object",
                        "additionalProperties": false,
                        "properties": {
                            "auth": {
                                "type": "object",
                                "additionalProperties": false,
                                "properties": {
                                    "password": { "type": "string" },
                                    "postgresPassword": { "type": "string" },
                                    "secretKeys": {
                                        "type": "object",
                                        "additionalProperties": false,
                                        "properties": {
                                            "adminPasswordKey": { "type": "string" },
                                            "replicationPasswordKey": { "type": "string" },
                                            "userPasswordKey": { "type": "string" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "ldap": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "bind_password": { "type": "boolean" },
                    "bindpw": { "type": "string" },
                    "enabled": { "type": "boolean" }
                }
            },
            "serviceBindings": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": { "type": "boolean" }
                }
            }
        }
    });

    similar_asserts::assert_eq!(actual, expected);
}
