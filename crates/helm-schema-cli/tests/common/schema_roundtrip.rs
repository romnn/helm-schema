use std::path::PathBuf;

use color_eyre::eyre::{self, WrapErr};
use helm_schema::AnalysisSession;
use helm_schema_cli::{GenerateOptions, ProviderOptions};
use serde_json::Value;
use vfs::VfsPath;

pub(crate) fn physical_chart_dir(chart_relative_path: &str) -> PathBuf {
    test_util::workspace_testdata()
        .join("charts")
        .join(chart_relative_path)
}

fn chart_dir(chart_relative_path: &str) -> VfsPath {
    let chart_dir = physical_chart_dir(chart_relative_path);
    let chart_dir_str = chart_dir.to_string_lossy().to_string();
    VfsPath::new(vfs::PhysicalFS::new(&chart_dir_str))
}

pub fn generate_chart_schema_for_path(chart_relative_path: &str) -> eyre::Result<Value> {
    let _guard = test_util::builder().with_tracing(false).build()?;

    let opts = GenerateOptions {
        chart_dir: chart_dir(chart_relative_path),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            // Provider availability is a deterministic test INPUT: the
            // committed bundle under testdata/ pins exactly which upstream
            // schemas generation can see, so cold and warm runs produce the
            // same fixtures. The gitignored .cache/ dirs must not be used
            // here — an empty cache silently strips provider-backed facts
            // from every expected schema.
            k8s_schema_cache_dir: Some(
                test_util::workspace_testdata()
                    .join("provider-bundle/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            crd_catalog_cache_dir: Some(
                test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            ),
            disable_k8s_schemas: false,
            crd_override_dir: Some(
                test_util::workspace_testdata().join("provider-bundle/crds-catalog-cache"),
            ),
            ..Default::default()
        },
    };

    AnalysisSession::new(opts)
        .generated_schema()
        // Mirror the CLI's default output policy: repeated schema subtrees
        // are interned into root-level `$defs` before anything ships.
        .map(|generated| json_schema_minify::minimize_schema(generated.schema))
        .map_err(eyre::Report::from)
        .wrap_err("generate schema")
}
