//! Regression test for the stringified pattern-condition exactness collapse.
//!
//! `regexMatch p (toString .Values.x)` tests the total stringification of
//! every raw kind, while the raw-level `MatchesPattern` guard also asserts
//! string-ness: `toString 3` renders `"3"` and matches `^[0-9]+$`, but the
//! guard rejects the integer. Claiming the guard as the condition's exact
//! truth backprojected an unconditional `{type: string, pattern}` schema
//! from the fail arm and rejected numeric values Helm renders fine. The
//! stringified arm must stay a partial condition; the unstringified subject
//! keeps its exact guard.

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

const VALUES_YAML: &str = indoc! {"
    port: 8080
    name: app
"};

const TEMPLATE: &str = indoc! {r#"
    {{- if not (regexMatch "^[0-9]+$" (toString .Values.port)) -}}
    {{- fail "bad port" -}}
    {{- end -}}
    {{- if not (regexMatch "^[a-z]+$" .Values.name) -}}
    {{- fail "bad name" -}}
    {{- end -}}
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: {{ .Values.name }}
    data:
      port: {{ .Values.port | quote }}
"#};

fn generated_schema() -> eyre::Result<serde_json::Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, VALUES_YAML)?;
    test_util::write(&chart_dir.join("templates/cm.yaml")?, TEMPLATE)?;

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
fn stringified_subject_admits_every_matching_rendering() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema()?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // `toString 3` is `"3"`, the pattern matches, and rendering succeeds
    // (adjudicated with `helm template`): the raw integer must stay valid.
    // Instances compose over the chart defaults because the schema
    // validates the coalesced document.
    for port in [serde_json::json!(3), serde_json::json!("3")] {
        let instance = serde_json::json!({ "port": port, "name": "app" });
        assert!(
            validator.is_valid(&instance),
            "{instance} renders through `toString` to a matching string; the \
             raw-level string guard must not reject it: {}",
            validator
                .iter_errors(&instance)
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        );
    }

    Ok(())
}

#[test]
fn unstringified_subject_keeps_the_exact_pattern_guard() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema()?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // Without a stringification, `regexMatch` observes the raw string and
    // the fail arm's exact backprojection stands.
    assert!(
        !validator.is_valid(&serde_json::json!({ "name": "Bad-Name-7", "port": 8080 })),
        "a raw string that fails the pattern aborts rendering and must stay rejected",
    );
    let matching = serde_json::json!({ "name": "app", "port": 8080 });
    assert!(
        validator.is_valid(&matching),
        "a matching raw string renders: {}",
        validator
            .iter_errors(&matching)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    );

    Ok(())
}
