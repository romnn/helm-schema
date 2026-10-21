//! The documents `helm lint` validates are further must-accept documents.
//!
//! `helm template` validates the coalesced values document, but `helm lint`
//! validates the root chart's `values.yaml` coalesced with the user's
//! override files only, never with dependency defaults (Helm v4.2.3
//! `pkg/chart/v2/lint/rules/values.go:62-68`). A constraint derived from a
//! key only a dependency declares therefore rejects a lint document while
//! the coalesced defaults satisfy it, and it is withdrawn.
//!
//! Every verdict below is adjudicated against Helm v4.2.3 with
//! `--kube-version 1.29.0`.

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{LintDocument, LintOutcome, LintWithdrawal, ValuesPath};
use helm_schema_cli::{GenerateOptions, ProviderOptions, SchemaProfile};
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

fn generate(chart_dir: VfsPath) -> eyre::Result<(serde_json::Value, Vec<LintWithdrawal>)> {
    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        authoring: helm_schema::generation::AuthoringPolicy::default(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };
    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| (generated.schema, generated.emission_report.lint_withdrawals))
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")
}

/// The subchart's template dereferences `grp`, which only its own defaults
/// supply, under the `flag` gate.
const GATED_GRP_TEMPLATE: &str = indoc! {r"
    {{- if .Values.flag }}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: kid-cm
    data:
      v: {{ .Values.grp.enabled | quote }}
    {{- end }}
"};

/// A root chart with one dependency `kid`.
fn write_lint_root_chart(
    chart_dir: &VfsPath,
    root_values: &str,
    kid_values: &str,
    kid_template: &str,
) -> eyre::Result<()> {
    for (path, contents) in [
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: lintroot
                version: 0.1.0
                dependencies:
                  - name: kid
                    version: 0.1.0
            "},
        ),
        ("values.yaml", root_values),
        (
            "charts/kid/Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: kid
                version: 0.1.0
            "},
        ),
        ("charts/kid/values.yaml", kid_values),
        ("charts/kid/templates/cm.yaml", kid_template),
    ] {
        test_util::write(&chart_dir.join(path)?, contents)?;
    }
    Ok(())
}

const KID_FLAG_ON_VALUES: &str = indoc! {"
    flag: true
    grp:
      enabled: true
"};

/// The schema both root `kid.flag` defaults produce.
fn expected_lint_root_schema() -> serde_json::Value {
    serde_json::json!({
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
                    "properties": {
                        "kid": {
                            "properties": { "flag": { "$ref": "#/$defs/t" } },
                            "required": ["flag"],
                            "type": "object",
                        },
                    },
                    "required": ["kid"],
                    "type": "object",
                },
                // The arm that made a missing `grp` abort under the gate is
                // withdrawn; only the present-`grp` typing remains.
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": { "grp": { "type": "object" } },
                        },
                    },
                },
            },
        ],
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "flag": { "type": "boolean" },
                    "global": {},
                    "grp": {
                        "additionalProperties": {},
                        "properties": { "enabled": {} },
                    },
                },
                "type": "object",
            },
        },
        "type": "object",
    })
}

#[test]
fn a_dependency_default_missing_from_the_root_values_stays_accepted() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(
        &chart_dir,
        indoc! {"
            kid:
              flag: true
        "},
        KID_FLAG_ON_VALUES,
        GATED_GRP_TEMPLATE,
    )?;
    let (schema, withdrawals) = generate(chart_dir)?;
    sim_assert_eq!(have: withdrawals, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse("kid"),
        document: LintDocument::Root,
        deciding_paths: vec![ValuesPath::parse("kid.grp")],
        outcome: LintOutcome::Withdrawn,
    }]);

    sim_assert_eq!(have: schema, want: expected_lint_root_schema());

    let validator = jsonschema::validator_for(&schema)?;
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": { "flag": true } })),
        "`helm lint` validates the root values.yaml as written and passes"
    );
    assert!(
        validator.is_valid(&serde_json::json!({
            "kid": { "flag": true, "global": {}, "grp": { "enabled": true } },
        })),
        "`helm template` renders the coalesced defaults"
    );
    // The recorded precision cost: `--set kid.grp=null` aborts in Helm with
    // `{"kid":{"flag":true,"global":{}}}`, a document a schema cannot tell
    // apart from the lint document by the `grp` key alone.
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": { "flag": true, "global": {} } })),
        "the withdrawn arm no longer rejects a deleted `grp`"
    );
    Ok(())
}

