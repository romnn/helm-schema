//! Regression test for the "quoted-string false helper-edge"
//! contamination bug. A manifest action like
//! `{{ "include \"common.unusedReplicas\"" | quote }}` contains the
//! literal text `include "common.unusedReplicas"` inside a Go string
//! literal — that's a quoted PAYLOAD, not a real helper call. Helm
//! never invokes `common.unusedReplicas` from that action.
//!
//! Pre-fix `extract_helper_calls` regex-scanned the action body for
//! `include "..."` / `template "..."` without first skipping over Go
//! string literals, so it manufactured a phantom edge from the
//! consumer chart to `common.unusedReplicas`. That fake reachability
//! made the unused helper's `default 5 .Values.replicas` body feed
//! type-hint extraction at the consumer's prefix — leaking an integer
//! type hint onto `.Values.replicas`.
//!
//! Fix: the helper-call regex now skips Go string literals (both
//! `"..."` with `\"` escapes and `` `...` `` backtick raw strings) via
//! alternation, so the bytes inside a string can no longer match the
//! `include "name"` / `template "name"` pattern.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const ROOT_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
dependencies:
  - name: common
    version: 0.1.0
";

const ROOT_VALUES_YAML: &str = "\
replicas: ~
";

const LIBRARY_CHART_YAML: &str = "\
apiVersion: v2
name: common
version: 0.1.0
type: library
";

// `common.used` is the only helper actually included by the app.
// `common.unusedReplicas` has a literal default that would leak via
// the helper-call false edge if the regex didn't skip string literals.
const LIBRARY_HELPERS: &str = "\
{{- define \"common.used\" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
{{- end -}}

{{- define \"common.unusedReplicas\" -}}
{{- default 5 .Values.replicas -}}
{{- end -}}
";

// The manifest contains a quoted PAYLOAD that includes the literal
// text `include "common.unusedReplicas"`. This is not a real call —
// at render time Helm pipes that string through `quote` and emits it
// verbatim. The real helper call is `{{ include "common.used" . }}`.
const ROOT_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
  labels:
    {{- include \"common.used\" . | nindent 4 }}
data:
  payload: {{ \"include \\\"common.unusedReplicas\\\"\" | quote }}
  replicas: \"{{ .Values.replicas }}\"
";

#[test]
fn quoted_string_payload_does_not_create_phantom_helper_edge() -> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, ROOT_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, ROOT_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/common/Chart.yaml")?,
        LIBRARY_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/common/templates/_helpers.tpl")?,
        LIBRARY_HELPERS,
    )?;
    test_util::write(&chart_dir.join("templates/cm.yaml")?, ROOT_TEMPLATE)?;

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

    let schema = AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(Report::from)
        .wrap_err("generate schema")?;

    let replicas = schema
        .pointer("/properties/replicas")
        .expect("/properties/replicas present");

    sim_assert_eq!(
        have: replicas,
        want: &serde_json::json!({}),
        "quoted-string payload made common.unusedReplicas look \
         reachable; its literal default leaked into root.replicas \
         as: {replicas}",
    );

    Ok(())
}
