//! CRLF charts must analyze identically to LF charts.
//!
//! Charts authored on Windows arrive with CRLF line endings, but nothing
//! else in the suite ever exercises that: `.gitattributes` normalizes every
//! vendored fixture to LF on checkout, on every platform. Without this test
//! a CRLF-only regression in template parsing, values-comment extraction,
//! or guard analysis would ship unseen and only surface for Windows users.

use color_eyre::eyre;
use helm_schema::AnalysisSession;
use helm_schema::generation::{GenerateOptions, SchemaProfile};
use helm_schema::provider::ProviderOptions;
use indoc::indoc;
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

/// One small chart exercising the line-ending-sensitive surfaces: values
/// doc-comments, trim-marked control flow, a helper define plus include,
/// and a quoted template splice.
fn build_chart(line_ending: &str) -> eyre::Result<VfsPath> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    let files = [
        (
            "Chart.yaml",
            indoc! {"
                apiVersion: v2
                name: root
                version: 0.1.0
            "},
        ),
        (
            "values.yaml",
            indoc! {"
                # -- Whether the config map is enabled
                enabled: true
                replicas: 1
                nameOverride: null
            "},
        ),
        (
            "templates/_helpers.tpl",
            indoc! {r#"
                {{- define "root.name" -}}
                {{- default .Chart.Name .Values.nameOverride -}}
                {{- end -}}
            "#},
        ),
        (
            "templates/configmap.yaml",
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: {{ include "root.name" . }}
                data:
                  enabled: "{{ .Values.enabled }}"
                  {{- if .Values.replicas }}
                  replicas: {{ .Values.replicas | quote }}
                  {{- end }}
            "#},
        ),
    ];
    for (path, source) in files {
        test_util::write(
            &chart_dir.join(path)?,
            source.replace('\n', line_ending).into_bytes(),
        )?;
    }
    Ok(chart_dir)
}

fn schema_for(chart_dir: VfsPath) -> eyre::Result<serde_json::Value> {
    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        profile: SchemaProfile::default(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            k8s_schema_cache_dir: Some(test_util::cold_provider_cache_root("k8s")),
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")),
            disable_k8s_schemas: true,
            ..Default::default()
        },
    };
    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(eyre::Report::from)
}

#[test]
fn crlf_chart_generates_identical_schema_to_lf_chart() -> eyre::Result<()> {
    let lf = schema_for(build_chart("\n")?)?;
    let crlf = schema_for(build_chart("\r\n")?)?;

    // Guard against the test passing vacuously on two empty schemas: the LF
    // analysis must have seen the values doc-comment and the guarded use.
    sim_assert_eq!(
        have: lf.pointer("/properties/enabled/description").and_then(serde_json::Value::as_str),
        want: Some("Whether the config map is enabled")
    );
    assert!(
        lf.pointer("/properties/replicas").is_some(),
        "LF analysis must surface the guarded replicas use: {lf}"
    );

    sim_assert_eq!(have: crlf, want: lf);
    Ok(())
}