/// `helm lint -f` validates the root values.yaml coalesced with the user's
/// file, never with dependency defaults (Helm v4.2.3
/// `pkg/chart/v2/lint/rules/values.go:62-68`). Enabling `kid.flag` by
/// override therefore lints a document without `kid.grp`, which
/// `helm template` renders because the subchart default supplies `grp`.
#[test]
fn an_override_that_activates_a_dependency_guard_still_lints() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(
        &chart_dir,
        indoc! {"
            kid:
              flag: false
        "},
        KID_FLAG_ON_VALUES,
        GATED_GRP_TEMPLATE,
    )?;
    let (schema, _) = generate(chart_dir)?;
    sim_assert_eq!(have: schema, want: expected_lint_root_schema());

    let validator = jsonschema::validator_for(&schema)?;
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": { "flag": true } })),
        "`helm lint -f` with `kid.flag: true` validates this document and passes"
    );
    assert!(
        validator.is_valid(&serde_json::json!({
            "kid": { "flag": true, "global": {}, "grp": { "enabled": true } },
        })),
        "`helm template` renders the coalesced document"
    );
    Ok(())
}

/// The switch itself can be one only the dependency declares: the root sets
/// nothing under `kid`, so `helm lint -f` with `kid.flag: true` validates
/// `{kid: {flag: true}}`, still without `kid.grp`, and passes.
#[test]
fn an_override_that_activates_a_dependency_only_guard_still_lints() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(
        &chart_dir,
        "kid: {}\n",
        indoc! {"
            flag: false
            grp:
              enabled: true
        "},
        GATED_GRP_TEMPLATE,
    )?;
    let (schema, withdrawals) = generate(chart_dir)?;
    sim_assert_eq!(have: schema, want: expected_lint_root_schema());
    sim_assert_eq!(have: withdrawals, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse("kid"),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.grp")],
        outcome: LintOutcome::Withdrawn,
    }]);

    let validator = jsonschema::validator_for(&schema)?;
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": { "flag": true } })),
        "`helm lint -f` with `kid.flag: true` validates this document and passes"
    );
    assert!(
        validator.is_valid(&serde_json::json!({
            "kid": { "flag": true, "global": {}, "grp": { "enabled": true } },
        })),
        "`helm template` renders the coalesced document"
    );
    Ok(())
}

/// The schema of the umbrella chart with an empty root `values.yaml`.
fn expected_empty_root_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "kid": {
                                    "allOf": [
                                        { "type": "object" },
                                        {
                                            "anyOf": [
                                                {
                                                    "not": {
                                                        "properties": { "grp": {} },
                                                        "required": ["grp"],
                                                        "type": "object",
                                                    },
                                                },
                                                {
                                                    "properties": { "grp": { "enum": [null] } },
                                                    "required": ["grp"],
                                                    "type": "object",
                                                },
                                            ],
                                        },
                                    ],
                                },
                            },
                            "required": ["kid"],
                            "type": "object",
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": { "kid": {} },
                                        "required": ["kid"],
                                        "type": "object",
                                    },
                                },
                                {
                                    "properties": { "kid": { "enum": [null] } },
                                    "required": ["kid"],
                                    "type": "object",
                                },
                            ],
                        },
                    ],
                },
                "then": false,
            },
        ],
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "global": {},
                    "grp": {
                        "additionalProperties": {},
                        "properties": { "enabled": {} },
                        "type": "object",
                    },
                },
                "type": "object",
            },
        },
        "type": "object",
    })
}

