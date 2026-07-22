//! Regression test for the map-coalescing soundness gap in fail-validator
//! encoding.
//!
//! `{{- if not .Values.global -}}{{- fail … -}}{{- end -}}` under a chart
//! whose `values.yaml` declares a NON-EMPTY mapping default for `global`
//! can only abort rendering when the user replaces the mapping with a falsy
//! scalar or deletes it via explicit `null`: Helm coalesces mapping values
//! with the declared mapping default instead of replacing it, so any object
//! the user writes (even `{}`) merges into the default and renders truthy.
//!
//! The encoded terminal clause used to test the LITERAL document value for
//! truthiness, so `global: {}` matched "present and falsy" and the schema
//! rejected a values document the chart renders fine.

use color_eyre::eyre::{self, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use vfs::VfsPath;

const CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
";

const VALUES_YAML: &str = "\
global:
  imageRegistry: \"\"
name: app
";

const TEMPLATE: &str = "\
{{- if not .Values.global -}}
{{- fail \"global context lost\" -}}
{{- end -}}
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ .Values.name }}
";

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
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            crd_catalog_cache_dir: Some(
                test_util::workspace_root().join(".cache/crds-catalog-cache"),
            ),
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
fn empty_mapping_override_of_nonempty_mapping_default_stays_valid() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema()?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // `global: {}` merges into the declared non-empty mapping default and
    // renders truthy, so the fail branch never fires.
    let empty_mapping = serde_json::json!({ "global": {} });
    assert!(
        validator.is_valid(&empty_mapping),
        "`global: {{}}` coalesces with the non-empty mapping default and renders \
         truthy; the fail validator must not reject it: {}",
        validator
            .iter_errors(&empty_mapping)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    );

    // A falsy non-mapping override REPLACES the default, renders falsy, and
    // genuinely aborts rendering: the validator must still reject it.
    assert!(
        !validator.is_valid(&serde_json::json!({ "global": "" })),
        "a falsy scalar override of `global` aborts rendering and must stay rejected",
    );

    Ok(())
}
