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

use color_eyre::eyre::{self, WrapErr as _};
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
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")),
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
            {
                "additionalProperties": {},
                "properties": { "kid": { "type": ["null", "object"] } },
            },
        ],
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "flag": { "type": "boolean" },
                    "global": { "additionalProperties": {}, "type": "object" },
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
                "additionalProperties": {},
                "properties": { "kid": { "type": ["null", "object"] } },
            },
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
                    "global": { "additionalProperties": {}, "type": "object" },
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
        "properties": { "global": { "additionalProperties": {}, "type": "object" }, "mode": {} },
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
            {
                "additionalProperties": {},
                "properties": { "kid": { "type": ["null", "object"] } },
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