/// An empty root `values.yaml` is an empty table to Helm, not a missing
/// document: `helm lint` validates `{}` against the schema, while
/// `helm template` renders the subchart's own `grp`.
#[test]
fn an_empty_root_values_file_is_still_a_lint_document() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(
        &chart_dir,
        "# every value comes from the dependency\n",
        indoc! {"
            grp:
              enabled: true
        "},
        indoc! {r"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: kid-cm
            data:
              v: {{ .Values.grp.enabled | quote }}
        "},
    )?;
    let (schema, withdrawals) = generate(chart_dir)?;
    sim_assert_eq!(have: schema, want: expected_empty_root_schema());
    sim_assert_eq!(have: withdrawals, want: vec![LintWithdrawal {
        anchor: ValuesPath::parse("kid"),
        document: LintDocument::Floor,
        deciding_paths: vec![ValuesPath::parse("kid.grp")],
        outcome: LintOutcome::Withdrawn,
    }]);

    let validator = jsonschema::validator_for(&schema)?;
    assert!(
        validator.is_valid(&serde_json::json!({})),
        "`helm lint` validates the empty root values and passes"
    );
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": {} })),
        "`helm lint -f` with an empty `kid` table validates it without `grp` and passes"
    );
    assert!(
        validator.is_valid(&serde_json::json!({
            "kid": { "global": {}, "grp": { "enabled": true } },
        })),
        "`helm template` renders the coalesced defaults"
    );
    Ok(())
}

/// The subchart aborts unless `mode` is `auto` or `all`; only its own
/// defaults supply `mode`, so `helm lint` validates `kid` without it and the
/// clause is conditioned on `mode` being present instead of withdrawn.
const MODE_TEMPLATE: &str = indoc! {r#"
    {{- if and (ne .Values.mode "auto") (ne .Values.mode "all") }}
    {{- fail "mode must be auto or all" }}
    {{- end }}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: kid-cm
    data:
      mode: {{ .Values.mode | quote }}
"#};

/// The root-anchored `mode` clause, conditioned on `kid.mode` being present.
fn mode_root_clause() -> serde_json::Value {
    serde_json::json!({
        "if": {
            "allOf": [
                {
                    "anyOf": [
                        {
                            "not": {
                                "properties": {
                                    "kid": {
                                        "properties": { "mode": {} },
                                        "required": ["mode"],
                                        "type": "object",
                                    },
                                },
                                "required": ["kid"],
                                "type": "object",
                            },
                        },
                        {
                            "properties": {
                                "kid": {
                                    "properties": { "mode": { "not": { "enum": ["all"] } } },
                                    "required": ["mode"],
                                    "type": "object",
                                },
                            },
                            "required": ["kid"],
                            "type": "object",
                        },
                    ],
                },
                {
                    "anyOf": [
                        {
                            "not": {
                                "properties": {
                                    "kid": {
                                        "properties": { "mode": {} },
                                        "required": ["mode"],
                                        "type": "object",
                                    },
                                },
                                "required": ["kid"],
                                "type": "object",
                            },
                        },
                        {
                            "properties": {
                                "kid": {
                                    "properties": { "mode": { "not": { "enum": ["auto"] } } },
                                    "required": ["mode"],
                                    "type": "object",
                                },
                            },
                            "required": ["kid"],
                            "type": "object",
                        },
                    ],
                },
                {
                    "anyOf": [
                        {
                            "not": {
                                "properties": { "kid": {} },
                                "required": ["kid"],
                                "type": "object",
                            },
                        },
                        {
                            "properties": { "kid": { "enum": [null] } },
                            "required": ["kid"],
                            "type": "object",
                        },
                    ],
                },
            ],
        },
        "then": {
            "if": {
                "properties": {
                    "kid": {
                        "properties": { "mode": { "not": { "enum": [null] } } },
                        "required": ["mode"],
                        "type": "object",
                    },
                },
                "required": ["kid"],
                "type": "object",
            },
            "then": false,
        },
    })
}

/// The `kid` property of the `mode` chart, with its own conditioned clause.
fn mode_kid_schema() -> serde_json::Value {
    serde_json::json!({
        "additionalProperties": {},
        "allOf": [
            {
                "if": {
                    "allOf": [
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": { "mode": {} },
                                        "required": ["mode"],
                                        "type": "object",
                                    },
                                },
                                {
                                    "properties": { "mode": { "not": { "enum": ["all"] } } },
                                    "required": ["mode"],
                                    "type": "object",
                                },
                            ],
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": { "mode": {} },
                                        "required": ["mode"],
                                        "type": "object",
                                    },
                                },
                                {
                                    "properties": { "mode": { "not": { "enum": ["auto"] } } },
                                    "required": ["mode"],
                                    "type": "object",
                                },
                            ],
                        },
                    ],
                },
                "then": {
                    "if": {
                        "properties": { "mode": { "not": { "enum": [null] } } },
                        "required": ["mode"],
                        "type": "object",
                    },
                    "then": false,
                },
            },
        ],
        "properties": { "global": {}, "mode": {} },
        "type": "object",
    })
}

