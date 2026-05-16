//! Regression test for the "unused-sibling helper" leak when the
//! consumer is the ROOT chart. The companion test
//! `library_used_helper_does_not_leak_unused_sibling.rs` exercises
//! the same guarantee under a wrapper subchart (where
//! `.Values.replicas` resolves to `app.replicas`); this variant puts
//! the consumer at the root so `.Values.replicas` resolves to a
//! top-level `replicas`, exercising the empty-prefix branch of
//! helper-call-graph scoping.
//!
//! Both shapes must keep the unused-sibling helper from leaking its
//! literal default into the consumer's type hints.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
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

const LIBRARY_HELPERS: &str = "\
{{- define \"common.used\" -}}
app.kubernetes.io/name: {{ .Chart.Name }}
{{- end -}}

{{- define \"common.unusedReplicas\" -}}
{{- default 5 .Values.replicas -}}
{{- end -}}
";

const ROOT_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
  labels:
    {{- include \"common.used\" . | nindent 4 }}
data:
  replicas: \"{{ .Values.replicas }}\"
";

#[test]
fn unused_sibling_does_not_leak_when_consumer_is_root_chart() -> color_eyre::eyre::Result<()> {
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

    let replicas = schema
        .pointer("/properties/replicas")
        .expect("/properties/replicas present");

    assert_eq!(
        replicas,
        &serde_json::json!({}),
        "unused-sibling-helper integer hint leaked into root.replicas \
         even though the app only includes common.used; got: {replicas}",
    );

    Ok(())
}
