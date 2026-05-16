//! Regression test for over-broadcast type-hints from library subcharts.
//!
//! A library subchart's helper that contains a literal-default expression
//! (`default 5 .Values.replicas`) used to leak that type assertion across
//! every chart in the discovery — including app subcharts that never
//! include the helper. Result: an unrelated app chart's
//! `<app>.replicas` got typed `{type: integer}` solely because a sibling
//! library happened to mention `.Values.replicas` in a helper.
//!
//! Fix: library-helper extracts (both type_hints and fallback_paths) are
//! scoped to the prefixes of charts that actually `include` any of the
//! library's helpers. An unused library — no `include` references found
//! anywhere — contributes nothing.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
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

const WRAPPER_VALUES_YAML: &str = "\
app:
  replicas: ~
";

const LIBRARY_CHART_YAML: &str = "\
apiVersion: v2
name: unused-library
version: 0.1.0
type: library
";

// Library helper that defines a literal-default for .Values.replicas.
// The wrapper never `include`s this helper — but the previous code
// broadcast the type hint anyway.
const LIBRARY_HELPER: &str = "\
{{- define \"unused.replicas\" -}}
{{- default 5 .Values.replicas -}}
{{- end -}}
";

const APP_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
";

// App chart references its own .Values.replicas (which becomes
// .Values.app.replicas in the umbrella's composed scope). It does NOT
// include the library helper above — so the library's literal default
// must not influence the app's schema.
const APP_CONFIGMAP_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app-config
data:
  replicas: \"{{ .Values.replicas }}\"
";

const APP_VALUES_YAML: &str = "\
replicas: ~
";

#[test]
fn library_literal_default_does_not_leak_type_to_sibling_chart() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/unused-library/Chart.yaml")?,
        LIBRARY_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/unused-library/templates/_helpers.tpl")?,
        LIBRARY_HELPER,
    )?;
    test_util::write(&chart_dir.join("charts/app/Chart.yaml")?, APP_CHART_YAML)?;
    test_util::write(&chart_dir.join("charts/app/values.yaml")?, APP_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/app/templates/configmap.yaml")?,
        APP_CONFIGMAP_TEMPLATE,
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

    // Before the fix: app.replicas would be `{"type": "integer"}` —
    // wrongly typed from the unused library's literal default.
    // After the fix: app.replicas should be `{}` — no type signal
    // (the app's own template doesn't assert a type).
    assert_eq!(
        app_replicas,
        &serde_json::json!({}),
        "library-helper literal default leaked across chart boundary: app.replicas = {app_replicas}",
    );

    Ok(())
}
