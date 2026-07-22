//! Regression test for values-program wrapper SCOPING under parent
//! composition. A subchart's engine (nats' `$tplYaml`) rewrites the
//! SUBCHART's values root, which the parent sees as its `sub` subtree —
//! an empty chart-local scope must compose to the subchart prefix, not
//! re-root at the parent document. Re-rooting once moved the parent's
//! `properties`/`additionalProperties` into a wrapped `anyOf` arm, where
//! later root-level transforms (override merges, the global mirror)
//! could no longer see them, falsely rejecting injected top-level keys.

use color_eyre::eyre::{self, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const ROOT_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
dependencies:
  - name: sub
    version: 0.1.0
";

const ROOT_VALUES_YAML: &str = "\
port: 4222
";

const ROOT_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: app
data:
  port: {{ .Values.port }}
";

const SUB_CHART_YAML: &str = "\
apiVersion: v2
name: sub
version: 0.1.0
";

const SUB_VALUES_YAML: &str = "\
port: 4222
";

const SUB_ENGINE_HELPERS: &str = r#"
{{- define "sub.tplValues" -}}
{{- $doc := .doc -}}
{{- if and (eq (kindOf $doc) "map") (eq (len $doc) 1) (hasKey $doc "$tplYaml") -}}
{{- $tpl := get $doc "$tplYaml" -}}
{{- toJson (dict "doc" (fromYaml (tpl $tpl .ctx))) -}}
{{- else -}}
{{- toJson (dict "doc" $doc) -}}
{{- end -}}
{{- end -}}
"#;

const SUB_TEMPLATE: &str = "\
apiVersion: v1
kind: ConfigMap
metadata:
  name: sub
data:
  port: {{ .Values.port }}
{{- $values := get (include \"sub.tplValues\" (dict \"doc\" .Values \"ctx\" $) | fromJson) \"doc\" }}
{{- $_ := set . \"Values\" $values }}
";

#[test]
fn subchart_wrapper_engine_scopes_to_its_values_prefix() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;

    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, ROOT_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, ROOT_VALUES_YAML)?;
    test_util::write(&chart_dir.join("templates/cm.yaml")?, ROOT_TEMPLATE)?;
    test_util::write(&chart_dir.join("charts/sub/Chart.yaml")?, SUB_CHART_YAML)?;
    test_util::write(&chart_dir.join("charts/sub/values.yaml")?, SUB_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/sub/templates/_helpers.tpl")?,
        SUB_ENGINE_HELPERS,
    )?;
    test_util::write(
        &chart_dir.join("charts/sub/templates/cm.yaml")?,
        SUB_TEMPLATE,
    )?;

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
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")?;

    // The parent document root must keep its base properties tree at the
    // top level: later root-level transforms deep-merge into
    // `/properties`, and a wrapped root would hide the sibling
    // `additionalProperties: false` from what they add.
    sim_assert_eq!(
        have: schema.get("anyOf"),
        want: None::<&serde_json::Value>,
        "the subchart's engine must not wrap the PARENT document root: {schema}",
    );
    assert!(
        schema.pointer("/properties/port").is_some() && schema.pointer("/properties/sub").is_some(),
        "parent root keys stay at the top level: {schema}"
    );

    let validator = jsonschema::validator_for(&schema).expect("schema validator");
    for (instance, want) in [
        // The engine rewrites the SUBCHART's values, so the wrapper
        // alternative exists under `sub.*` ...
        (
            serde_json::json!({ "port": 1, "sub": { "port": { "$tplYaml": "{{ add 1 2 }}" } } }),
            true,
        ),
        // ... and nowhere else: the parent's identically-shaped key keeps
        // rejecting the raw sentinel map,
        (
            serde_json::json!({ "port": { "$tplYaml": "{{ add 1 2 }}" }, "sub": { "port": 1 } }),
            false,
        ),
        // and the parent root is not the engine's recursion root either.
        (serde_json::json!({ "$tplYaml": "port: 1" }), false),
        (serde_json::json!({ "port": 1, "sub": { "port": 1 } }), true),
    ] {
        assert!(
            validator.is_valid(&instance) == want,
            "wrapper alternatives follow the engine's OWN values root: \
             instance={instance}; want={want}; schema={schema}"
        );
    }

    Ok(())
}
