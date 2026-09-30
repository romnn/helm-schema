use test_util::prelude::sim_assert_eq;

use super::*;
use crate::{AuthoringPolicy, DeclaredTypes, RootPolicy, SchemaProfile};

const STRICT: AuthoringPolicy = AuthoringPolicy {
    root: RootPolicy::Closed,
    declared_types: DeclaredTypes::Assert,
};

const OPEN_ROOT: AuthoringPolicy = AuthoringPolicy {
    root: RootPolicy::Open,
    declared_types: DeclaredTypes::Assert,
};

const ANNOTATE: AuthoringPolicy = AuthoringPolicy {
    root: RootPolicy::Closed,
    declared_types: DeclaredTypes::Annotate,
};

const OPEN_AND_ANNOTATE: AuthoringPolicy = AuthoringPolicy {
    root: RootPolicy::Open,
    declared_types: DeclaredTypes::Annotate,
};

/// redis-ha's shape: declared scalars interpolated into opaque config text,
/// which renders any scalar (the F80 witnesses).
const OPAQUE_CONFIG: &str = indoc! {r"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: haproxy
    data:
      haproxy.cfg: |
        server r0 check inter {{ .Values.haproxy.checkInterval }} fall {{ .Values.haproxy.checkFall }}
        mode {{ .Values.settings.mode }}
        hosts {{ .Values.hosts }}
"};

const OPAQUE_VALUES: &str = indoc! {r"
    haproxy:
      checkInterval: 1s
      checkFall: 1
    settings:
      mode: fast
      retries: 3
    hosts:
      - a
"};

/// Declared values next to real template and resource contracts.
const DEPLOYMENT: &str = indoc! {r#"
    apiVersion: apps/v1
    kind: Deployment
    metadata:
      name: app
      labels:
        {{- toYaml .Values.podLabels | nindent 4 }}
    spec:
      replicas: {{ .Values.replicas }}
      template:
        spec:
          containers:
            - name: app
              image: {{ .Values.image | trunc 63 }}
              ports:
                - containerPort: {{ .Values.probe.port }}
              args:
                - --path={{ .Values.probe.path }}
    {{- if .Values.tls.enabled }}
    ---
    apiVersion: v1
    kind: Secret
    metadata:
      name: tls
    stringData:
      cert: {{ required "tls.cert is required" .Values.tls.cert | quote }}
    {{- end }}
"#};

const DEPLOYMENT_VALUES: &str = indoc! {r#"
    replicas: 1
    image: nginx
    podLabels:
      app: demo
    probe:
      port: 8080
      path: /
    tls:
      enabled: false
      cert: ""
"#};

/// A custom kind with no resource schema: its values are typed by nothing
/// but their declared defaults, directly and inside a guarded branch.
const WIDGET: &str = indoc! {r"
    apiVersion: example.com/v1
    kind: Widget
    metadata:
      name: widget
    spec:
      replicas: {{ .Values.replicas }}
      mode: {{ .Values.settings.mode }}
      hosts: {{ .Values.hosts }}
      {{- if .Values.feature.enabled }}
      size: {{ .Values.feature.size }}
      {{- end }}
"};

const WIDGET_VALUES: &str = indoc! {r"
    replicas: 1
    settings:
      mode: fast
    hosts:
      - a
    feature:
      enabled: false
      size: 5
"};

fn schema_with_policy(
    source: impl SchemaSignalSource,
    values_yaml: &str,
    policy: AuthoringPolicy,
    profile: SchemaProfile,
) -> Value {
    let schema_signals = source.into_schema_signals();
    let documents = prepared_values_documents(Some(values_yaml));
    generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider())
            .with_values_documents(&documents)
            .with_profile(profile)
            .with_authoring_policy(policy),
    )
}

fn verdicts(schema: &Value, instances: &[Value]) -> Vec<bool> {
    instances
        .iter()
        .map(|instance| schema_accepts_instance(schema, instance))
        .collect()
}

/// Declared defaults are the only typing of a custom kind's values: `assert`
/// keeps them, `annotate` drops the scalar type, the item type, and the
/// guarded branch typing they alone supplied. Member-access structure stays.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn annotate_drops_assertions_only_declared_defaults_supply() {
    let strict = schema_with_policy(parse_ir(WIDGET), WIDGET_VALUES, STRICT, SchemaProfile::Full);
    let annotated = schema_with_policy(
        parse_ir(WIDGET),
        WIDGET_VALUES,
        ANNOTATE,
        SchemaProfile::Full,
    );
    sim_assert_eq!(have: &strict, want: &serde_json::json!({
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
                            "not": {
                                "properties": {
                                    "feature": {}
                                },
                                "required": [
                                    "feature"
                                ],
                                "type": "object"
                            }
                        },
                        {
                            "properties": {
                                "feature": {
                                    "enum": [
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "feature"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "not": {
                                "properties": {
                                    "settings": {}
                                },
                                "required": [
                                    "settings"
                                ],
                                "type": "object"
                            }
                        },
                        {
                            "properties": {
                                "settings": {
                                    "enum": [
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "settings"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "feature": {
                "additionalProperties": {},
                "allOf": [
                    {
                        "if": {
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
                        "then": {
                            "additionalProperties": {},
                            "properties": {
                                "size": {
                                    "type": "integer"
                                }
                            }
                        }
                    }
                ],
                "properties": {
                    "enabled": {},
                    "size": {}
                },
                "type": "object"
            },
            "global": {},
            "hosts": {
                "items": {
                    "type": "string"
                },
                "type": "array"
            },
            "replicas": {
                "type": "integer"
            },
            "settings": {
                "additionalProperties": {},
                "properties": {
                    "mode": {
                        "type": "string"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    sim_assert_eq!(have: &annotated, want: &serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "anyOf": [
                        {
                            "not": {
                                "properties": {
                                    "feature": {}
                                },
                                "required": [
                                    "feature"
                                ],
                                "type": "object"
                            }
                        },
                        {
                            "properties": {
                                "feature": {
                                    "enum": [
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "feature"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "not": {
                                "properties": {
                                    "settings": {}
                                },
                                "required": [
                                    "settings"
                                ],
                                "type": "object"
                            }
                        },
                        {
                            "properties": {
                                "settings": {
                                    "enum": [
                                        null
                                    ]
                                }
                            },
                            "required": [
                                "settings"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "feature": {
                "additionalProperties": {},
                "properties": {
                    "enabled": {},
                    "size": {}
                },
                "type": "object"
            },
            "global": {},
            "hosts": {},
            "replicas": {},
            "settings": {
                "additionalProperties": {},
                "properties": {
                    "mode": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    let instances = [
        serde_json::json!({
            "replicas": 1, "settings": { "mode": "fast" }, "hosts": ["a"],
            "feature": { "enabled": false, "size": 5 }
        }),
        serde_json::json!({
            "replicas": "three", "settings": { "mode": 1 }, "hosts": "a",
            "feature": { "enabled": true, "size": "large" }
        }),
    ];
    sim_assert_eq!(
        have: (verdicts(&strict, &instances), verdicts(&annotated, &instances)),
        want: (vec![true, false], vec![true, true])
    );
    let open_only = schema_with_policy(
        parse_ir(WIDGET),
        WIDGET_VALUES,
        OPEN_ROOT,
        SchemaProfile::Full,
    );
    sim_assert_eq!(have: verdicts(&open_only, &instances), want: vec![true, false]);
}

/// Template, resource, guard, and terminal constraints are not declared
/// assertions: a provider-typed replica count, a strict string consumer, a
/// provider string map, a mixed-origin member, and a `required` terminal
/// emit identically under both declared-type policies and both profiles.
#[test]
fn annotate_keeps_every_recovered_constraint() {
    for profile in [SchemaProfile::Full, SchemaProfile::Lean] {
        sim_assert_eq!(
            have: schema_with_policy(parse_ir(DEPLOYMENT), DEPLOYMENT_VALUES, ANNOTATE, profile),
            want: schema_with_policy(parse_ir(DEPLOYMENT), DEPLOYMENT_VALUES, STRICT, profile)
        );
    }
}

fn dependency_schema(policy: AuthoringPolicy) -> Value {
    let mut contract = parse_ir(OPAQUE_CONFIG);
    contract.push_dependency_values_root(undeclared_dependency_root("sub"));
    let dependency = indoc! {"
        sub:
          limit: 5
          nested:
            flag: true
    "};
    let documents = PreparedValuesDocuments::new(
        serde_yaml::from_str(&format!("{OPAQUE_VALUES}{dependency}"))
            .unwrap_or(serde_yaml::Value::Null),
        serde_yaml::from_str(dependency).unwrap_or(serde_yaml::Value::Null),
    );
    let signals = contract.into_schema_signals();
    let descriptions = BTreeMap::from([(
        "haproxy.checkFall".to_string(),
        "Failed checks before a server is marked down.".to_string(),
    )]);
    generate_values_schema(
        ValuesSchemaInput::new(&signals, &provider())
            .with_values_documents(&documents)
            .with_values_descriptions(&descriptions)
            .with_authoring_policy(policy),
    )
}

/// Dependency defaults Helm refills are backfilled as property slots with
/// their descriptions, without their declared types; the root policy is
/// independent of the declared-type policy.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn annotate_backfills_dependency_defaults_as_documentation() {
    sim_assert_eq!(
        have: dependency_schema(ANNOTATE),
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": {
                                        "haproxy": {}
                                    },
                                    "required": [
                                        "haproxy"
                                    ],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": {
                                    "haproxy": {
                                        "enum": [
                                            null
                                        ]
                                    }
                                },
                                "required": [
                                    "haproxy"
                                ],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                },
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": {
                                        "settings": {}
                                    },
                                    "required": [
                                        "settings"
                                    ],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": {
                                    "settings": {
                                        "enum": [
                                            null
                                        ]
                                    }
                                },
                                "required": [
                                    "settings"
                                ],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                }
            ],
            "properties": {
                "global": {},
                "haproxy": {
                    "additionalProperties": {},
                    "properties": {
                        "checkFall": {
                            "description": "Failed checks before a server is marked down."
                        },
                        "checkInterval": {}
                    },
                    "type": "object"
                },
                "hosts": {},
                "settings": {
                    "additionalProperties": {},
                    "properties": {
                        "mode": {}
                    },
                    "type": "object"
                },
                "sub": {
                    "properties": {
                        "global": {},
                        "limit": {},
                        "nested": {
                            "additionalProperties": {},
                            "properties": {
                                "flag": {}
                            }
                        }
                    },
                    "type": "object"
                }
            },
            "type": "object"
        })
    );
    sim_assert_eq!(
        have: dependency_schema(OPEN_AND_ANNOTATE),
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "allOf": [
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": {
                                        "haproxy": {}
                                    },
                                    "required": [
                                        "haproxy"
                                    ],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": {
                                    "haproxy": {
                                        "enum": [
                                            null
                                        ]
                                    }
                                },
                                "required": [
                                    "haproxy"
                                ],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                },
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": {
                                        "settings": {}
                                    },
                                    "required": [
                                        "settings"
                                    ],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": {
                                    "settings": {
                                        "enum": [
                                            null
                                        ]
                                    }
                                },
                                "required": [
                                    "settings"
                                ],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                }
            ],
            "properties": {
                "global": {},
                "haproxy": {
                    "additionalProperties": {},
                    "properties": {
                        "checkFall": {
                            "description": "Failed checks before a server is marked down."
                        },
                        "checkInterval": {}
                    },
                    "type": "object"
                },
                "hosts": {},
                "settings": {
                    "additionalProperties": {},
                    "properties": {
                        "mode": {}
                    },
                    "type": "object"
                },
                "sub": {
                    "properties": {
                        "global": {},
                        "limit": {},
                        "nested": {
                            "additionalProperties": {},
                            "properties": {
                                "flag": {}
                            }
                        }
                    },
                    "type": "object"
                }
            },
            "type": "object"
        })
    );
}

/// `--open-root` removes only the generated root closure: the dependency
/// instance carrier is open either way, and nested provider constraints
/// (the string map behind `podLabels`) keep rejecting, in both profiles.
#[test]
fn open_root_omits_only_the_generated_root_closure() {
    for profile in [SchemaProfile::Full, SchemaProfile::Lean] {
        let strict = schema_with_policy(parse_ir(DEPLOYMENT), DEPLOYMENT_VALUES, STRICT, profile);
        let mut want = strict.clone();
        if let Some(root) = want.as_object_mut() {
            root.remove("additionalProperties");
        }
        let open = schema_with_policy(parse_ir(DEPLOYMENT), DEPLOYMENT_VALUES, OPEN_ROOT, profile);
        sim_assert_eq!(have: &open, want: &want);
        let coalesced = serde_yaml::from_str::<Value>(DEPLOYMENT_VALUES).unwrap_or(Value::Null);
        let mut unknown_key = coalesced.clone();
        unknown_key["ciOnly"] = serde_json::json!(true);
        let mut bad_label = coalesced;
        bad_label["podLabels"] = serde_json::json!({ "app": 1 });
        sim_assert_eq!(
            have: (
                verdicts(&strict, &[unknown_key.clone(), bad_label.clone()]),
                verdicts(&open, &[unknown_key, bad_label]),
            ),
            want: (vec![false, false], vec![true, false])
        );
    }
    let carrier = dependency_schema(OPEN_ROOT);
    sim_assert_eq!(
        have: carrier.pointer("/properties/sub/additionalProperties"),
        want: Some(&serde_json::json!({}))
    );
}
