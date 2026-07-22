//! Regression test for `fail` validators lifted from a CONDITIONAL
//! dependency.
//!
//! A dependency gated off by `condition:` in the parent `Chart.yaml` never
//! renders, so its `fail` branches cannot abort rendering. The lifted fail
//! captures used to lose that activation predicate (it was conjoined onto
//! contract rows but not onto fail conditions), so a bitnami-style password
//! validator inside a DISABLED subchart still became an ungated
//! `if … then false` clause that rejected the parent chart's own defaults.

use color_eyre::eyre::{self, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use vfs::VfsPath;

const ROOT_CHART_YAML: &str = "\
apiVersion: v2
name: app
version: 0.1.0
dependencies:
  - name: redis
    version: 0.1.0
    condition: redis.enabled
";

const ROOT_VALUES_YAML: &str = "\
redis:
  enabled: false
";

const REDIS_CHART_YAML: &str = "\
apiVersion: v2
name: redis
version: 0.1.0
";

const REDIS_VALUES_YAML: &str = "\
auth:
  enabled: true
  usePassword: true
";

const REDIS_TEMPLATE: &str = "\
{{- if and .Values.auth.enabled .Values.auth.usePassword -}}
{{- fail \"auth.enabled and auth.usePassword are mutually exclusive\" -}}
{{- end -}}
apiVersion: v1
kind: ConfigMap
metadata:
  name: redis
";

fn generated_schema() -> eyre::Result<serde_json::Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(&chart_dir.join("Chart.yaml")?, ROOT_CHART_YAML)?;
    test_util::write(&chart_dir.join("values.yaml")?, ROOT_VALUES_YAML)?;
    test_util::write(
        &chart_dir.join("charts/redis/Chart.yaml")?,
        REDIS_CHART_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/redis/values.yaml")?,
        REDIS_VALUES_YAML,
    )?;
    test_util::write(
        &chart_dir.join("charts/redis/templates/cm.yaml")?,
        REDIS_TEMPLATE,
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

    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")
}

#[test]
fn fail_validator_from_disabled_dependency_does_not_reject_defaults() -> eyre::Result<()> {
    let _guard = test_util::builder().with_tracing(false).build()?;
    let schema = generated_schema()?;
    let validator = jsonschema::validator_for(&schema).expect("schema validator");

    // With the dependency disabled (its `condition:` default is false), the
    // subchart never renders and its fail branch cannot fire — even though
    // the subchart's own defaults satisfy the failing test.
    let defaults = serde_json::json!({ "redis": { "enabled": false } });
    assert!(
        validator.is_valid(&defaults),
        "the redis dependency is disabled, so its fail validator must not \
         reject the parent defaults: {}",
        validator
            .iter_errors(&defaults)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("; "),
    );

    // Enabling the dependency activates the validator: the subchart's
    // defaults satisfy the failing test, so rendering aborts and the
    // document must be rejected.
    assert!(
        !validator.is_valid(&serde_json::json!({ "redis": { "enabled": true } })),
        "with the dependency enabled, the subchart's defaults satisfy the \
         failing test and the document must be rejected",
    );

    Ok(())
}
