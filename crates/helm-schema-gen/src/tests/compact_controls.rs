use test_util::prelude::sim_assert_eq;

use super::*;

/// Compact control actions (`{{if(…)}}…{{end}}`) inside a block scalar are
/// control structure, not output (Helm 4.2.3 renders `hello` with a truthy
/// flag and an empty scalar otherwise). The opaque leaf's declared policy
/// applies only while its own consumption executes.
#[test]
fn compact_control_actions_in_a_block_scalar_stay_block_text() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: block
        data:
          key: |-
            {{if(.Values.flag)}}{{ .Values.x }}{{end}}
    "};
    let values_yaml = indoc! {"
        flag: true
        x: hello
    "};

    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "$defs": {
                "t": {"anyOf": [{"const": true}, {"type": "number", "not": {"const": 0}}, {"type": "string", "minLength": 1}, {"type": "array", "minItems": 1}, {"type": "object", "minProperties": 1}]},
                "helm-condition-0": {"if": {"type": "object", "properties": {"flag": {"$ref": "#/$defs/t"}}, "required": ["flag"]}, "then": true, "else": false}
            },
            "allOf": [{"if": {"$ref": "#/$defs/helm-condition-0"}, "then": {"additionalProperties": {}, "properties": {"x": {"type": "string"}}}}],
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "flag": {},
                "global": {},
                "x": {},
            },
            "type": "object",
        })
    );
}
