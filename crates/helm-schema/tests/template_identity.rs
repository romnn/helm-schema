//! Template identity across charts.
//!
//! Helm registers every template under its exact execution name
//! `<root>/[charts/<dependency key>]*/templates/<path>` and reports
//! `<namespace>/templates` as `.Template.BasePath`. Both facts follow the
//! rendered file, so a helper the parent defines resolves
//! `include (print $.Template.BasePath "/frag.yaml")` to the *executing*
//! chart's own template. Verified against `helm template` v4.2.3: the root
//! probe renders `parentName: p` and the subchart probe `childName: c`,
//! even though both charts ship `templates/frag.yaml` and both reach it
//! through the same parent-defined helper.

use color_eyre::eyre::{self, WrapErr as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{GenerateOptions, SchemaProfile};
use helm_schema::provider::ProviderOptions;
use indoc::indoc;
use serde_json::json;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

/// Both charts ship `templates/frag.yaml` and render it through the
/// parent's `shared.selfinclude` helper.
fn write_shared_helper_chart(chart_dir: &VfsPath) -> eyre::Result<()> {
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: d3e2e
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("values.yaml")?,
        indoc! {"
            parentSecret:
              name: p
        "},
    )?;
    test_util::write(
        &chart_dir.join("templates/_helpers.tpl")?,
        indoc! {r#"
            {{- define "shared.selfinclude" -}}
            {{ include (print $.Template.BasePath "/frag.yaml") $ }}
            {{- end -}}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("templates/frag.yaml")?,
        "parentName: {{ .Values.parentSecret.name }}\n",
    )?;
    // A subdirectory template still reports the chart's own `templates`
    // directory, so this reaches `d3e2e/templates/frag.yaml`.
    test_util::write(
        &chart_dir.join("templates/sub/probe.yaml")?,
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: parent-probe
            data:
              {{- include "shared.selfinclude" . | nindent 2 }}
        "#},
    )?;
    test_util::write(
        &chart_dir.join("charts/kid/Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: kid
            version: 0.1.0
        "},
    )?;
    test_util::write(
        &chart_dir.join("charts/kid/values.yaml")?,
        "childToken: c\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/kid/templates/frag.yaml")?,
        "childName: {{ .Values.childToken }}\n",
    )?;
    test_util::write(
        &chart_dir.join("charts/kid/templates/probe.yaml")?,
        indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: kid-probe
            data:
              {{- include "shared.selfinclude" . | nindent 2 }}
        "#},
    )?;
    Ok(())
}

fn generate(chart_dir: VfsPath) -> eyre::Result<serde_json::Value> {
    AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")),
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    })
    .generated_schema()
    .map(|generated| generated.schema)
    .map_err(eyre::Report::from)
    .wrap_err("generate schema")
}

#[test]
fn a_shared_helper_resolves_each_charts_own_template() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    write_shared_helper_chart(&chart_dir)?;
    let schema = generate(chart_dir)?;

    // `kid` carries only its own `childToken`: the parent's
    // `templates/frag.yaml` is a different template, so `parentSecret` is
    // not a subchart key and its absence guard stays at the root, where
    // Helm really does abort on `.Values.parentSecret.name`.
    sim_assert_eq!(
        have: schema,
        want: json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "allOf": [
                {
                    "additionalProperties": {},
                    "properties": { "kid": { "type": ["null", "object"] } }
                },
                {
                    "if": {
                        "anyOf": [
                            {
                                "not": {
                                    "properties": { "parentSecret": {} },
                                    "required": ["parentSecret"],
                                    "type": "object"
                                }
                            },
                            {
                                "properties": { "parentSecret": { "enum": [null] } },
                                "required": ["parentSecret"],
                                "type": "object"
                            }
                        ]
                    },
                    "then": false
                }
            ],
            "properties": {
                "global": {},
                "kid": {
                    "additionalProperties": {},
                    "properties": {
                        "childToken": {},
                        "global": { "additionalProperties": {}, "type": "object" }
                    },
                    "type": "object"
                },
                "parentSecret": {
                    "additionalProperties": {},
                    "properties": { "name": {} },
                    "type": "object"
                }
            },
            "type": "object"
        })
    );
    Ok(())
}
