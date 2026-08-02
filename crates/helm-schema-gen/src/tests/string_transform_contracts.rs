use super::*;
use color_eyre::eyre;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the full expected schema keeps this precedence regression in one auditable scenario"
)]
fn dependency_global_render_uses_follow_the_parent_key_with_a_child_fallback() -> eyre::Result<()> {
    let mut contract = parse_ir(indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          registry: {{ .Values.global.imageRegistry | default "" | b64enc | quote }}
    "#});
    contract.map_value_paths(|path| format!("metrics.{path}"));
    contract.project_dependency_global_contracts(&["metrics".to_string()]);
    let schema = schema_for_values_yaml(contract, None);
    let expected: serde_json::Value = serde_json::from_str(indoc! {r##"
        {
          "$defs": {
            "t": {
              "anyOf": [
                { "const": true },
                { "not": { "const": 0 }, "type": "number" },
                { "minLength": 1, "type": "string" },
                { "minItems": 1, "type": "array" },
                { "minProperties": 1, "type": "object" }
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
                          "imageRegistry": { "$ref": "#/$defs/t" }
                        },
                        "required": ["imageRegistry"],
                        "type": "object"
                      }
                    },
                    "required": ["global"],
                    "type": "object"
                  },
                  {
                    "properties": {
                      "global": {
                        "properties": {
                          "imageRegistry": {
                            "not": { "enum": [null] }
                          }
                        },
                        "required": ["imageRegistry"],
                        "type": "object"
                      }
                    },
                    "required": ["global"],
                    "type": "object"
                  },
                  {
                    "properties": {
                      "global": {
                        "required": ["imageRegistry"],
                        "type": "object"
                      }
                    },
                    "required": ["global"],
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
                        "anyOf": [
                          { "type": "string" },
                          { "type": "null" }
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
                      "metrics": {
                        "properties": {
                          "global": {
                            "properties": {
                              "imageRegistry": { "$ref": "#/$defs/t" }
                            },
                            "required": ["imageRegistry"],
                            "type": "object"
                          }
                        },
                        "required": ["global"],
                        "type": "object"
                      }
                    },
                    "required": ["metrics"],
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
                                  "properties": { "imageRegistry": {} },
                                  "required": ["imageRegistry"],
                                  "type": "object"
                                }
                              },
                              "required": ["global"],
                              "type": "object"
                            }
                          },
                          {
                            "properties": {
                              "global": {
                                "properties": {
                                  "imageRegistry": { "enum": [null] }
                                },
                                "required": ["imageRegistry"],
                                "type": "object"
                              }
                            },
                            "required": ["global"],
                            "type": "object"
                          }
                        ]
                      },
                      {
                        "not": {
                          "properties": {
                            "global": {
                              "required": ["imageRegistry"],
                              "type": "object"
                            }
                          },
                          "required": ["global"],
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
                  "metrics": {
                    "additionalProperties": {},
                    "properties": {
                      "global": {
                        "additionalProperties": {},
                        "properties": {
                          "imageRegistry": {
                            "anyOf": [
                              { "type": "string" },
                              { "type": "null" }
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
                        "not": {
                          "properties": {
                            "metrics": {
                              "properties": { "global": {} },
                              "required": ["global"],
                              "type": "object"
                            }
                          },
                          "required": ["metrics"],
                          "type": "object"
                        }
                      },
                      {
                        "properties": {
                          "metrics": {
                            "properties": {
                              "global": { "enum": [null] }
                            },
                            "required": ["global"],
                            "type": "object"
                          }
                        },
                        "required": ["metrics"],
                        "type": "object"
                      }
                    ]
                  },
                  {
                    "anyOf": [
                      {
                        "not": {
                          "properties": { "metrics": {} },
                          "required": ["metrics"],
                          "type": "object"
                        }
                      },
                      {
                        "properties": {
                          "metrics": { "enum": [null] }
                        },
                        "required": ["metrics"],
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
              "properties": { "imageRegistry": {} }
            },
            "metrics": {
              "additionalProperties": {},
              "allOf": [
                {
                  "if": {
                    "anyOf": [
                      {
                        "not": {
                          "properties": { "global": {} },
                          "required": ["global"],
                          "type": "object"
                        }
                      },
                      {
                        "properties": {
                          "global": { "enum": [null] }
                        },
                        "required": ["global"],
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
                  "properties": { "imageRegistry": {} },
                  "type": "object"
                }
              },
              "type": "object"
            }
          },
          "type": "object"
        }
    "##})?;
    sim_assert_eq!(have: schema, want: expected);

    assert!(schema_accepts_instance(
        &schema,
        &serde_json::json!({
            "global": { "imageRegistry": "registry.example" },
            "metrics": { "global": { "imageRegistry": 7 } }
        })
    ));
    assert!(!schema_accepts_instance(
        &schema,
        &serde_json::json!({
            "global": {},
            "metrics": { "global": { "imageRegistry": 7 } }
        })
    ));
    assert!(!schema_accepts_instance(
        &schema,
        &serde_json::json!({
            "global": { "imageRegistry": 7 },
            "metrics": { "global": { "imageRegistry": "registry.child" } }
        })
    ));
    Ok(())
}

#[test]
fn shadowed_dependency_global_default_does_not_type_ignored_input() {
    let mut contract = ContractIr::default();
    contract.add_type_hint("metrics.agent.replicas", "integer");
    let schema_signals = contract.finalize().into_schema_signals();
    let shadowed =
        std::collections::BTreeSet::from(["metrics.agent.global.imageRegistry".to_string()]);
    let schema = generate_values_schema(
        ValuesSchemaInput::new(&schema_signals, &provider())
            .with_values_yaml(Some(indoc! {"
                metrics:
                  agent:
                    replicas: 1
                    global:
                      imageRegistry: []
            "}))
            .with_shadowed_input_paths(&shadowed),
    );

    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "metrics": {
                    "additionalProperties": {},
                    "properties": {
                        "agent": {
                            "additionalProperties": {},
                            "properties": {
                                "replicas": {
                                    "type": "integer"
                                }
                            },
                            "type": "object"
                        }
                    },
                    "type": "object"
                }
            },
            "type": "object"
        })
    );
}

/// A total stringification is neutral evidence about its own input; an
/// INDEPENDENT unconditional string consumer still binds. Cilium's
/// `cluster.name` is quoted into the configmap, but `replace` also consumes
/// it in validation logic — a map value fails `helm template` there.
#[test]
fn stringified_use_keeps_unconditional_string_transform_contract() {
    let src = indoc! {r#"
        {{- if gt (len (.Values.cluster.name | replace "-" "")) 30 }}
        {{- fail "cluster name too long" }}
        {{- end }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          cluster-name: {{ .Values.cluster.name | quote }}
    "#};
    let values_yaml = indoc! {"
        cluster:
          name: default
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "cluster": { "name": "prod" } })
        ),
        "string cluster names render: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "cluster": { "name": { "bad": true } } })
        ),
        "replace consumes the raw name, so a map fails rendering and must be rejected: {schema}"
    );
}

/// Mutually exclusive guarded uses lower their own domains under their own
/// conditions (falco's `rolearn`): the quote branch renders anything, the
/// b64enc branch fails rendering for non-strings.
#[test]
fn quote_branch_does_not_erase_b64enc_branch_contract() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if .Values.aws.useirsa }}
          role-arn: {{ .Values.aws.rolearn | quote }}
          {{- else }}
          AWS_ROLEARN: "{{ .Values.aws.rolearn | b64enc }}"
          {{- end }}
    "#};
    let values_yaml = indoc! {r#"
        aws:
          useirsa: true
          rolearn: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // The b64enc contract rides its own row's condition: it binds only
    // where that branch renders. In the quote branch the same map renders
    // fine (Helm prints it as text).
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "aws": { "useirsa": true, "rolearn": { "bad": true } } })
        ),
        "the quote branch renders any value: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "aws": { "useirsa": false, "rolearn": { "bad": true } } })
        ),
        "the b64enc branch rejects non-strings: {schema}"
    );
    for useirsa in [true, false] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({ "aws": { "useirsa": useirsa, "rolearn": "arn:aws:iam::1:role/x" } })
            ),
            "strings render in both branches (useirsa={useirsa}): {schema}"
        );
    }
}

