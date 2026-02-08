use crate::{DefaultValuesSchemaGenerator, ValuesSchemaGenerator};
use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser, TreeSitterParser};
use helm_schema_ir::{DefaultIrGenerator, IrGenerator};
use helm_schema_k8s::DefaultK8sSchemaProvider;

fn prometheusrule_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/prometheusrule.yaml"
    );
    std::fs::read_to_string(path).expect("read prometheusrule.yaml")
}

fn helpers_src() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../testdata/charts/bitnami-redis/templates/_helpers.tpl"
    );
    std::fs::read_to_string(path).expect("read _helpers.tpl")
}

fn build_define_index(parser: &dyn HelmParser) -> DefineIndex {
    let mut idx = DefineIndex::new();
    let _ = idx.add_source(parser, &helpers_src());
    idx
}

/// Full schema generation for prometheusrule using fused-Rust parser.
#[test]
fn schema_for_prometheusrule_fused_rust() {
    let src = prometheusrule_src();
    let ast = FusedRustParser.parse(&src).expect("parse");
    let idx = build_define_index(&FusedRustParser);
    let ir = DefaultIrGenerator.generate(&ast, &idx);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &DefaultK8sSchemaProvider);

    let actual: serde_json::Value = schema;

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {
            "commonAnnotations": {
                "anyOf": [
                    {
                        "type": "object",
                        "additionalProperties": {}
                    },
                    {
                        "type": "string"
                    }
                ]
            },
            "commonLabels": {
                "type": "object",
                "additionalProperties": {"type": "string"}
            },
            "metrics": {
                "type": "object",
                "properties": {
                    "enabled": {
                        "type": "boolean"
                    },
                    "prometheusRule": {
                        "type": "object",
                        "properties": {
                            "additionalLabels": {
                                "anyOf": [
                                    {
                                        "type": "object",
                                        "additionalProperties": {}
                                    },
                                    {
                                        "type": "string"
                                    }
                                ]
                            },
                            "enabled": {
                                "type": "boolean"
                            },
                            "namespace": {
                                "type": "string"
                            },
                            "rules": {
                                "type": "object",
                                "additionalProperties": {}
                            }
                        },
                        "additionalProperties": false
                    }
                },
                "additionalProperties": false
            }
        },
        "additionalProperties": false
    });

    similar_asserts::assert_eq!(actual, expected);
}

/// Schema generation using tree-sitter parser should produce same result.
#[test]
fn schema_for_prometheusrule_tree_sitter() {
    let src = prometheusrule_src();

    let rust_ast = FusedRustParser.parse(&src).expect("fused rust");
    let rust_idx = build_define_index(&FusedRustParser);
    let rust_ir = DefaultIrGenerator.generate(&rust_ast, &rust_idx);
    let rust_schema = DefaultValuesSchemaGenerator.generate(&rust_ir, &DefaultK8sSchemaProvider);

    let ts_ast = TreeSitterParser.parse(&src).expect("tree-sitter");
    let ts_idx = build_define_index(&TreeSitterParser);
    let ts_ir = DefaultIrGenerator.generate(&ts_ast, &ts_idx);
    let ts_schema = DefaultValuesSchemaGenerator.generate(&ts_ir, &DefaultK8sSchemaProvider);

    similar_asserts::assert_eq!(rust_schema, ts_schema);
}

/// Simple template produces correct schema structure.
#[test]
fn simple_template_schema() {
    let src = r#"{{- if .Values.enabled }}
foo: {{ .Values.name }}
replicas: {{ .Values.replicas }}
{{- end }}
"#;
    let ast = FusedRustParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = DefaultIrGenerator.generate(&ast, &idx);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &DefaultK8sSchemaProvider);

    // enabled is a guard → boolean, name is scalar → string, replicas → integer
    assert_eq!(
        schema.get("$schema").and_then(|v| v.as_str()),
        Some("http://json-schema.org/draft-07/schema#")
    );

    let props = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .expect("properties");
    // Plain "enabled" (no parent path) is treated as a string scalar, not boolean.
    // Boolean inference only applies to "*.enabled" patterns with a dot prefix.
    assert_eq!(
        props
            .get("enabled")
            .and_then(|v| v.get("type"))
            .and_then(|t| t.as_str()),
        Some("string")
    );
    assert_eq!(
        props
            .get("name")
            .and_then(|v| v.get("type"))
            .and_then(|t| t.as_str()),
        Some("string")
    );
    assert_eq!(
        props
            .get("replicas")
            .and_then(|v| v.get("type"))
            .and_then(|t| t.as_str()),
        Some("integer")
    );
}

/// Guard-like values (*.enabled) get boolean type.
#[test]
fn guard_values_get_boolean_type() {
    let src = r#"{{- if .Values.feature.enabled }}
key: {{ .Values.feature.name }}
{{- end }}
"#;
    let ast = FusedRustParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = DefaultIrGenerator.generate(&ast, &idx);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &DefaultK8sSchemaProvider);

    let enabled_schema = schema
        .get("properties")
        .and_then(|p| p.get("feature"))
        .and_then(|f| f.get("properties"))
        .and_then(|p| p.get("enabled"));

    assert_eq!(
        enabled_schema,
        Some(&serde_json::json!({"type": "boolean"}))
    );
}
