use test_util::prelude::sim_assert_eq;

use super::*;

const CONSTANT_CONFIGMAP: &str = indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: example
    data:
      key: value
"};

fn dependency_contract(root_src: &str, instances: &[(&str, &str)]) -> ContractIr {
    let mut contract = parse_ir(root_src);
    for (prefix, src) in instances {
        let prefix_segments = helm_schema_core::split_value_path(prefix);
        let mut instance = parse_ir(src);
        instance.map_value_paths(|path| {
            helm_schema_core::ValuesPath::from_segments(
                prefix_segments
                    .iter()
                    .cloned()
                    .map(helm_schema_core::Segment::from)
                    .chain(path.segments().cloned()),
            )
        });
        instance.project_dependency_global_contracts(&prefix_segments);
        contract.append(instance);
        contract.push_dependency_values_root(undeclared_dependency_root(prefix));
    }
    contract
}

const REGISTRY_CONSUMER: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: registry
    data:
      registry: {{ .Values.global.imageRegistry | default "none" | trunc 63 | quote }}
"#};

const GUARDED_CONSUMER: &str = indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: registry
    {{- if .Values.global.enabled }}
    data:
      registry: {{ .Values.global.imageRegistry | trunc 63 | quote }}
    {{- end }}
"};

const DECLARED_GLOBAL: &str = indoc! {r#"
    global:
      imageRegistry: ""
      pullSecrets: []
"#};

/// Helm accepts a root `global` of any shape and skips injecting a non-map
/// one (matrix cells u1, u2, s6), so the reserved property carries no type,
/// requirement, or declared shape of its own.
#[test]
fn unconsumed_global_is_reserved_without_shape_at_the_root() {
    let want = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {}
        },
        "type": "object"
    });
    for values_yaml in [None, Some(DECLARED_GLOBAL)] {
        let schema = schema_for_values_yaml(parse_ir(CONSTANT_CONFIGMAP), values_yaml);
        sim_assert_eq!(have: &schema, want: &want);
    }
    let accepted = [
        serde_json::json!({}),
        serde_json::json!({ "global": { "imageRegistry": "x", "other": 1 } }),
        serde_json::json!({ "global": 5 }),
        serde_json::json!({ "global": ["a"] }),
        serde_json::json!({ "global": null }),
    ]
    .iter()
    .map(|instance| schema_accepts_instance(&want, instance))
    .collect::<Vec<_>>();
    sim_assert_eq!(have: accepted, want: vec![true; 5]);
}

