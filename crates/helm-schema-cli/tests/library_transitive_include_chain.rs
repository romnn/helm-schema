//! Regression test for transitive helper-include chains.
//!
//! Pre-fix behaviour: library-name-level scoping only considered
//! non-library charts when matching callers, so a chain like
//!   `app   --include "libA.X" -->  libA.X`
//!   `libA.X  --include "libB.Y" -->  libB.Y` (libB.Y has `default ... .Values.nameOverride`)
//! never carried libB.Y's signals to the app — libB was scanned for
//! direct callers, found none (only libA includes it), got an empty
//! caller list, and contributed nothing.
//!
//! Fix: the helper call graph is transitive. Reachability is computed
//! by BFS/DFS from each non-library chart's directly-called helpers,
//! following `helper → helper` edges through the library layer.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use serde_json::Value;
use vfs::VfsPath;

const WRAPPER_CHART_YAML: &str = "\
apiVersion: v2
name: wrapper
version: 0.1.0
dependencies:
  - name: liba
    version: 0.1.0
  - name: libb
    version: 0.1.0
  - name: app
    version: 0.1.0
";

const WRAPPER_VALUES_YAML: &str = "\
app: {}
";

const LIBA_CHART_YAML: &str = "\
apiVersion: v2
name: liba
version: 0.1.0
type: library
";

// libA's only helper calls libB's helper.
const LIBA_HELPERS: &str = "\
{{- define \"liba.fullname\" -}}
{{- include \"libb.name\" . -}}
{{- end -}}
";

const LIBB_CHART_YAML: &str = "\
apiVersion: v2
name: libb
version: 0.1.0
type: library
";

// libB's helper carries the `default` signal — `nameOverride` has a
// non-literal fallback through `.Chart.Name`.
const LIBB_HELPERS: &str = "\
{{- define \"libb.name\" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix \"-\" -}}
{{- end -}}
";

const APP_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
";

// App directly includes liba.fullname only. The signal it depends on
// lives in libb.name, two hops away.
const APP_TEMPLATE: &str = "\
{{- if .Values.nameOverride }}
apiVersion: v1
kind: ConfigMap
metadata:
  name: {{ include \"liba.fullname\" . }}
{{- end }}
";

const APP_VALUES_YAML: &str = "\
nameOverride: ~
";

#[test]
fn transitive_library_include_chain_propagates_fallback() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, WRAPPER_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, WRAPPER_VALUES_YAML)?;

    test_util::write(&chart_dir.join("charts/liba/Chart.yaml")?, LIBA_CHART_YAML)?;
    test_util::write(
        &chart_dir.join("charts/liba/templates/_helpers.tpl")?,
        LIBA_HELPERS,
    )?;

    test_util::write(&chart_dir.join("charts/libb/Chart.yaml")?, LIBB_CHART_YAML)?;
    test_util::write(
        &chart_dir.join("charts/libb/templates/_helpers.tpl")?,
        LIBB_HELPERS,
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

    // The app has an UNCONDITIONAL `if .Values.nameOverride`, but the
    // transitively-reachable libB.name helper provides a `default`
    // fallback. So nameOverride should NOT be marked required on the
    // app's prefix.
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
        !app_required.contains(&"nameOverride".to_string()),
        "libb.name provides a default for nameOverride and is transitively reachable \
         from the app via liba.fullname — required-inference should pick this up; \
         got app.required = {app_required:?}",
    );

    Ok(())
}
