use test_util::prelude::sim_assert_eq;

use super::*;

/// Compact control actions (`{{if(…)}}…{{end}}`) inside a block scalar are
/// control structure, not output (Helm 4.2.3 renders `hello` with a truthy
/// flag and an empty scalar otherwise): the block stays opaque text whose
/// reads constrain nothing.
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
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "flag": {},
                "x": {},
            },
            "type": "object",
        })
    );
}