/// A `join` occurrence proves nothing about OTHER occurrences: sealed-secrets
/// also `range`s `additionalNamespaces` under its namespaced-roles flag, and
/// a scalar fails that render (`range can\'t iterate over ns-a`).
#[test]
fn join_use_does_not_erase_range_branch() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          {{- if .Values.additionalNamespaces }}
          namespaces: {{ join "," .Values.additionalNamespaces | quote }}
          {{- end }}
        {{- if .Values.rbac.namespacedRoles }}
        {{- range .Values.additionalNamespaces }}
        ---
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: role-{{ . }}
        {{- end }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalNamespaces: []
        rbac:
          namespacedRoles: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // `.Values.rbac.namespacedRoles` is navigated on every render, so the
    // composed document keeps the `rbac` host.
    assert!(
        schema_accepts_instance(
            &schema,
            &composed_instance(
                values_yaml,
                serde_json::json!({ "additionalNamespaces": "ns-a" })
            )
        ),
        "with namespaced roles off, only the join renders and a scalar is fine: {schema}"
    );
    for namespaces in [
        serde_json::json!(["ns-a"]),
        serde_json::json!({ "a": "ns-a" }),
    ] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "rbac": { "namespacedRoles": true },
                    "additionalNamespaces": namespaces
                })
            ),
            "range iterates lists and maps: {schema}"
        );
    }
    // `range` cannot iterate a string, so `namespacedRoles=true` plus a
    // string fails `helm template` and the guarded iterable domain rejects
    // the combination.
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "namespacedRoles": true },
                "additionalNamespaces": "ns-a"
            })
        ),
        "inside the ranged branch a string cannot iterate: {schema}"
    );
    // Integer counts iterate (Helm's `--set` channel delivers int64; a
    // JSON Schema cannot separate that from the failing values-file
    // float64 spelling, so the renderable channel wins); non-integral
    // numbers fail in every channel.
    for count in [2, 0, -1] {
        assert!(
            schema_accepts_instance(
                &schema,
                &serde_json::json!({
                    "rbac": { "namespacedRoles": true },
                    "additionalNamespaces": count
                })
            ),
            "range iterates integer counts: {schema}"
        );
    }
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({
                "rbac": { "namespacedRoles": true },
                "additionalNamespaces": 2.5
            })
        ),
        "non-integral numbers cannot iterate: {schema}"
    );
    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "rbac": { "namespacedRoles": true } })
        ),
        "an absent collection ranges zero times and renders: {schema}"
    );
}

