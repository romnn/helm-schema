use crate::{DefaultValuesSchemaGenerator, ValuesSchemaGenerator};
use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
use helm_schema_ir::{IrGenerator, SymbolicIrGenerator};
use helm_schema_k8s::UpstreamK8sSchemaProvider;

/// Simple template produces correct schema structure.
#[test]
fn simple_template_schema() {
    let src = r#"{{- if .Values.enabled }}
foo: {{ .Values.name }}
replicas: {{ .Values.replicas }}
{{- end }}
"#;
    let ast = TreeSitterParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = SymbolicIrGenerator.generate(src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/kubernetes-json-schema"
        ))
        .with_allow_download(false);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &provider);

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "enabled": {"type": "boolean"},
            "name": {},
            "replicas": {}
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}

/// Guard-like values (*.enabled) get boolean type.
#[test]
fn guard_values_get_boolean_type() {
    let src = r#"{{- if .Values.feature.enabled }}
key: {{ .Values.feature.name }}
{{- end }}
"#;
    let ast = TreeSitterParser.parse(src).expect("parse");
    let idx = DefineIndex::new();
    let ir = SymbolicIrGenerator.generate(src, &ast, &idx);
    let provider = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../testdata/kubernetes-json-schema"
        ))
        .with_allow_download(false);
    let schema = DefaultValuesSchemaGenerator.generate(&ir, &provider);

    let expected = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "feature": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "enabled": {"type": "boolean"},
                    "name": {}
                }
            }
        }
    });
    similar_asserts::assert_eq!(schema, expected);
}
