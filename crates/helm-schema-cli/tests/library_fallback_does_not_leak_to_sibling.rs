//! Regression test for the sibling-chart false-negative under
//! `--infer-required`.
//!
//! Previously, library subcharts broadcast their `default <expr>
//! .Values.X` extracts across every chart prefix in the discovery —
//! including sibling app charts that never `include` any of the
//! library's helpers. With `--infer-required`, this silently
//! suppressed required-inference for unrelated paths: an app chart's
//! unconditional `if .Values.nameOverride` should mark `nameOverride`
//! required, but the library's `default .Chart.Name .Values.nameOverride`
//! (in a completely unused subchart) caused the app's prefix-scoped
//! `<app>.nameOverride` to be excluded.
//!
//! Fix: library extracts are scoped only to the prefixes of charts that
//! actually `include` the library's helpers. Detected by a substring
//! search for `"<library_name>."` in caller charts' template sources.
//! An unused library contributes no extracts.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use serde_json::Value;
use vfs::VfsPath;

const WRAPPER_CHART_YAML: &str = "\
apiVersion: v2
name: wrapper
version: 0.1.0
dependencies:
  - name: unused-library
    version: 0.1.0
  - name: app
    version: 0.1.0
";

const WRAPPER_VALUES_YAML: &str = "{}\n";

const LIBRARY_CHART_YAML: &str = "\
apiVersion: v2
name: unused-library
version: 0.1.0
type: library
";

// Non-literal default in a library helper. Under the old broadcast
// behaviour this added `nameOverride` (and `<app>.nameOverride`,
// `<library>.nameOverride`, …) to the global fallback set, silently
// suppressing required-inference even in charts that don't include the
// helper.
const LIBRARY_NAME_HELPER: &str = "\
{{- define \"unused-library.name\" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix \"-\" -}}
{{- end -}}
";

const APP_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
";

// App template has an UNCONDITIONAL `if .Values.nameOverride` — that's
// the canonical Step-3 \"this path is required\" signal. The app does
// NOT include the library's helper, so the library's default should
// not absolve it.
const APP_TEMPLATE: &str = "\
{{- if .Values.nameOverride }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ .Values.nameOverride }}
{{- end }}
";

const APP_VALUES_YAML: &str = "{}\n";

#[test]
fn library_fallback_does_not_leak_to_sibling_chart_required() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/unused-library/Chart.yaml")?,
        LIBRARY_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/unused-library/templates/_name.tpl")?,
        LIBRARY_NAME_HELPER,
    )?;
    test_util::write(&chart_dir.join("charts/app/Chart.yaml")?, APP_CHART_YAML)?;
    test_util::write(&chart_dir.join("charts/app/values.yaml")?, APP_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/app/templates/cm.yaml")?,
        APP_TEMPLATE,
    )?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: true,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            k8s_schema_cache_dir: None,
            allow_net: false,
            disable_k8s_schemas: true,
            crd_override_dir: None,
            ..Default::default()
        },
    };

    let schema = generate_values_schema_for_chart(&opts)
        .map_err(Report::from)
        .wrap_err("generate schema")?;

    let app_required: Vec<String> = schema
        .pointer("/properties/app/required")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    assert!(
        app_required.contains(&"nameOverride".to_string()),
        "app chart's unconditional `if .Values.nameOverride` should mark \
         `nameOverride` required even though an unused sibling library \
         has a `default <expr> .Values.nameOverride` helper; got \
         app.required = {app_required:?}",
    );

    Ok(())
}