/// printf's format parameter is a real Go `string`: NFS provisioner calls
/// `printf .Values.storageClass.provisionerName`, and a non-string value
/// fails template evaluation (`wrong type for value; expected string`).
#[test]
fn dynamic_printf_format_requires_string() {
    let src = indoc! {r"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: {{ printf .Values.storageClass.provisionerName }}
    "};
    let values_yaml = indoc! {"
        storageClass:
          provisionerName: cluster.local/provisioner
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    assert!(
        schema_accepts_instance(
            &schema,
            &serde_json::json!({ "storageClass": { "provisionerName": "x/y" } })
        ),
        "string formats evaluate: {schema}"
    );
    assert!(
        !schema_accepts_instance(
            &schema,
            &serde_json::json!({ "storageClass": { "provisionerName": 7 } })
        ),
        "a non-string printf format fails template evaluation and must be rejected: {schema}"
    );
}

/// Case mapping has two independent effects at an unquoted YAML slot: its
/// operand must be a Go string, and token-breaking characters survive the
/// mapping unchanged. Keeping only the former loses the sink's lexical
/// language (kube-state-metrics' probe schemes).
#[test]
fn case_mapping_keeps_the_plain_slot_language_beside_its_string_contract() {
    let src = indoc! {r"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: test
              livenessProbe:
                httpGet:
                  path: /
                  port: 8080
                  scheme: {{ upper .Values.scheme }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("scheme: http\n"));

    for value in ["http", "HTTPS"] {
        let instance = serde_json::json!({ "scheme": value });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "ordinary string schemes render: instance={instance}; schema={schema}"
        );
    }
    let multiline = ['a', '\n', 'b'].into_iter().collect::<String>();
    for value in ["a: b", "a #b", multiline.as_str(), "&anchor"] {
        let instance = serde_json::json!({ "scheme": value });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "case mapping preserves token-breaking text: instance={instance}; schema={schema}"
        );
    }
    let non_string = serde_json::json!({ "scheme": 7 });
    assert!(
        !schema_accepts_instance(&schema, &non_string),
        "upper still requires a Go string: schema={schema}"
    );
}

/// Helm's `lookup` consumes four strings but returns external cluster state;
/// its result is not any argument's identity. A literal `default` therefore
/// keeps every falsy source spelling out of the strict argument lane
/// (Cilium's configurable cluster-info name and namespace).
#[test]
fn lookup_argument_contract_preserves_a_literal_defaults_falsy_escape() {
    let helpers = indoc! {r#"
        {{- define "lookup-name" -}}
        {{- $name := default "cluster-info" .Values.name -}}
        {{- $configmap := lookup "v1" "ConfigMap" "default" $name -}}
        {{- if $configmap -}}
        {{- get $configmap.data "value" -}}
        {{- else -}}
        fallback
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          value: {{ include "lookup-name" . | quote }}
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some("name: \"\"\n"));

    for value in [
        serde_json::json!("custom"),
        serde_json::json!(""),
        serde_json::json!(false),
        serde_json::json!(0),
        serde_json::json!([]),
        serde_json::json!({}),
        serde_json::json!(null),
    ] {
        let instance = serde_json::json!({ "name": value });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "lookup receives the literal fallback for every falsy source: instance={instance}; schema={schema}"
        );
    }
    for value in [
        serde_json::json!([1]),
        serde_json::json!({ "bad": true }),
        serde_json::json!(7),
        serde_json::json!(true),
    ] {
        let instance = serde_json::json!({ "name": value });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "truthy non-strings reach lookup and abort: instance={instance}; schema={schema}"
        );
    }
}