/// The schema of the `mode` chart.
fn expected_mode_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "additionalProperties": {},
                "properties": {
                    "kid": {
                        "additionalProperties": {},
                        "properties": { "mode": { "type": ["null", "string"] } },
                    },
                },
            },
            mode_root_clause(),
        ],
        "properties": {
            "global": {},
            "kid": mode_kid_schema(),
        },
        "type": "object",
    })
}

/// `helm lint` passes the root values and `helm template` renders the
/// defaults, while `--set kid.mode=bogus` aborts: the value check must
/// survive the lint gate (Helm v4.2.3).
#[test]
fn a_dependency_value_check_survives_the_lint_gate() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(&chart_dir, "kid: {}\n", "mode: auto\n", MODE_TEMPLATE)?;
    let (schema, withdrawals) = generate(chart_dir)?;
    sim_assert_eq!(have: schema, want: expected_mode_schema());
    sim_assert_eq!(have: withdrawals, want: vec![
        LintWithdrawal {
            anchor: ValuesPath::parse("kid"),
            document: LintDocument::Root,
            deciding_paths: vec![ValuesPath::parse("kid.mode")],
            outcome: LintOutcome::Conditioned,
        },
        LintWithdrawal {
            anchor: ValuesPath::parse(""),
            document: LintDocument::Floor,
            deciding_paths: vec![ValuesPath::parse("kid.mode")],
            outcome: LintOutcome::Conditioned,
        },
    ]);

    let validator = jsonschema::validator_for(&schema)?;
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": {} })),
        "`helm lint` passes"
    );
    assert!(
        validator.is_valid(&serde_json::json!({ "kid": { "global": {}, "mode": "all" } })),
        "`helm template --set kid.mode=all` renders"
    );
    assert!(
        !validator.is_valid(&serde_json::json!({ "kid": { "global": {}, "mode": "bogus" } })),
        "`helm template --set kid.mode=bogus` aborts"
    );
    Ok(())
}

