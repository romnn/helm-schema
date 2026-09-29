use test_util::prelude::sim_assert_eq;

use super::*;

/// `{{-3}}` is a plain `{{` before the literal `-3`, not a trim marker, so
/// the helper renders `prefix -3` under a truthy flag (Helm 4.2.3:
/// `--set flag=true` aborts with `boom`; `flag=false` renders). The
/// comparison against the helper's output therefore selects the `fail`
/// branch exactly when the flag is truthy.
#[test]
fn negative_literal_output_does_not_trim_the_preceding_text() {
    let helpers = r#"{{- define "h" -}}{{ if .Values.flag }}prefix {{-3}}{{ end }}{{- end -}}"#;
    let src = indoc! {r#"
        {{- $mode := include "h" . }}
        {{- if eq $mode "prefix -3" }}{{ fail "boom" }}{{ end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: example
        data:
          key: {{ if .Values.flag }}prefix {{-3}} {{ .Values.x }}{{ end }}
    "#};
    let values_yaml = indoc! {"
        flag: false
        x: hello
    "};

    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "$defs": {
                "t": {
                    "anyOf": [
                        { "const": true },
                        { "not": { "const": 0 }, "type": "number" },
                        { "minLength": 1, "type": "string" },
                        { "minItems": 1, "type": "array" },
                        { "minProperties": 1, "type": "object" },
                    ],
                },
            },
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "if": {
                        "properties": { "flag": { "$ref": "#/$defs/t" } },
                        "required": ["flag"],
                        "type": "object",
                    },
                    "then": false,
                },
            ],
            "properties": {
                "flag": {},
                "global": {},
                "x": {},
            },
            "type": "object",
        })
    );
}