/// A consumed member keeps its contract; the namespace stays open to the
/// members other charts in the tree share, and an unread declared member
/// (`pullSecrets`) gains no declared restriction.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn consumed_global_member_keeps_its_contract_and_admits_shared_members() {
    let schema = schema_for_values_yaml(parse_ir(REGISTRY_CONSUMER), Some(DECLARED_GLOBAL));
    sim_assert_eq!(have: &schema, want: &serde_json::json!({
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
                    "properties": {
                        "global": {
                            "properties": {
                                "imageRegistry": {
                                    "$ref": "#/$defs/t"
                                }
                            },
                            "required": [
                                "imageRegistry"
                            ],
                            "type": "object"
                        }
                    },
                    "required": [
                        "global"
                    ],
                    "type": "object"
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {
                            "additionalProperties": {},
                            "properties": {
                                "imageRegistry": {
                                    "type": [
                                        "null",
                                        "string"
                                    ]
                                }
                            }
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
                                    "global": {}
                                },
                                "required": [
                                    "global"
                                ],
                                "type": "object"
                            }
                        },
                        {
                            "properties": {
                                "global": {
                                    "enum": [
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "global": {
                "additionalProperties": {},
                "properties": {
                    "imageRegistry": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    let verdicts = [
        serde_json::json!({ "global": { "imageRegistry": "x" } }),
        serde_json::json!({ "global": { "imageRegistry": "x", "other": 1, "pullSecrets": "s" } }),
        serde_json::json!({ "global": { "imageRegistry": 1 } }),
        serde_json::json!({ "global": { "imageRegistry": { "a": "b" } } }),
    ]
    .iter()
    .map(|instance| schema_accepts_instance(&schema, instance))
    .collect::<Vec<_>>();
    sim_assert_eq!(have: verdicts, want: vec![true, true, false, false]);
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn guarded_global_reads_keep_their_guards() {
    let schema = schema_for_values_yaml(parse_ir(GUARDED_CONSUMER), None);
    sim_assert_eq!(have: schema, want: serde_json::json!({
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
                    "properties": {
                        "global": {
                            "properties": {
                                "enabled": {
                                    "$ref": "#/$defs/t"
                                }
                            },
                            "required": [
                                "enabled"
                            ],
                            "type": "object"
                        }
                    },
                    "required": [
                        "global"
                    ],
                    "type": "object"
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {
                            "additionalProperties": {},
                            "properties": {
                                "imageRegistry": {
                                    "type": [
                                        "null",
                                        "string"
                                    ]
                                }
                            }
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
                                    "global": {}
                                },
                                "required": [
                                    "global"
                                ],
                                "type": "object"
                            }
                        },
                        {
                            "properties": {
                                "global": {
                                    "enum": [
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "global": {
                "additionalProperties": {},
                "allOf": [
                    {
                        "if": {
                            "allOf": [
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
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "imageRegistry": {}
                                                },
                                                "required": [
                                                    "imageRegistry"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "imageRegistry": {
                                                    "enum": [
                                                        null
                                                    ]
                                                }
                                            },
                                            "required": [
                                                "imageRegistry"
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
                    "enabled": {},
                    "imageRegistry": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
}

/// Helm injects `global` into every dependency instance, whatever its
/// alias: two aliases of one child each reserve the namespace.
#[test]
fn every_dependency_alias_reserves_its_global_namespace() {
    let schema = schema_for_dependency_values_yaml(
        dependency_contract(
            CONSTANT_CONFIGMAP,
            &[("kid", CONSTANT_CONFIGMAP), ("pup", CONSTANT_CONFIGMAP)],
        ),
        indoc! {r#"
            kid:
              global:
                imageRegistry: ""
            pup:
              global: {}
        "#},
        "",
    );
    sim_assert_eq!(have: schema, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "global": {}
                },
                "type": "object"
            },
            "pup": {
                "additionalProperties": {},
                "properties": {
                    "global": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
}

#[test]
fn nested_dependency_instances_reserve_their_global_namespaces() {
    let schema = schema_for_dependency_values_yaml(
        dependency_contract(
            CONSTANT_CONFIGMAP,
            &[("kid", CONSTANT_CONFIGMAP), ("kid.gc", CONSTANT_CONFIGMAP)],
        ),
        indoc! {"
            kid:
              global: {}
              gc:
                global: {}
        "},
        "",
    );
    sim_assert_eq!(have: schema, want: serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "gc": {
                        "additionalProperties": {},
                        "properties": {
                            "global": {}
                        },
                        "type": "object"
                    },
                    "global": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
}

/// Reserving the namespace never weakens an unconditional termination.
#[test]
fn reservation_preserves_a_terminal_false_root() {
    let schema = schema_for_values_yaml(parse_ir("{{ fail \"no\" }}\n"), None);
    sim_assert_eq!(have: &schema, want: &serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            false
        ],
        "properties": {
            "global": {}
        },
        "type": "object"
    }));
    sim_assert_eq!(
        have: schema_accepts_instance(&schema, &serde_json::json!({})),
        want: false
    );
}

/// A dependency aliased `global` makes the root namespace a dependency
/// values root, and Helm v4.2.3 type-asserts it (matrix cells g1-g6): the
/// registration is Helm's own contract, not synthetic evidence, so the
/// reservation keeps the table requirement and opens the namespace, where an
/// unconsumed `{}` would accept the aborts. The documents are the COALESCED
/// ones Helm validates. Without a parent default (g4, g5) a user `5` or
/// `null` reaches the assertion and aborts; with a declared `global: {}`
/// default (g6) a user `null` is deleted first and the dependency is refilled
/// with its own table, which renders.
#[test]
fn a_dependency_aliased_global_keeps_its_table_requirement() {
    let want = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {
                "properties": {
                    "global": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    });
    let contract = || dependency_contract(CONSTANT_CONFIGMAP, &[("global", CONSTANT_CONFIGMAP)]);
    let undeclared = schema_for_dependency_values_yaml(contract(), "", "");
    let declared = schema_for_dependency_values_yaml(contract(), "global: {}\n", "");
    sim_assert_eq!(have: &undeclared, want: &want);
    sim_assert_eq!(have: &declared, want: &want);
    let cells = [
        (
            "g2: --set global.x=7",
            serde_json::json!({ "global": { "global": {}, "x": 7 } }),
            true,
        ),
        (
            "g4: --set global=5",
            serde_json::json!({ "global": 5 }),
            false,
        ),
        (
            "g5: no default, global: null",
            serde_json::json!({ "global": null }),
            false,
        ),
        (
            "g6: declared default, null deleted",
            serde_json::json!({ "global": { "global": {} } }),
            true,
        ),
    ];
    let have = cells
        .iter()
        .map(|(label, document, _)| (*label, schema_accepts_instance(&want, document)))
        .collect::<Vec<_>>();
    let expected = cells
        .iter()
        .map(|(label, _, renders)| (*label, *renders))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: have, want: expected);
}

/// A dependency's `.Values.global.x` reads its alias-local `kid.global.x`,
/// into which Helm merges the parent's `global.x`: both input paths carry the
/// member contract under the projection's selection guards, and both
/// namespaces stay open to shared members.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn root_and_alias_local_global_members_are_both_admitted() {
    let schema = schema_for_dependency_values_yaml(
        dependency_contract(CONSTANT_CONFIGMAP, &[("kid", REGISTRY_CONSUMER)]),
        indoc! {r#"
            global:
              imageRegistry: ""
            kid:
              global:
                imageRegistry: local
        "#},
        "",
    );
    sim_assert_eq!(have: &schema, want: &serde_json::json!({
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
                                "global": {
                                    "properties": {
                                        "imageRegistry": {
                                            "$ref": "#/$defs/t"
                                        }
                                    },
                                    "required": [
                                        "imageRegistry"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "global": {
                                    "properties": {
                                        "imageRegistry": {
                                            "not": {
                                                "enum": [
                                                    null
                                                ]
                                            }
                                        }
                                    },
                                    "required": [
                                        "imageRegistry"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "global": {
                                    "required": [
                                        "imageRegistry"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {
                            "additionalProperties": {},
                            "properties": {
                                "imageRegistry": {
                                    "type": [
                                        "null",
                                        "string"
                                    ]
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "global": {
                                            "properties": {
                                                "imageRegistry": {
                                                    "$ref": "#/$defs/t"
                                                }
                                            },
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "global": {
                                                        "properties": {
                                                            "imageRegistry": {}
                                                        },
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "global": {
                                                "required": [
                                                    "imageRegistry"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "global"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": {
                                "global": {
                                    "additionalProperties": {},
                                    "properties": {
                                        "imageRegistry": {
                                            "type": [
                                                "null",
                                                "string"
                                            ]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "anyOf": [
                                {
                                    "properties": {
                                        "kid": {
                                            "allOf": [
                                                {
                                                    "type": "object"
                                                },
                                                {
                                                    "anyOf": [
                                                        {
                                                            "not": {
                                                                "properties": {
                                                                    "global": {}
                                                                },
                                                                "required": [
                                                                    "global"
                                                                ],
                                                                "type": "object"
                                                            }
                                                        },
                                                        {
                                                            "properties": {
                                                                "global": {
                                                                    "enum": [
                                                                        null
                                                                    ]
                                                                }
                                                            },
                                                            "required": [
                                                                "global"
                                                            ],
                                                            "type": "object"
                                                        }
                                                    ]
                                                }
                                            ]
                                        }
                                    },
                                    "required": [
                                        "kid"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "kid": {
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "kid"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": {
                                            "kid": {}
                                        },
                                        "required": [
                                            "kid"
                                        ],
                                        "type": "object"
                                    }
                                },
                                {
                                    "properties": {
                                        "kid": {
                                            "enum": [
                                                null
                                            ]
                                        }
                                    },
                                    "required": [
                                        "kid"
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
            "global": {
                "additionalProperties": {},
                "properties": {
                    "imageRegistry": {}
                }
            },
            "kid": {
                "additionalProperties": {},
                "allOf": [
                    {
                        "if": {
                            "allOf": [
                                {
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "global": {}
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "global": {
                                                    "enum": [
                                                        null
                                                    ]
                                                }
                                            },
                                            "required": [
                                                "global"
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
                    "global": {
                        "additionalProperties": {},
                        "properties": {
                            "imageRegistry": {}
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    let verdicts = [
        serde_json::json!({
            "global": { "imageRegistry": "x", "other": 1 },
            "kid": { "global": { "imageRegistry": "x", "other": 1 } }
        }),
        serde_json::json!({
            "global": { "imageRegistry": "x" },
            "kid": { "global": { "imageRegistry": "local" } }
        }),
        serde_json::json!({
            "global": { "imageRegistry": 1 },
            "kid": { "global": { "imageRegistry": 1 } }
        }),
    ]
    .iter()
    .map(|instance| schema_accepts_instance(&schema, instance))
    .collect::<Vec<_>>();
    sim_assert_eq!(have: verdicts, want: vec![true, true, false]);
}
