//! Regression test for the "duplicate helper name across libraries"
//! contamination bug. Two library subcharts both define a helper with
//! the same name (`common.name`) but with DIFFERENT bodies. Helm's
//! template engine and our `DefineIndex` both resolve duplicate
//! names with last-write-wins on iteration order — only one body
//! actually executes at render time.
//!
//! Pre-fix the helper call graph concatenated all same-name bodies
//! into a single node, so text-level extractors (type-hint inference,
//! required-inference fallback detection) saw content from defines
//! that Helm itself shadowed and never rendered. That produced phantom
//! signals — a literal `default 5` in the losing body would type-hint
//! `.Values.replicas` as integer even though the winning body never
//! references `.Values.replicas`.
//!
//! Fix: align helper-graph semantics with `DefineIndex` — when the
//! same name appears more than once, the last define inserted wins
//! (its body fully replaces the previous one).
//!
//! Topology:
//!   - library `winner` defines  common.name: `{{ .Chart.Name }}`
//!   - library `loser`  defines  common.name: `{{ default 5 .Values.replicas }}`
//!   - root app includes common.name and references .Values.replicas
//!
//! `read_dir` on `MemoryFS` returns entries alphabetically, so the
//! iteration order is `loser` → `winner`; `winner` wins.
//!
//! Expected (matching what Helm actually renders): root.replicas has
//! no integer type hint — the `default 5 .Values.replicas` body is
//! shadowed and never executes.

use color_eyre::eyre::{Report, WrapErr};
use helm_schema_cli::{GenerateOptions, ProviderOptions, generate_values_schema_for_chart};
use vfs::VfsPath;

const ROOT_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
dependencies:
  - name: loser
    version: 0.1.0
  - name: winner
    version: 0.1.0
";

const ROOT_VALUES_YAML: &str = "\
replicas: ~
";

const WINNER_CHART_YAML: &str = "\
apiVersion: v2
name: winner
version: 0.1.0
type: library
";

const WINNER_HELPERS: &str = "\
{{- define \"common.name\" -}}
{{ .Chart.Name }}
{{- end -}}
";

const LOSER_CHART_YAML: &str = "\
apiVersion: v2
name: loser
version: 0.1.0
type: library
";

const LOSER_HELPERS: &str = "\
{{- define \"common.name\" -}}
{{- default 5 .Values.replicas -}}
{{- end -}}
";

const ROOT_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
  labels:
    name: \"{{ include \"common.name\" . }}\"
data:
  replicas: \"{{ .Values.replicas }}\"
";

#[test]
fn duplicate_helper_name_losing_body_does_not_contaminate_type_hints()
-> color_eyre::eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build();

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, ROOT_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, ROOT_VALUES_YAML)?;

    test_util::write(
        &chart_dir.join("charts/winner/Chart.yaml")?,
        WINNER_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/winner/templates/_helpers.tpl")?,
        WINNER_HELPERS,
    )?;

    test_util::write(
        &chart_dir.join("charts/loser/Chart.yaml")?,
        LOSER_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/loser/templates/_helpers.tpl")?,
        LOSER_HELPERS,
    )?;

    test_util::write(&chart_dir.join("templates/cm.yaml")?, ROOT_TEMPLATE)?;

    let opts = GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        infer_required: false,
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

    let replicas = schema
        .pointer("/properties/replicas")
        .expect("/properties/replicas present");

    assert_eq!(
        replicas,
        &serde_json::json!({}),
        "losing-define body's integer literal leaked into root.replicas \
         even though AST last-write-wins discards that body; got: {replicas}",
    );

    Ok(())
}
