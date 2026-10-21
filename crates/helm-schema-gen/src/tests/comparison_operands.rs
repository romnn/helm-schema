//! Comparison operands decode through one canonical provenance whatever
//! their spelling: a piped cast is the call it abbreviates, an ordering
//! comparison keeps its sound subset inside a disjunction, and a length
//! bound to a local keeps the character-count subset of its subject.

use super::*;
use indoc::formatdoc;
use test_util::prelude::sim_assert_eq;

fn fail_gate_schema(condition: &str, values_yaml: &str) -> Value {
    let src = formatdoc! {r#"
        {{{{- if {condition} }}}}
        {{{{ fail "rejected" }}}}
        {{{{- end }}}}
    "#};
    schema_for_values_yaml(parse_ir(&src), Some(values_yaml))
}

fn assert_verdicts(schema: &Value, cases: &[(Value, bool)]) {
    for (instance, want) in cases {
        assert!(
            schema_accepts_instance(schema, instance) == *want,
            "instance={instance} want_accept={want}; schema={schema}"
        );
    }
}

/// gitea's multi-replica validators gate on `gt (.Values.replicaCount |
/// int) 1`: the pipeline passes the value as the cast's final argument, so
/// it is the call `int .Values.replicaCount` and lowers through the same
/// raw-integer subset and string preimage.
#[test]
fn piped_int_cast_comparison_decodes_like_the_call_spelling() {
    let call = fail_gate_schema("gt (int .Values.count) 1", "count: 1\n");
    let piped = fail_gate_schema("gt (.Values.count | int) 1", "count: 1\n");
    sim_assert_eq!(have: piped.clone(), want: call);
    sim_assert_eq!(
        have: piped.clone(),
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "if": {
                        "properties": {
                            "count": {
                                "anyOf": [
                                    {
                                        "exclusiveMinimum": 1,
                                        "type": "integer"
                                    },
                                    {
                                        "pattern": "^\\+?([1-9][0-9]{1,17}|[2-9]|0[xX]0*[1-9a-fA-F][0-9a-fA-F]{1,14}|0[xX]0*[2-9a-fA-F]|0[bB]0*1[0-1]{1,61}|0[oO]?0*[1-7][0-7]{1,19}|0[oO]?0*[2-7])$",
                                        "type": "string"
                                    }
                                ]
                            }
                        },
                        "required": [
                            "count"
                        ],
                        "type": "object"
                    },
                    "then": false
                }
            ],
            "properties": {
                "count": {},
                "global": {}
            },
            "type": "object"
        }),
    );
    assert_verdicts(
        &piped,
        &[
            (serde_json::json!({ "count": 2 }), false),
            // A clean decimal spelling coerces into the failing region.
            (serde_json::json!({ "count": "2" }), false),
            (serde_json::json!({ "count": 1 }), true),
            (serde_json::json!({ "count": "1" }), true),
            // A missing count coerces to 0 and never enters the region.
            (serde_json::json!({}), true),
        ],
    );
}