fn expected_unconditional_dependency_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [{
            "if": {
                "properties": {
                    "kid": {
                        "properties": {"enableServiceLinks": {"not": {"enum": [null]}}},
                        "required": ["enableServiceLinks"],
                        "type": "object"
                    }
                },
                "required": ["kid"],
                "type": "object"
            },
            "then": {
                "additionalProperties": {},
                "properties": {
                    "kid": {"required": ["enableServiceLinks"], "type": "object"}
                }
            }
        }],
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "enableServiceLinks": {
                        "anyOf": [
                            {
                                "description": "EnableServiceLinks indicates whether information about services should be injected into pod's environment variables, matching the syntax of Docker links. Optional: Defaults to true.",
                                "type": "boolean"
                            },
                            {
                                "pattern": "^(true|True|TRUE|false|False|FALSE|yes|Yes|YES|no|No|NO|on|On|ON|off|Off|OFF|y|Y|n|N)$",
                                "type": "string"
                            }
                        ]
                    },
                    "flag": { "type": "boolean" },
                    "global": {}
                },
                "type": "object"
            }
        },
        "type": "object"
    })
}

/// A direct provider hole requires a value after coalescing, while lint also sees
/// the parent's partial override without the dependency's defaults.
#[test]
fn unconditional_dependency_defaults_do_not_require_raw_parent_members() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(
        &chart_dir,
        indoc! {"
            kid:
              flag: true
        "},
        indoc! {"
            flag: true
            enableServiceLinks: false
        "},
        indoc! {"
            apiVersion: v1
            kind: Pod
            metadata:
              name: kid
            spec:
              enableServiceLinks: {{ .Values.enableServiceLinks }}
              containers:
                - name: kid
                  image: busybox:1.37
        "},
    )?;
    let schema = generate_provider_schema(chart_dir)?;
    sim_assert_eq!(have: schema, want: expected_unconditional_dependency_schema());
    let validator = jsonschema::validator_for(&schema)?;
    assert!(validator.is_valid(&serde_json::json!({"kid": {"flag": true}})));
    assert!(validator.is_valid(&serde_json::json!({
        "kid": {"flag": true, "enableServiceLinks": false, "global": {}}
    })));
    for (value, accepted) in [
        (serde_json::json!(null), false),
        (serde_json::json!(true), true),
        (serde_json::json!(false), true),
        (serde_json::json!(0), false),
        (serde_json::json!(""), false),
        (serde_json::json!("false"), true),
        (serde_json::json!("invalid"), false),
        (serde_json::json!([]), false),
        (serde_json::json!({}), false),
    ] {
        sim_assert_eq!(
            have: validator.is_valid(&serde_json::json!({
                "kid": {"flag": true, "enableServiceLinks": value}
            })),
            want: accepted
        );
    }
    Ok(())
}

fn generate_provider_schema(chart_dir: VfsPath) -> eyre::Result<serde_json::Value> {
    Ok(AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        authoring: helm_schema::generation::AuthoringPolicy::default(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            k8s_schema_cache_dir: Some(
                test_util::workspace_testdata()
                    .join("provider-bundle/kubernetes-json-schema-cache"),
            ),
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            ..Default::default()
        },
    })
    .generated_schema()?
    .schema)
}

fn missing_webhook_condition() -> serde_json::Value {
    serde_json::json!({
        "allOf": [
            {"type": "object"},
            {"anyOf": [
                {"not": {
                    "properties": {"webhook": {}},
                    "required": ["webhook"],
                    "type": "object"
                }},
                {
                    "properties": {"webhook": {"enum": [null]}},
                    "required": ["webhook"],
                    "type": "object"
                }
            ]}
        ]
    })
}

