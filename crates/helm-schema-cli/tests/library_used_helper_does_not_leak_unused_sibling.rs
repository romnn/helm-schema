//! Regression test for the "used library with unused helper" leak.
//!
//! Pre-fix behaviour: library-name-level scoping meant that ANY signal
//! extracted from a library's helpers was scoped to every caller of
//! that library — even when the specific helper carrying the signal
//! was never included. So a library `common` with two defines:
//!   `common.used`         — the only helper the app includes
//!   `common.unused`       — contains `default 5 .Values.replicas`
//! would incorrectly type `app.replicas` as integer (and exclude it
//! from `--infer-required`) just because `common` as a whole was used.
//!
//! Fix: helper-granular call graph. Signals are stored per-define, and
//! reachability is computed transitively from each chart's direct
//! include set. An unused helper inside a used library contributes
//! nothing.

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
  - name: app
    version: 0.1.0
";

const WRAPPER_VALUES_YAML: &str = "\
app: {}
";

const LIBRARY_CHART_YAML: &str = "\
apiVersion: v2
name: common
version: 0.1.0
type: library
";

// Two defines in one library:
//   - `common.labels`       gets included by the app
//   - `common.unusedReplicas` is NEVER included by anyone but carries
//     a literal default that the previous code would have leaked
const LIBRARY_HELPERS: &str = "\
{{- define \"common.labels\" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
{{- end -}}

{{- define \"common.unusedReplicas\" -}}
{{- default 5 .Values.replicas -}}
{{- end -}}
";

const APP_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
";

// App includes only `common.labels` — never `common.unusedReplicas`.
// It references `.Values.replicas` directly. The unused helper's
// literal default must not influence the app's `replicas` schema.
const APP_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
  labels:
    {{- include \"common.labels\" . | nindent 4 }}
data:
  replicas: \"{{ .Values.replicas }}\"
";

const APP_VALUES_YAML: &str = "\
replicas: ~
";

#[test]
fn unused_helper_in_used_library_does_not_leak_type_hint() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/common/Chart.yaml")?,
        LIBRARY_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/common/templates/_helpers.tpl")?,
        LIBRARY_HELPERS,
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
        infer_required: false,
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

    let app_replicas = schema
        .pointer("/properties/app/properties/replicas")
        .expect("/properties/app/properties/replicas present");

    // Before the fix: app.replicas would be a nullable-integer union
    // courtesy of the unused `common.unusedReplicas` helper.
    // After the fix: app.replicas should be `{}` — the app's own
    // template asserts nothing, and the unused helper contributes
    // nothing because it's never reachable from the app's includes.
    assert_eq!(
        app_replicas,
        &serde_json::json!({}),
        "unused-helper signal leaked across helper boundary: app.replicas = {app_replicas}",
    );

    Ok(())
}

#[test]
fn unused_helper_in_used_library_does_not_suppress_required() -> color_eyre::eyre::Result<()> {
    // Same wrapper as above; this time exercise the --infer-required
    // axis. The unused helper has `default 5 .Values.replicas`, which
    // is also a fallback-path signal. With helper-granular scoping,
    // that fallback should NOT exclude app.replicas from required.

    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/common/Chart.yaml")?,
        LIBRARY_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/common/templates/_helpers.tpl")?,
        LIBRARY_HELPERS,
    )?;
    test_util::write(&chart_dir.join("charts/app/Chart.yaml")?, APP_CHART_YAML)?;
    test_util::write(&chart_dir.join("charts/app/values.yaml")?, APP_VALUES_YAML)?;

    // App with an UNCONDITIONAL `if .Values.replicas` — the canonical
    // Step-3 "required" signal. The app does NOT include
    // `common.unusedReplicas`, so its default shouldn't absolve.
    let app_template_with_if = "\
{{- if .Values.replicas }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
  labels:
    {{- include \"common.labels\" . | nindent 4 }}
data:
  replicas: \"{{ .Values.replicas }}\"
{{- end }}
";
    test_util::write(
        &chart_dir.join("charts/app/templates/cm.yaml")?,
        app_template_with_if,
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
        app_required.contains(&"replicas".to_string()),
        "app's `if .Values.replicas` should mark replicas required even though \
         a never-included sibling helper has a `default 5 .Values.replicas`; \
         got app.required = {app_required:?}",
    );

    Ok(())
}
