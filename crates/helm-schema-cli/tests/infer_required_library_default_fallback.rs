//! Regression test for the `--infer-required` × library-helper-non-literal-
//! default interaction.
//!
//! Pattern: a library subchart (`type: library`) defines a helper that uses
//! `default <non-literal-expr> .Values.X`. The consuming chart includes that
//! helper, which resolves `.Values.X` in the consumer's value scope.
//!
//! Helm runtime: `X` being unset is contractually valid — the default fires.
//! So `X` must NOT be marked required under `--infer-required`.
//!
//! Before the fix:
//!   1. CLI skipped library charts entirely for hint/fallback extraction, so
//!      the consumer's `nameOverride` use looked like an unconditional header.
//!   2. The type-hint extractor only recognised LITERAL defaults
//!      (`default "x"`), not `default .Chart.Name`, so `nameOverride` wasn't
//!      flagged as "has a fallback".
//!
//! Driver: the real Temporal chart's `common-0.1.0` library subchart, whose
//! `_name.tpl` uses `default .Chart.Name .Values.nameOverride`. Recreated
//! here as a minimal in-memory fixture.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use serde_json::Value;
use vfs::VfsPath;

const WRAPPER_CHART_YAML: &str = "\
apiVersion: v2
name: wrapper
version: 0.1.0
dependencies:
  - name: common
    version: 0.1.0
";

const WRAPPER_VALUES_YAML: &str = "\
# nameOverride deliberately absent — the chart relies on the library
# helper's `default .Chart.Name` fallback.
{}
";

// Wrapper template that consumes the library helper. This is what causes
// `.Values.nameOverride` to appear in the IR (via helper inlining) with
// empty guards, looking like an unconditional reference.
const WRAPPER_CONFIGMAP_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include \"common.name\" . }}
data: {}
";

const LIBRARY_CHART_YAML: &str = "\
apiVersion: v2
name: common
version: 0.1.0
type: library
";

// The non-literal-default helper. `default .Chart.Name .Values.nameOverride`
// makes `nameOverride` null-tolerant in the consumer's scope, but the
// fallback is a function/identifier expression, not a literal — only the
// broader `extract_default_fallback_paths` extractor catches it.
const LIBRARY_NAME_HELPER: &str = "\
{{- define \"common.name\" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix \"-\" -}}
{{- end -}}
";

#[test]
fn library_helper_non_literal_default_suppresses_required() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("templates/configmap.yaml")?,
        WRAPPER_CONFIGMAP_TEMPLATE,
    )?;
    test_util::write(
        &chart_dir.join("charts/common/Chart.yaml")?,
        LIBRARY_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/common/templates/_name.tpl")?,
        LIBRARY_NAME_HELPER,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: true,
        provider: ProviderOptions {
            k8s_version: "v1.35.0".to_string(),
            k8s_schema_cache_dir: None,
            allow_net: false,
            disable_k8s_schemas: true,
            crd_catalog_dir: None,
        },
    };

    let schema = generate_values_schema_for_chart(&opts)
        .map_err(Report::from)
        .wrap_err("generate schema")?;

    let required: Vec<String> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    assert!(
        !required.contains(&"nameOverride".to_string()),
        "library helper's `default .Chart.Name .Values.nameOverride` must \
         suppress nameOverride from --infer-required; got required={required:?}",
    );

    Ok(())
}
