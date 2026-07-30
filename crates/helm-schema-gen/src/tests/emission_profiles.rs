use super::*;
use crate::SchemaProfile;
use test_util::prelude::sim_assert_eq;

#[test]
fn lean_profile_omits_only_document_level_conditionals() {
    let source = indoc! {r#"
        {{- if .Values.enabled }}
        data:
          message: {{ .Values.message | quote }}
        {{- end }}
        {{- if .Values.forbidden }}
        {{- fail "forbidden" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: probe
    "#};
    let values_yaml = indoc! {"
        enabled: false
        forbidden: false
        message: hello
    "};
    let signals = schema_signals_for(parse_ir(source));

    let default_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider).with_values_yaml(Some(values_yaml)),
    );
    let full_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_profile(SchemaProfile::Full),
    );
    let lean_schema = generate_values_schema(
        ValuesSchemaInput::new(&signals, &NoopProvider)
            .with_values_yaml(Some(values_yaml))
            .with_profile(SchemaProfile::Lean),
    );

    sim_assert_eq!(have: default_schema, want: full_schema);
    sim_assert_eq!(
        have: lean_schema,
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "enabled": {},
                "forbidden": {},
                "message": {},
            },
            "type": "object",
        })
    );

    for instance in [
        serde_json::json!({}),
        serde_json::json!({
            "enabled": false,
            "forbidden": false,
            "message": "hello",
        }),
        serde_json::json!({
            "enabled": true,
            "forbidden": false,
            "message": "hello",
        }),
    ] {
        assert!(schema_accepts_instance(&full_schema, &instance));
        assert!(schema_accepts_instance(&lean_schema, &instance));
    }
    assert!(!schema_accepts_instance(
        &full_schema,
        &serde_json::json!({ "forbidden": true })
    ));
    assert!(schema_accepts_instance(
        &lean_schema,
        &serde_json::json!({ "forbidden": true })
    ));
}
