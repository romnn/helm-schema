//! Schema inference must not discover API versions from incidental cache contents.

use std::path::Path;

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{AuthoringPolicy, GenerateOptions, SchemaProfile};
use helm_schema::provider::ProviderOptions;
use indoc::{formatdoc, indoc};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use test_util::scratch::ScratchDir;
use vfs::VfsPath;

fn chart_schema(kind: &str, cache: &Path) -> eyre::Result<Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    test_util::write(
        &chart_dir.join("Chart.yaml")?,
        indoc! {"
            apiVersion: v2
            name: cache-inference
            version: 0.1.0
        "},
    )?;
    test_util::write(&chart_dir.join("values.yaml")?, "setting: plain\n")?;
    // An omitted API version exercises the same inference path as an unresolved splice.
    test_util::write(
        &chart_dir.join("templates/resource.yaml")?,
        formatdoc! {"
            kind: {kind}
            metadata:
              name: test
            spec:
              setting: {{{{ .Values.setting }}}}
        "},
    )?;
    Ok(AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::default().into(),
        authoring: AuthoringPolicy::default(),
        provider: ProviderOptions {
            k8s_versions: vec!["v1.35.0".to_string()],
            allow_net: false,
            k8s_schema_cache_dir: Some(cache.join("k8s")),
            crd_catalog_cache_dir: Some(cache.join("crds")),
            api_version_guess: true,
            ..Default::default()
        },
    })
    .generated_schema()?
    .schema)
}

fn assert_cache_independent(kind: &str, cache_file: &str, schema: &Value) -> eyre::Result<()> {
    let cache = ScratchDir::new("api-version-inference-cache")?;
    let cold = chart_schema(kind, cache.path())?;
    let path = cache.path().join(cache_file);
    let parent = path.parent().ok_or_eyre("cache file has no parent")?;
    std::fs::create_dir_all(parent)?;
    std::fs::write(path, serde_json::to_vec(schema)?)?;
    let warm = chart_schema(kind, cache.path())?;

    assert!(cold.pointer("/properties/setting").is_some());
    sim_assert_eq!(have: warm, want: cold);
    Ok(())
}

#[test]
fn unknown_builtin_api_version_is_independent_of_warm_cache() -> eyre::Result<()> {
    assert_cache_independent(
        "PodDisruptionBudget",
        "k8s/default/v1.35.0/poddisruptionbudget-policy-v1.json",
        &json!({
            "type": "object",
            "x-kubernetes-group-version-kind": [
                {"group": "policy", "version": "v1", "kind": "PodDisruptionBudget"}
            ],
            "properties": {"spec": {"type": "object", "properties": {
                "setting": {"type": "string", "enum": ["only"]}
            }}}
        }),
    )
}

#[test]
fn unknown_crd_api_version_is_independent_of_warm_cache() -> eyre::Result<()> {
    assert_cache_independent(
        "Widget",
        "crds/default/example.io/widget_v1.json",
        &json!({
            "type": "object",
            "properties": {"spec": {"type": "object", "properties": {
                "setting": {"type": "string", "enum": ["only"]}
            }}}
        }),
    )
}