/// `default` substitutes its literal for EVERY Helm-falsy input before the
/// cast — absent, null, 0 and the empty string alike — so a region the
/// fallback escapes claims only truthy inputs. The pipeline spelling is the
/// same call.
#[test]
fn defaulted_int_cast_region_claims_only_truthy_inputs() {
    let call = fail_gate_schema("gt 1 (int (default 2 .Values.count))", "count: 1\n");
    let piped = fail_gate_schema("gt 1 (.Values.count | default 2 | int)", "count: 1\n");
    sim_assert_eq!(have: piped, want: call.clone());
    sim_assert_eq!(
        have: call.clone(),
        want: serde_json::json!({
            "$defs": {
                "t": {
                    "anyOf": [
                        {
                            "const": true
                        },
                        {
                            "not": {
                                "const": 0
                            },
                            "type": "number"
                        },
                        {
                            "minLength": 1,
                            "type": "string"
                        },
                        {
                            "minItems": 1,
                            "type": "array"
                        },
                        {
                            "minProperties": 1,
                            "type": "object"
                        }
                    ]
                }
            },
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "if": {
                        "allOf": [
                            {
                                "properties": {
                                    "count": {
                                        "$ref": "#/$defs/t"
                                    }
                                },
                                "required": [
                                    "count"
                                ],
                                "type": "object"
                            },
                            {
                                "anyOf": [
                                    {
                                        "not": {
                                            "properties": {
                                                "count": {}
                                            },
                                            "required": [
                                                "count"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    {
                                        "properties": {
                                            "count": {
                                                "anyOf": [
                                                    {
                                                        "exclusiveMaximum": 1,
                                                        "type": "integer"
                                                    },
                                                    {
                                                        "not": {
                                                            "pattern": "^\\+?([1-9][0-9]{1,17}|[1-9]|0[xX]0*[1-9a-fA-F][0-9a-fA-F]{1,14}|0[xX]0*[1-9a-fA-F]|0[bB]0*1[0-1]{1,61}|0[bB]0*1|0[oO]?0*[1-7][0-7]{1,19}|0[oO]?0*[1-7]|[0-9_]*[1-9][0-9_]*_[0-9_]*|[0-9_]*_[0-9_]*[1-9][0-9_]*|0[xX][0-9a-fA-F_]*[1-9a-fA-F][0-9a-fA-F_]*_[0-9a-fA-F_]*|0[xX][0-9a-fA-F_]*_[0-9a-fA-F_]*[1-9a-fA-F][0-9a-fA-F_]*|0[bB][01_]*1[01_]*_[01_]*|0[bB][01_]*_[01_]*1[01_]*|0[oO]?[0-7_]*[1-7][0-7_]*_[0-7_]*|0[oO]?[0-7_]*_[0-7_]*[1-7][0-7_]*|[1-9][0-9]{18}|0[xX]0*[1-9a-fA-F][0-9a-fA-F]{15}|0[bB]0*1[01]{62}|0[oO]?0*[1-7][0-7]{20})(\\.0*)?$"
                                                        },
                                                        "type": "string"
                                                    }
                                                ]
                                            }
                                        },
                                        "required": [
                                            "count"
                                        ],
                                        "type": "object"
                                    }
                                ]
                            }
                        ]
                    },
                    "then": false
                }
            ],
            "properties": {
                "count": {},
                "global": {}
            },
            "type": "object"
        }),
    );
    assert_verdicts(
        &call,
        &[
            // Every Sprig-empty value takes the fallback 2, outside the region.
            (serde_json::json!({}), true),
            (serde_json::json!({ "count": null }), true),
            (serde_json::json!({ "count": "" }), true),
            (serde_json::json!({ "count": 0 }), true),
            (serde_json::json!({ "count": 0.0 }), true),
            (serde_json::json!({ "count": false }), true),
            (serde_json::json!({ "count": {} }), true),
            (serde_json::json!({ "count": [] }), true),
            (serde_json::json!({ "count": 1 }), true),
            (serde_json::json!({ "count": -1 }), false),
            // A non-empty spelling bypasses the fallback and coerces to 0.
            (serde_json::json!({ "count": "0" }), false),
        ],
    );
}

/// trino's worker region `or .Values.server.keda.enabled (gt (int
/// .Values.server.workers) 0)`: the ordering disjunct keeps its raw-integer
/// subset beside the exact truthy disjunct instead of being dropped.
#[test]
fn ordering_comparison_keeps_its_subset_inside_or() {
    let schema = fail_gate_schema(
        "or .Values.enabled (gt (int .Values.count) 0)",
        indoc! {"
            enabled: false
            count: 0
        "},
    );
    sim_assert_eq!(
        have: schema.clone(),
        want: serde_json::json!({
            "$defs": {
                "t": {
                    "anyOf": [
                        {
                            "const": true
                        },
                        {
                            "not": {
                                "const": 0
                            },
                            "type": "number"
                        },
                        {
                            "minLength": 1,
                            "type": "string"
                        },
                        {
                            "minItems": 1,
                            "type": "array"
                        },
                        {
                            "minProperties": 1,
                            "type": "object"
                        }
                    ]
                }
            },
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "if": {
                        "anyOf": [
                            {
                                "properties": {
                                    "enabled": {
                                        "$ref": "#/$defs/t"
                                    }
                                },
                                "required": [
                                    "enabled"
                                ],
                                "type": "object"
                            },
                            {
                                "properties": {
                                    "count": {
                                        "anyOf": [
                                            {
                                                "exclusiveMinimum": 0,
                                                "type": "integer"
                                            },
                                            {
                                                "pattern": "^\\+?([1-9][0-9]{1,17}|[1-9]|0[xX]0*[1-9a-fA-F][0-9a-fA-F]{1,14}|0[xX]0*[1-9a-fA-F]|0[bB]0*1[0-1]{1,61}|0[bB]0*1|0[oO]?0*[1-7][0-7]{1,19}|0[oO]?0*[1-7])$",
                                                "type": "string"
                                            }
                                        ]
                                    }
                                },
                                "required": [
                                    "count"
                                ],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                }
            ],
            "properties": {
                "count": {},
                "enabled": {},
                "global": {}
            },
            "type": "object"
        }),
    );
    assert_verdicts(
        &schema,
        &[
            (serde_json::json!({ "enabled": false, "count": 2 }), false),
            (serde_json::json!({ "enabled": false, "count": "3" }), false),
            (serde_json::json!({ "enabled": true, "count": 0 }), false),
            (serde_json::json!({ "enabled": false, "count": 0 }), true),
        ],
    );
}

/// datadog's `$length := len $clusterName` / `gt $length 80`: the local
/// carries the evaluated length provenance, so it lowers exactly like the
/// call and pipeline spellings. Go counts BYTES while the pattern counts
/// characters: more than 8 characters certainly exceeds 8 bytes, but a
/// shorter multibyte string that exceeds the byte bound stays accepted —
/// the subset never claims the exact byte condition.
#[test]
fn bound_local_length_keeps_the_character_count_subset() {
    let call = fail_gate_schema("gt (len .Values.name) 8", "name: short\n");
    let piped = fail_gate_schema("gt (.Values.name | len) 8", "name: short\n");
    let local_src = indoc! {r#"
        {{- $length := len .Values.name -}}
        {{- if gt $length 8 }}
        {{ fail "rejected" }}
        {{- end }}
    "#};
    let local = schema_for_values_yaml(parse_ir(local_src), Some("name: short\n"));
    sim_assert_eq!(have: piped, want: call.clone());
    sim_assert_eq!(have: local.clone(), want: call);
    sim_assert_eq!(
        have: local.clone(),
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "additionalProperties": {},
                    "properties": {
                        "name": {
                            "not": {
                                "type": "boolean"
                            }
                        }
                    }
                },
                {
                    "additionalProperties": {},
                    "properties": {
                        "name": {
                            "not": {
                                "type": "integer"
                            }
                        }
                    }
                },
                {
                    "additionalProperties": {},
                    "properties": {
                        "name": {
                            "not": {
                                "type": "number"
                            }
                        }
                    }
                },
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": {
                                        "name": {}
                                    },
                                    "required": [
                                        "name"
                                    ],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": {
                                    "name": {
                                        "enum": [
                                            null
                                        ]
                                    }
                                },
                                "required": [
                                    "name"
                                ],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                },
                {
                    "if": {
                        "properties": {
                            "name": {
                                "pattern": "^[\\s\\S]{9,}$",
                                "type": "string"
                            }
                        },
                        "required": [
                            "name"
                        ],
                        "type": "object"
                    },
                    "then": false
                }
            ],
            "properties": {
                "global": {},
                "name": {}
            },
            "type": "object"
        }),
    );
    assert_verdicts(
        &local,
        &[
            (serde_json::json!({ "name": "123456789" }), false),
            (serde_json::json!({ "name": "ééééééééé" }), false),
            (serde_json::json!({ "name": "12345678" }), true),
            (serde_json::json!({ "name": "éééééééé" }), true),
        ],
    );
}

/// gitea's multi-replica `fail`s sit in a block-scalar `assertions: |`
/// value, where control regions lower through the inline lane: the
/// comparison keeps the same structural subset there as in a document.
#[test]
fn block_scalar_region_keeps_the_ordering_subset() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          name: checks
        stringData:
          assertions: |
            {{- if gt (.Values.count | int) 1 -}}
              {{- fail "rejected" -}}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("count: 1\n"));
    sim_assert_eq!(
        have: schema.clone(),
        want: fail_gate_schema("gt (int .Values.count) 1", "count: 1\n"),
    );
    assert_verdicts(
        &schema,
        &[
            (serde_json::json!({ "count": 2 }), false),
            (serde_json::json!({ "count": 1 }), true),
        ],
    );
}
