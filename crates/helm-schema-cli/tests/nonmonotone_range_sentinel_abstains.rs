//! Regression test for the non-monotone range-sentinel exactness collapse.
//!
//! A range sentinel is only existentially quantifiable when the loop body
//! is monotone: seeded falsy and reassigned exclusively by truthy-implying
//! writes. An `else` arm writing `false` makes the accumulator
//! last-write-wins — `[{enabled: true}, {enabled: false}]` ends the
//! sentinel falsy while a member still satisfies the existential — so the
//! encoded `contains` arm rejected a values document Helm renders fine.
//! The quantification must abstain for such sentinels while the monotone
//! pattern keeps its exact existential encoding.

use color_eyre::eyre::{self, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions, SchemaProfile};
use indoc::indoc;
use vfs::VfsPath;

const CHART_YAML: &str = indoc! {"
    apiVersion: v2
    name: app
    version: 0.1.0
"};

const VALUES_YAML: &str = indoc! {r#"
    items: []
    mustBeSet: ""
"#};

const NONMONOTONE_TEMPLATE: &str = indoc! {r#"
    {{- $f := false -}}
    {{- range .Values.items -}}
    {{- if .enabled -}}{{- $f = true -}}{{- else -}}{{- $f = false -}}{{- end -}}
    {{- end -}}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: test
    data:
    {{- if $f }}
      mode: "on"
      required: {{ required "must be set" .Values.mustBeSet }}
    {{- end }}
"#};

const MONOTONE_TEMPLATE: &str = indoc! {r#"
    {{- $f := false -}}
    {{- range .Values.items -}}
    {{- if .enabled -}}{{- $f = true -}}{{- end -}}
    {{- end -}}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: test
    data:
    {{- if $f }}
      mode: "on"
      required: {{ required "must be set" .Values.mustBeSet }}
    {{- end }}
"#};

fn generated_schema(template: &str) -> eyre::Result<serde_json::Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, VALUES_YAML)?;
    test_util::write(&chart_dir.join("templates/cm.yaml")?, template)?;

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
        .map(|generated| generated.schema)
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")
}

#[test]
fn nonmonotone_sentinel_does_not_claim_the_existential() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema(NONMONOTONE_TEMPLATE)?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // Helm's last write wins: the trailing `enabled: false` member leaves
    // the sentinel falsy, the guarded body never renders, and the empty
    // `mustBeSet` is never demanded (adjudicated with `helm template`).
    let truthy_then_falsy = serde_json::json!({
        "items": [{ "enabled": true }, { "enabled": false }],
        "mustBeSet": "",
    });
    assert!(
        validator.is_valid(&truthy_then_falsy),
        "a truthy member followed by a falsy one leaves the sentinel falsy at \
         render time; the schema must not read the accumulator existentially: {}",
        validator
            .iter_errors(&truthy_then_falsy)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    );

    Ok(())
}

#[test]
fn monotone_sentinel_keeps_the_exact_existential() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema(MONOTONE_TEMPLATE)?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // With no falsy-capable write, "some member enables" is the exact
    // sentinel truth: an enabled member reaches `required` with the empty
    // string and aborts rendering.
    assert!(
        !validator.is_valid(&serde_json::json!({
            "items": [{ "enabled": true }],
            "mustBeSet": "",
        })),
        "an enabled member renders the guarded body and aborts on the empty \
         required value; the schema must keep rejecting it",
    );
    let all_disabled = serde_json::json!({
        "items": [{ "enabled": false }],
        "mustBeSet": "",
    });
    assert!(
        validator.is_valid(&all_disabled),
        "with every member disabled the guarded body never renders: {}",
        validator
            .iter_errors(&all_disabled)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    );

    Ok(())
}
