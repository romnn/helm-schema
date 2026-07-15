use std::io::Read;
use std::path::PathBuf;

use color_eyre::eyre::{Report, WrapErr};
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

#[allow(
    dead_code,
    reason = "shared integration-test helper; not every test binary reads values.yaml"
)]
pub(crate) fn read_values_yaml_for_path(
    chart_relative_path: &str,
) -> std::result::Result<String, Report> {
    let mut out = String::new();
    chart_dir(chart_relative_path)
        .join("values.yaml")
        .wrap_err("join values.yaml")?
        .open_file()
        .wrap_err("open values.yaml")?
        .read_to_string(&mut out)
        .wrap_err("read values.yaml")?;
    Ok(out)
}

pub fn generate_chart_schema_for_path(
    chart_relative_path: &str,
) -> std::result::Result<Value, Report> {
    let _guard = test_util::builder().with_tracing(false).build();

    let opts = GenerateOptions {
        chart_dir: chart_dir(chart_relative_path),
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        provider: ProviderOptions {
            k8s_versions: vec!["v1.29.0-standalone-strict".to_string()],
            k8s_schema_cache_dir: Some(
                test_util::workspace_root().join(".cache/kubernetes-json-schema-cache"),
            ),
            allow_net: false,
            crd_catalog_cache_dir: Some(
                test_util::workspace_root().join(".cache/crds-catalog-cache"),
            ),
            disable_k8s_schemas: false,
            crd_override_dir: Some(test_util::workspace_root().join(".cache/crds-catalog-cache")),
            ..Default::default()
        },
    };

    AnalysisSession::new(opts)
        .generated_schema()
        .map(|generated| generated.schema)
        .map_err(Report::from)
        .wrap_err("generate schema")
}