/// printf's data parameters render through any verb (Go fmt embeds
/// mismatches in the output): airflow formats `dags.gitSync.subPath` with a
/// literal format and Helm renders `subPath: 7` as `%!s(int64=7)`.
#[test]
fn printf_data_argument_accepts_any_value_through_helper_sink() {
    let helpers = indoc! {r#"
        {{- define "airflow_dags" -}}
        {{- printf "%s/dags/repo/%s" .Values.airflowHome .Values.dags.gitSync.subPath -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: config
        data:
          dags_folder: {{ include "airflow_dags" . }}
    "#};
    let values_yaml = indoc! {r#"
        airflowHome: /opt/airflow
        dags:
          gitSync:
            subPath: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for sub_path in [
        serde_json::json!("repo/dags"),
        serde_json::json!(7),
        serde_json::json!(null),
    ] {
        let instance = serde_json::json!({
            "airflowHome": "/opt/airflow",
            "dags": { "gitSync": { "subPath": sub_path } }
        });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "printf data arguments render any value: instance={instance}; schema={schema}"
        );
    }
}

/// `%s` is total as a formatter argument, but when its diagnostic output
/// opens an unquoted YAML token a non-string or missing value corrupts that
/// token. The same formatter inside explicit quotes remains total
/// (Sealed Secrets' unquoted image registry).
#[test]
fn token_initial_printf_string_argument_uses_the_plain_slot_language() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
          annotations:
            quoted: "{{ printf "%s/suffix" .Values.quoted }}"
            piped: {{ printf "%s/suffix" .Values.piped | quote }}
        spec:
          containers:
            - name: test
              image: {{ printf "%s/repository:tag" .Values.registry }}
    "#};
    let values_yaml = indoc! {"
        registry: registry.example.com
        quoted: value
        piped: value
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for registry in [
        serde_json::json!("registry.example.com"),
        serde_json::json!("true"),
        serde_json::json!("&anchor"),
    ] {
        let instance = serde_json::json!({ "registry": registry, "quoted": 7, "piped": ["any"] });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "a valid leading string and quoted diagnostics both render: \
             instance={instance}; schema={schema}"
        );
    }
    for registry in [
        serde_json::json!(7),
        serde_json::json!(false),
        serde_json::json!([]),
        serde_json::json!(null),
        serde_json::json!("a: b"),
    ] {
        let instance =
            serde_json::json!({ "registry": registry, "quoted": "value", "piped": "value" });
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "a token-opening %s must receive structurally safe string text: \
             instance={instance}; schema={schema}"
        );
    }
    assert!(
        !schema_accepts_instance(&schema, &serde_json::json!({ "quoted": "value" })),
        "a missing token-opening argument renders an invalid fmt diagnostic: {schema}"
    );
}

/// A helper-local fallback keeps the formatter contract on the arm that
/// actually supplies its token-opening `%s`. A dormant fallback remains
/// open even though the same helper rejects it when selected (Airflow's
/// image repository selection).
#[test]
fn helper_printf_keeps_its_selected_token_initial_argument() {
    let helpers = indoc! {r#"
        {{- define "image" -}}
        {{- $repository := .Values.primary | default .Values.fallback -}}
        {{- printf "%s:tag" $repository -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: {{ include "image" . }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            primary: ''
            fallback: repository
        "}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({ "primary": "", "fallback": "repository" }),
            true,
            "the fallback string is selected",
        ),
        (
            serde_json::json!({ "primary": "", "fallback": "a #b" }),
            true,
            "a selected comment leaves an ordinary string prefix",
        ),
        (
            serde_json::json!({ "primary": "", "fallback": "true #b" }),
            false,
            "a selected comment leaves a Boolean prefix",
        ),
        (
            serde_json::json!({ "primary": "", "fallback": {} }),
            true,
            "a selected empty mapping formats as plain map text",
        ),
        (
            serde_json::json!({ "primary": "", "fallback": { "key": {} } }),
            true,
            "a selected bounded mapping formats as plain map text",
        ),
        (
            serde_json::json!({ "primary": "", "fallback": { "a: b": {} } }),
            false,
            "a mapping key can still break the formatted token",
        ),
        (
            serde_json::json!({ "primary": "repository", "fallback": 7 }),
            true,
            "a live primary leaves the fallback dormant",
        ),
        (
            serde_json::json!({ "primary": "", "fallback": 7 }),
            false,
            "a selected numeric fallback emits an invalid diagnostic",
        ),
        (
            serde_json::json!({ "primary": "" }),
            false,
            "a missing selected fallback emits an invalid diagnostic",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "selected formatter arm ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

#[test]
fn helper_prefix_branch_scopes_the_token_opening_formatter_argument() {
    let helpers = indoc! {r#"
        {{- define "image" -}}
        {{- $registry := default .Values.image.registry .Values.global.registry -}}
        {{- $repository := .Values.image.repository -}}
        {{- if $registry -}}
          {{- printf "%s/%s:tag" $registry $repository -}}
        {{- else -}}
          {{- printf "%s:tag" $repository -}}
        {{- end -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: Pod
        metadata:
          name: test
        spec:
          containers:
            - name: test
              image: {{ include "image" . }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {"
            global:
              registry: docker.io
            image:
              registry: ''
              repository: repo
        "}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({
                "global": { "registry": "docker.io" },
                "image": { "registry": 7, "repository": 7 }
            }),
            true,
            "the global prefix keeps both local operands away from token opening",
        ),
        (
            serde_json::json!({
                "global": { "registry": "" },
                "image": { "registry": "", "repository": 7 }
            }),
            false,
            "the prefix-free branch makes repository token-opening",
        ),
        (
            serde_json::json!({
                "global": { "registry": "" },
                "image": { "registry": 7, "repository": "repo" }
            }),
            false,
            "the selected local registry opens the prefixed branch",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "formatter branch ({label}): instance={instance}; want={want}; schema={schema}"
        );
    }
}

#[test]
fn quoting_helper_printf_output_clears_its_plain_slot_contract() {
    let helpers = indoc! {r#"
        {{- define "image" -}}
        {{- printf "%s:tag" .Values.repository -}}
        {{- end -}}
    "#};
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: {{ include "image" . | quote }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some("repository: example.com/repository\n"),
    );

    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "repository": {}
            },
            "type": "object"
        })
    );
}

/// Chart repro (sealed-secrets `additionalNamespaces`): a declared-list
/// value joined under a self-truthy guard renders map and scalar values
/// through Sprig's singleton fallback, so the declared array type must not
/// reject them.
#[test]
fn self_guarded_join_of_declared_list_accepts_any_input() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: controller
                  args:
                    {{- if .Values.additionalNamespaces }}
                    - --additional-namespaces
                    - {{ join "," .Values.additionalNamespaces | quote }}
                    {{- end }}
    "#};
    let values_yaml = indoc! {"
        additionalNamespaces: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for probe in [
        serde_json::json!(["ns-a", "ns-b"]),
        serde_json::json!("ns-a"),
        serde_json::json!({ "k": "v" }),
    ] {
        let instance = serde_json::json!({ "additionalNamespaces": probe });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "strslice converts any joined input: instance={instance}; schema={schema}"
        );
    }
}

/// Chart repro (grafana `sidecar.alerts.skipTlsVerify`): an undeclared
/// value quoted into a typed string sink (`env[].value`) under a `with`
/// guard renders any type, so the sink typing must not flow back through the
/// stringification.
#[test]
fn with_guarded_quote_into_string_sink_accepts_any_input() {
    let src = indoc! {r"
        apiVersion: apps/v1
        kind: Deployment
        spec:
          template:
            spec:
              containers:
                - name: sidecar
                  env:
                    {{- with .Values.sidecar.skipTlsVerify }}
                    - name: SKIP_TLS_VERIFY
                      value: {{ quote . }}
                    {{- end }}
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some("sidecar: {}\n"));

    for probe in [
        serde_json::json!(true),
        serde_json::json!("true"),
        serde_json::json!({ "k": "v" }),
        serde_json::json!([1, 2]),
    ] {
        let instance = serde_json::json!({ "sidecar": { "skipTlsVerify": probe } });
        assert!(
            schema_accepts_instance(&schema, &instance),
            "quote erases input shape at the env sink: instance={instance}; schema={schema}"
        );
    }
}

/// `htpasswd` bcrypt-hashes two Go strings, so a non-string member value
/// aborts rendering — including through a destructured range and a helper
/// include (prometheus-pushgateway's `basicAuthUsers`).
#[test]
fn htpasswd_operands_require_strings() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          direct: {{ htpasswd "" .Values.adminPassword | quote }}
          config: |
            {{- include "repro.webConfiguration" . | nindent 4 }}
    "#};
    let helpers = indoc! {r#"
        {{- define "repro.webConfiguration" -}}
        basic_auth_users:
        {{- range $k, $v := .Values.basicAuthUsers }}
          {{ $k }}: {{ htpasswd "" $v | trimPrefix ":" }}
        {{- end }}
        {{- end -}}
    "#};
    let values_yaml = indoc! {"
        adminPassword: hunter2
        basicAuthUsers: {}
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    // Cases compose over the declared defaults: the direct `htpasswd` reads
    // `adminPassword` on every render and aborts on a nil operand.
    for (overrides, want) in [
        (serde_json::json!({ "adminPassword": 7 }), false),
        (serde_json::json!({ "adminPassword": "ok" }), true),
        (
            serde_json::json!({ "basicAuthUsers": { "admin": 7 } }),
            false,
        ),
        (
            serde_json::json!({ "basicAuthUsers": { "admin": { "bad": 1 } } }),
            false,
        ),
        (
            serde_json::json!({ "basicAuthUsers": { "admin": "hunter2" } }),
            true,
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "htpasswd consumes Go strings only: instance={instance}; schema={schema}"
        );
    }
}

/// Sprig's checksum family hashes a typed Go string, so a truthy non-string
/// reaching `sha256sum` aborts rendering — including a ranged member picked
/// through a local `default ""` selection, where only the truthy lane hashes
/// and every falsy spelling escapes to `nopass` (bitnami-redis' ACL users).
#[test]
fn checksum_operands_require_strings_through_ranged_default_selection() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          direct: {{ sha256sum .Values.seed | quote }}
          users.acl: |-
            {{- range .Values.users }}
            {{- $password := .password | default "" }}
            user {{ .username }} {{ if $password }}#{{ sha256sum $password }}{{ else }}nopass{{ end }}
            {{- end }}
    "#};
    let values_yaml = indoc! {"
        seed: audit
        users: []
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // Cases compose over the declared defaults: the direct `sha256sum` reads
    // `seed` on every render and aborts on a nil operand.
    for (overrides, want, label) in [
        (serde_json::json!({ "seed": 7 }), false, "direct numeric"),
        (serde_json::json!({ "seed": "ok" }), true, "direct string"),
        (
            serde_json::json!({ "users": [{ "username": "u", "password": 7 }] }),
            false,
            "truthy numeric member",
        ),
        (
            serde_json::json!({ "users": [{ "username": "u", "password": "s3cret" }] }),
            true,
            "string member",
        ),
        (
            serde_json::json!({ "users": [{ "username": "u" }] }),
            true,
            "absent member selects nopass",
        ),
        (
            serde_json::json!({ "users": [{ "username": "u", "password": 0 }] }),
            true,
            "falsy member escapes the hash",
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "checksum operand {label}: instance={instance}; schema={schema}"
        );
    }
}

/// The full bitnami-redis ACL shape: the whole document rides an
/// include-result gate (`if (include "redis.createConfigmap" .)`), which
/// decodes through the helper's literal dispatch (`{{- true -}}` under
/// `empty .Values.existingConfigmap`) instead of degrading to an
/// undecodable marker that would drop the member capture; the secret lane
/// and the default-user hash ride includes with no values identity.
#[test]
fn checksum_member_contract_survives_include_result_document_gate() {
    let src = indoc! {r#"
        {{- if (include "redis.createConfigmap" .) }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          users.acl: |-
            {{- if .Values.auth.acl.enabled}}
            {{- $password := include "redis.password" . }}
            user default on {{ if $password}}#{{ sha256sum $password}}{{ else }}nopass{{ end }} ~* &* +@all
            {{- if .Values.auth.acl.users -}}
            {{- $userSecret := .Values.auth.acl.userSecret -}}
            {{- range .Values.auth.acl.users }}
            {{- $userPassword := .password | default "" }}
            {{- if $userSecret }}
            {{- $secretPassword := include "common.secrets.get" (dict "secret" $userSecret "key" .username "context" $) }}
            user {{ .username }} {{ default "on" .enabled }} {{ if $secretPassword }}#{{ sha256sum $secretPassword }}{{ else }}nopass{{ end }} {{ default "~*" .keys }}
            {{- else }}
            user {{ .username }} {{ default "on" .enabled }} {{ if $userPassword }}#{{ sha256sum $userPassword }}{{ else }}nopass{{ end }} {{ default "~*" .keys }}
            {{- end }}
            {{- end }}
            {{- end }}
            {{- end }}
        {{- end }}
    "#};
    let helpers = indoc! {r#"
        {{- define "redis.createConfigmap" -}}
        {{- if empty .Values.existingConfigmap }}
            {{- true -}}
        {{- end -}}
        {{- end -}}
        {{- define "redis.password" -}}
        {{- .Values.auth.password -}}
        {{- end -}}
        {{- define "common.secrets.get" -}}
        secret
        {{- end -}}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir_with_helpers(src, helpers),
        Some(indoc! {r#"
            existingConfigmap: ""
            auth:
              password: ""
              acl:
                enabled: false
                users: []
                userSecret: ""
        "#}),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({
                "auth": { "acl": { "enabled": true, "users": [{ "username": "u", "password": 7 }] } }
            }),
            false,
            "numeric password under the live gate",
        ),
        (
            serde_json::json!({
                "auth": { "acl": { "enabled": true, "users": [{ "username": "u", "password": "ok" }] } }
            }),
            true,
            "string password",
        ),
        (
            serde_json::json!({
                "existingConfigmap": "external",
                "auth": { "acl": { "enabled": true, "users": [{ "username": "u", "password": 7 }] } }
            }),
            true,
            "numeric password behind the dead include gate",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "include-gated checksum member {label}: instance={instance}; schema={schema}"
        );
    }
}

/// The checksum contract survives OUTER branch guards around the range: the
/// selection's per-member truthiness cannot become a root guard, so it scopes
/// the member requirement to truthy values instead, and the enclosing `if`
/// chain lowers as the implication's outer guards (bitnami-redis nests the
/// ACL users range under `acl.enabled` and `acl.users`).
#[test]
fn checksum_member_contract_survives_outer_branch_guards() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          users.acl: |-
            {{- if .Values.auth.acl.enabled}}
            {{- if .Values.auth.acl.users -}}
            {{- $userSecret := .Values.auth.acl.userSecret -}}
            {{- range .Values.auth.acl.users }}
            {{- $userPassword := .password | default "" }}
            {{- if $userSecret }}
            user {{ .username }} secretlane
            {{- else }}
            user {{ .username }} {{ default "on" .enabled }} {{ if $userPassword }}#{{ sha256sum $userPassword }}{{ else }}nopass{{ end }} {{ default "~*" .keys }}
            {{- end }}
            {{- end }}
            {{- end }}
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {r#"
            auth:
              acl:
                enabled: false
                users: []
                userSecret: ""
        "#}),
    );

    for (instance, want, label) in [
        (
            serde_json::json!({
                "auth": { "acl": { "enabled": true, "users": [{ "username": "u", "password": 7 }] } }
            }),
            false,
            "numeric password under live guards",
        ),
        (
            serde_json::json!({
                "auth": { "acl": { "enabled": true, "users": [{ "username": "u", "password": "ok" }] } }
            }),
            true,
            "string password under live guards",
        ),
        (
            serde_json::json!({
                "auth": { "acl": { "enabled": true, "users": [{ "username": "u", "password": 0 }] } }
            }),
            true,
            "falsy password escapes to nopass",
        ),
        (
            serde_json::json!({
                "auth": { "acl": { "enabled": false, "users": [{ "username": "u", "password": 7 }] } }
            }),
            true,
            "numeric password in the dead arm",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "guarded checksum member {label}: instance={instance}; schema={schema}"
        );
    }
}

/// A direct `tpl` program input keeps its Go string contract through a
/// `default` selection chain: `tpl` parses the RAW value before any
/// truthiness selection runs, so a map aborts rendering even when its
/// Helm-falsy spelling would select a later arm (oauth2-proxy's
/// `tpl .Values.image.registry $ | default (tpl .Values.global.imageRegistry $) | default "quay.io"`).
#[test]
fn tpl_program_contract_survives_default_chain() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          image: "{{ tpl .Values.image.registry $ | default (tpl .Values.global.imageRegistry $) | default "quay.io" }}/proxy"
    "#};
    let values_yaml = indoc! {r#"
        image:
          registry: ""
        global:
          imageRegistry: ""
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    // Cases compose over the declared defaults: both `image` and `global`
    // are navigated on every render.
    for (overrides, want) in [
        (serde_json::json!({ "image": { "registry": {} } }), false),
        (serde_json::json!({ "image": { "registry": ["x"] } }), false),
        (
            serde_json::json!({ "image": { "registry": "quay.io" } }),
            true,
        ),
        (serde_json::json!({ "image": { "registry": "" } }), true),
        // The eagerly evaluated fallback arm parses its own program too
        (
            serde_json::json!({ "global": { "imageRegistry": {} } }),
            false,
        ),
        (
            serde_json::json!({ "global": { "imageRegistry": "ghcr.io" } }),
            true,
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "tpl parses raw program text before default selection: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A string transform reached only after the operand's own truthiness check
/// constrains the live arm, not the falsy fallback arm.
#[test]
fn self_guarded_string_transform_keeps_every_falsy_spelling() {
    let helpers = indoc! {r#"
        {{- define "repro.rawName" -}}
        {{- .Values.nameOverride | trunc 63 | trimSuffix "-" }}
        {{- end }}
        {{- define "repro.name" -}}
        {{- if .Values.nameOverride }}
        {{- include "repro.rawName" . }}
        {{- else }}
        fallback
        {{- end }}
        {{- end }}
        {{- define "repro.addr" -}}
        {{- with .Values.redis }}
        {{- ternary (printf "%s:6379" (include "repro.name" $)) .external (eq .type "internal") }}
        {{- end }}
        {{- end }}
    "#};
    let src = indoc! {r#"
        {{- if .Values.enabled }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: repro
        data:
          endpoint: {{ include "repro.addr" . | quote }}
        {{- end }}
    "#};
    let values_yaml = indoc! {"
        enabled: true
        nameOverride: ''
        redis:
          type: internal
          external: redis.example.com
    "};
    let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));

    for (value, want, label) in [
        (
            serde_json::json!({}),
            true,
            "an empty mapping takes the fallback",
        ),
        (
            serde_json::json!([]),
            true,
            "an empty list takes the fallback",
        ),
        (serde_json::json!(null), true, "null takes the fallback"),
        (
            serde_json::json!("custom"),
            true,
            "a string reaches the transform",
        ),
        (
            serde_json::json!({ "member": "value" }),
            false,
            "a truthy mapping reaches the string transform",
        ),
    ] {
        let instance = composed_instance(
            values_yaml,
            serde_json::json!({ "enabled": true, "nameOverride": value }),
        );
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; schema={schema}"
        );
    }
}

/// tempo's jaeger receivers: `regexSplit ":" . -1 | last` extracts the
/// port suffix of an endpoint string into a Service port slot, so the
/// accepted endpoints are strings whose LAST `:`-segment is numeric.
#[test]
fn split_last_segment_into_numeric_slot_requires_numeric_suffix() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: Service
        metadata:
          name: test
        spec:
          ports:
            {{- with .Values.endpoint }}
            - name: grpc
              port: {{ regexSplit ":" . -1 | last }}
              protocol: TCP
            {{- end }}
    "#};
    let schema = schema_for_values_yaml(parse_ir(src), Some("endpoint: ~\n"));
    for (instance, want) in [
        (serde_json::json!({ "endpoint": "0.0.0.0:audit" }), false),
        (serde_json::json!({ "endpoint": "0.0.0.0:14250" }), true),
        (serde_json::json!({ "endpoint": null }), true),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "the endpoint's port suffix feeds an integer slot: \
             instance={instance}; schema={schema}"
        );
    }
}

/// The datadog migration shape: a raw values string is checksummed into an
/// annotation (`userValues | sha256sum`) and spliced verbatim into a block
/// scalar. The annotation slot observes the DIGEST — a plain token for any
/// operand — so the slot's plain-scalar language must not project backward
/// onto the operand: YAML-looking and multiline file contents stay
/// accepted while the checksum's own strict-string contract still rejects
/// non-strings (helm aborts hashing a map or number).
#[test]
fn checksum_digest_splices_project_no_slot_language_onto_the_operand() {
    let src = indoc! {r"
        {{- if or .Values.migration.enabled .Values.migration.preview }}
        {{- if .Values.migration.userValues }}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
          annotations:
            checksum/migration-config: {{ .Values.migration.userValues | sha256sum }}
        data:
          values.yaml: |-
        {{ .Values.migration.userValues | indent 4 }}
        {{- end }}
        {{- end }}
    "};
    let schema = schema_for_values_yaml(
        parse_ir(src),
        Some(indoc! {"
            migration:
              enabled: false
              preview: false
              userValues: null
        "}),
    );
    for (instance, want, label) in [
        (
            serde_json::json!({ "migration": { "enabled": true, "userValues": "datadog: {}" } }),
            true,
            "single-line YAML file content renders",
        ),
        (
            serde_json::json!({ "migration": { "enabled": true, "userValues": indoc! {"
                datadog:
                  apiKey: x
            "} } }),
            true,
            "multiline YAML file content renders",
        ),
        (
            serde_json::json!({ "migration": { "enabled": true, "userValues": "plain" } }),
            true,
            "plain text renders",
        ),
        (
            serde_json::json!({ "migration": { "enabled": true, "userValues": { "a": 1 } } }),
            false,
            "a live map operand aborts the checksum",
        ),
        (
            serde_json::json!({ "migration": { "enabled": true, "userValues": 7 } }),
            false,
            "a live number operand aborts the checksum",
        ),
        (
            serde_json::json!({ "migration": { "userValues": { "a": 1 } } }),
            true,
            "the dormant gate keeps junk open",
        ),
    ] {
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "checksum operand slot-language abstention ({label}): \
             instance={instance}; want={want}; schema={schema}"
        );
    }
}

/// Every nil-strict string consumer — `tpl`, `b64enc`, `trim`, `trunc`,
/// `htpasswd`, and the rest of the transform catalog — reads its operand as
/// a Go string, so a NIL operand aborts rendering ("wrong type for value;
/// expected string") wherever the consumer runs. Absence is Helm-falsy, so
/// the truthy⇒string capture cannot state that; the presence claim is its
/// own abort-grade clause, scoped by the consumer's ambient guards and
/// exempt where the operand's own truthiness gates the read.
#[test]
fn nil_strict_string_consumers_require_their_operand_to_exist() {
    let src = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: test
        data:
          {{- if .Values.assert }}
          secret: {{ tpl .Values.config.secret $ | b64enc }}
          {{- end }}
          plain: {{ .Values.name | trim }}
          {{- with .Values.guarded }}
          guarded: {{ tpl . $ }}
          {{- end }}
          derived: {{ printf "%s-%s" .Values.name .Values.suffix | trunc 63 }}
    "#};
    let values_yaml = indoc! {"
        assert: true
        config:
          secret: value
        name: chart
        guarded: text
        suffix: x
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for (label, overrides, want) in [
        ("baseline", serde_json::json!({}), true),
        (
            "the live gate's operand must exist",
            serde_json::json!({ "config": { "secret": null } }),
            false,
        ),
        (
            "a dormant gate keeps the deletion open",
            serde_json::json!({ "assert": false, "config": { "secret": null } }),
            true,
        ),
        (
            "an unconditional consumer demands its operand outright",
            serde_json::json!({ "name": null }),
            false,
        ),
        (
            "a with-scoped operand renders when absent",
            serde_json::json!({ "guarded": null }),
            true,
        ),
        (
            // `printf` renders `%!s(<nil>)` for a missing operand, and only
            // its DERIVED text reaches the trim.
            "a derived operand claims nothing about its influences",
            serde_json::json!({ "suffix": null }),
            true,
        ),
    ] {
        let instance = composed_instance(values_yaml, overrides);
        assert!(
            schema_accepts_instance(&schema, &instance) == want,
            "{label}: instance={instance}; want={want}; schema={schema}"
        );
    }
}

#[test]
fn strict_tpl_in_composed_scalar_keeps_its_input_type() {
    let src = indoc! {r#"
        apiVersion: apps/v1
        {{- if .Values.server.stateful }}
        kind: StatefulSet
        {{- else if .Values.server.daemon }}
        kind: DaemonSet
        {{- else }}
        kind: Deployment
        {{- end }}
        metadata:
          name: test
        spec:
          selector:
            matchLabels:
              app: test
          template:
            metadata:
              labels:
                app: test
            spec:
              containers:
                - name: main
                {{- if .Values.image.digest }}
                  image: "{{ tpl .Values.image.repository . }}@{{ tpl .Values.image.digest . }}"
                {{- else }}
                  image: "{{ tpl .Values.image.repository . }}:{{ tpl .Values.image.tag . | default .Chart.AppVersion }}{{ if .Values.image.distroless }}-distroless{{ end }}"
                {{- end }}
    "#};
    let values_yaml = indoc! {"
        image:
          repository: example/image
          tag: ''
          digest: ''
          distroless: false
        server:
          stateful: false
          daemon: false
    "};
    let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));

    for repository in [serde_json::json!({}), serde_json::json!([])] {
        let instance = composed_instance(
            values_yaml,
            serde_json::json!({ "image": { "repository": repository } }),
        );
        assert!(
            !schema_accepts_instance(&schema, &instance),
            "tpl requires a string even when sibling tag/digest guards are falsy: \
             instance={instance}; schema={schema}"
        );
    }
}

/// A raw splice in an UNQUOTED slot renders the value's own characters, so text
/// that ends the plain token there corrupts the document. Two identities reach
/// such a slot and both now carry the claim: a directly ranged collection's KEY
/// (bare, and through a `replace` whose token cannot change the token-ending
/// characters), and a `tpl` operand — `tpl` is the identity on
/// template-ACTION-free input, so a value carrying `{{` escapes.
#[test]
fn unquoted_slots_bound_the_lexical_language_of_their_source() {
    let key_in_value_slot = indoc! {r#"
        env:
        {{- range $key, $value := .Values.extraEnvVars }}
          - name: {{ $key | replace "." "_" }}
            value: {{ $value | quote }}
        {{- end }}
    "#};
    let key_in_key_slot = indoc! {r"
        apiVersion: v1
        kind: Secret
        data:
        {{- range $key, $value := .Values.data }}
          {{ $key }}: {{ tpl $value $ | b64enc | quote }}
        {{- end }}
    "};
    let tpl_whole_slot = indoc! {r"
        volumeMounts:
          - name: secrets
            mountPath: {{ tpl .Values.mountPath $ }}
    "};
    let tpl_partial_slot = indoc! {r"
        command:
          - --cluster-name={{ tpl (.Values.clusterName) . }}
    "};

    for (label, src, values_yaml, cases) in [
        (
            "a ranged key in a plain value slot",
            key_in_value_slot,
            "extraEnvVars: {}\n",
            vec![
                (
                    serde_json::json!({ "extraEnvVars": { "BAD: KEY": "x" } }),
                    false,
                ),
                (
                    serde_json::json!({ "extraEnvVars": { "A #b": "x" } }),
                    false,
                ),
                (
                    serde_json::json!({ "extraEnvVars": { "GOOD.KEY": "x" } }),
                    true,
                ),
            ],
        ),
        (
            "a ranged key in a mapping-key slot",
            key_in_key_slot,
            "data: {}\n",
            vec![
                (serde_json::json!({ "data": { "BAD: KEY": "x" } }), false),
                (serde_json::json!({ "data": { "GOOD_KEY": "x" } }), true),
                // The VALUE renders through `b64enc | quote`, which reshapes
                // the text, so its own characters never reach a plain token.
                (serde_json::json!({ "data": { "K": "a: b" } }), true),
            ],
        ),
        (
            "a tpl operand filling a whole plain slot",
            tpl_whole_slot,
            "mountPath: /etc/secrets\n",
            vec![
                (serde_json::json!({ "mountPath": "/etc/a: b" }), false),
                (serde_json::json!({ "mountPath": "/etc/secrets" }), true),
                (
                    serde_json::json!({ "mountPath": "{{ .Release.Name }}: x" }),
                    true,
                ),
            ],
        ),
        (
            "a tpl operand inside a partial plain token",
            tpl_partial_slot,
            "clusterName: prod\n",
            vec![
                (serde_json::json!({ "clusterName": "x: y" }), false),
                (serde_json::json!({ "clusterName": "prod" }), true),
                // Literal text opens this token, so a leading indicator is
                // ordinary content rather than YAML structure.
                (serde_json::json!({ "clusterName": "- x" }), true),
            ],
        ),
    ] {
        let schema = schema_for_values_yaml(parse_ir(src), Some(values_yaml));
        for (instance, want) in cases {
            sim_assert_eq!(
                have: schema_accepts_instance(&schema, &instance),
                want: want,
                "{label}: instance={instance}; schema={schema}"
            );
        }
    }
}

/// A helper body renders at its CALLER's position, so its own plain slots
/// bind a lexical language only where the caller consumes the body's text as
/// YAML. Jenkins routes its `JCasC` defaults through two nested helpers into
/// a config map block scalar keyed `jcasc-default-config.yaml`: the manifest
/// stays valid, but the embedded document no longer parses. A block scalar
/// whose key names no YAML document is opaque text, and a reshaping stage
/// between the body and the sink renders its own characters, so both abstain.
#[test]
fn helper_slots_bind_their_language_where_the_caller_consumes_yaml() {
    let helpers = indoc! {r#"
        {{- define "chart.casc.podTemplate" -}}
        - name: "default"
        {{- if .Values.agent.annotations }}
          annotations:
          {{- range $key, $value := .Values.agent.annotations }}
          - key: {{ $key }}
            value: {{ $value | quote }}
          {{- end }}
        {{- end }}
          envVars:
          {{- range $var := .Values.agent.envVars }}
          - envVar:
              key: {{ $var.name | quote }}
              value: {{ tpl $var.value $ }}
          {{- end }}
        {{- end -}}
        {{- define "chart.casc.defaults" -}}
        jenkins:
          clouds:
          - kubernetes:
              templates:
              {{- include "chart.casc.podTemplate" . | nindent 8 }}
        {{- end -}}
    "#};
    let yaml_document_sink = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: casc
        data:
          jcasc-default-config.yaml: |-
            {{- include "chart.casc.defaults" . | nindent 4 }}
    "#};
    let opaque_document_sink = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: casc
        data:
          jcasc-default-config.txt: |-
            {{- include "chart.casc.defaults" . | nindent 4 }}
    "#};
    let encoded_sink = indoc! {r#"
        apiVersion: v1
        kind: Secret
        metadata:
          name: casc
        data:
          jcasc-default-config.yaml: {{ include "chart.casc.defaults" . | b64enc | quote }}
    "#};
    let values_yaml = indoc! {"
        agent:
          annotations: {}
          envVars: []
    "};

    let broken_key = serde_json::json!({ "agent": { "annotations": { "broken: key": "x" } } });
    let broken_value = serde_json::json!({
        "agent": { "envVars": [{ "name": "URL", "value": "http: //x" }] },
    });
    let safe = serde_json::json!({
        "agent": {
            "annotations": { "kubernetes.io/scrape": "true" },
            "envVars": [{ "name": "URL", "value": "http://x" }],
        },
    });

    for (label, src, cases) in [
        (
            "a block scalar naming a YAML document",
            yaml_document_sink,
            vec![(&broken_key, false), (&broken_value, false), (&safe, true)],
        ),
        (
            "a block scalar naming no YAML document",
            opaque_document_sink,
            vec![(&broken_key, true), (&broken_value, true), (&safe, true)],
        ),
        (
            "a reshaping stage between the body and the sink",
            encoded_sink,
            vec![(&broken_key, true), (&broken_value, true), (&safe, true)],
        ),
    ] {
        let schema = schema_for_values_yaml(parse_ir_with_helpers(src, helpers), Some(values_yaml));
        for (overrides, want) in cases {
            let instance = composed_instance(values_yaml, overrides.clone());
            sim_assert_eq!(
                have: schema_accepts_instance(&schema, &instance),
                want: want,
                "{label}: instance={instance}; schema={schema}"
            );
        }
    }
}