fn expected_webhook_dependency_schema() -> eyre::Result<serde_json::Value> {
    let mut expected = expected_unconditional_dependency_schema();
    let field = expected
        .pointer("/properties/kid/properties/enableServiceLinks")
        .ok_or_eyre("expected provider field absent")?
        .clone();
    let nonnull_field = serde_json::json!({
        "properties": {"enableServiceLinks": {"not": {"enum": [null]}}},
        "required": ["enableServiceLinks"],
        "type": "object"
    });
    let missing_webhook = missing_webhook_condition();
    let kid = expected
        .pointer_mut("/properties/kid")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_eyre("expected dependency schema absent")?;
    kid.get_mut("properties")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_eyre("expected dependency properties absent")?
        .insert(
            "webhook".into(),
            serde_json::json!({
                "additionalProperties": {},
                "properties": {"enableServiceLinks": field, "flag": {"type": "boolean"}},
                "type": "object"
            }),
        );
    kid.insert(
        "allOf".into(),
        serde_json::json!([{
            "if": missing_webhook,
            "then": false
        }]),
    );
    let main_clause = expected
        .pointer("/allOf/0")
        .ok_or_eyre("expected main presence guard absent")?
        .clone();
    let clauses = serde_json::json!([
        main_clause,
        {
            "if": {
                "properties": {"kid": {
                    "properties": {"webhook": nonnull_field},
                    "required": ["webhook"],
                    "type": "object"
                }},
                "required": ["kid"],
                "type": "object"
            },
            "then": {
                "additionalProperties": {},
                "properties": {"kid": {
                    "additionalProperties": {},
                    "properties": {"webhook": {
                        "required": ["enableServiceLinks"],
                        "type": "object"
                    }}
                }}
            }
        },
        {
            "if": {"allOf": [
                {
                    "properties": {"kid": missing_webhook},
                    "required": ["kid"],
                    "type": "object"
                },
                {"anyOf": [
                    {"not": {
                        "properties": {"kid": {}},
                        "required": ["kid"],
                        "type": "object"
                    }},
                    {
                        "properties": {"kid": {"enum": [null]}},
                        "required": ["kid"],
                        "type": "object"
                    }
                ]}
            ]},
            "then": false
        }
    ]);
    expected
        .as_object_mut()
        .ok_or_eyre("expected root schema is not an object")?
        .insert("allOf".into(), clauses);
    Ok(expected)
}

/// Independent main and webhook defaults must each survive a partial umbrella override.
#[test]
fn webhook_defaults_keep_independent_lint_presence_guards() -> eyre::Result<()> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_lint_root_chart(
        &chart_dir,
        indoc! {"
            kid:
              flag: true
              webhook:
                flag: true
        "},
        indoc! {"
            flag: true
            enableServiceLinks: false
            webhook:
              flag: true
              enableServiceLinks: false
        "},
        indoc! {"
            apiVersion: v1
            kind: Pod
            metadata:
              name: kid
            spec:
              enableServiceLinks: {{ .Values.enableServiceLinks }}
              containers:
                - name: kid
                  image: busybox:1.37
            ---
            apiVersion: v1
            kind: Pod
            metadata:
              name: kid-webhook
            spec:
              enableServiceLinks: {{ .Values.webhook.enableServiceLinks }}
              containers:
                - name: kid
                  image: busybox:1.37
        "},
    )?;
    let schema = generate_provider_schema(chart_dir)?;
    sim_assert_eq!(have: schema, want: expected_webhook_dependency_schema()?);
    let validator = jsonschema::validator_for(&schema)?;
    let states = [
        (None, true),
        (Some(serde_json::json!(false)), true),
        (Some(serde_json::json!(true)), true),
        (Some(serde_json::json!(null)), false),
        (Some(serde_json::json!(0)), false),
        (Some(serde_json::json!("invalid")), false),
        (Some(serde_json::json!([])), false),
        (Some(serde_json::json!({})), false),
    ];
    for (main, main_valid) in &states {
        for (webhook, webhook_valid) in &states {
            let mut values = serde_json::json!({"kid": {"flag": true, "webhook": {"flag": true}}});
            if let Some(main) = main {
                values["kid"]["enableServiceLinks"] = main.clone();
            }
            if let Some(webhook) = webhook {
                values["kid"]["webhook"]["enableServiceLinks"] = webhook.clone();
            }
            sim_assert_eq!(
                have: validator.is_valid(&values),
                want: *main_valid && *webhook_valid,
                "{values}"
            );
        }
    }
    Ok(())
}
