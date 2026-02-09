use crate::{DefaultValuesSchemaGenerator, ValuesSchemaGenerator};
use helm_schema_ast::{DefineIndex, FusedRustParser, HelmParser};
use helm_schema_ir::{DefaultIrGenerator, IrGenerator};
use helm_schema_k8s::DefaultK8sSchemaProvider;

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
