use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::vyt;
use std::path::PathBuf;
use test_util::prelude::*;
use vfs::VfsPath;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn run_vyt_on_chart_templates(
    chart: &helm_schema_chart::ChartSummary,
) -> eyre::Result<Vec<vyt::VYUse>> {
    let mut defs = vyt::DefineIndex::default();
    for p in &chart.templates {
        let src = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))?;
        vyt::extend_define_index_from_str(&mut defs, &src)
            .wrap_err_with(|| format!("index defines in {}", p.as_str()))?;
    }
    let defs = std::sync::Arc::new(defs);

    let mut all: Vec<vyt::VYUse> = Vec::new();
    for p in &chart.templates {
        let src = p
            .read_to_string()
            .wrap_err_with(|| format!("read template {}", p.as_str()))?;
        let parsed = helm_schema_template::parse::parse_gotmpl_document(&src)
            .ok_or_eyre("template parse returned None")
            .wrap_err_with(|| format!("parse template {}", p.as_str()))?;

        let mut uses = vyt::VYT::new(src)
            .with_defines(std::sync::Arc::clone(&defs))
            .run(&parsed.tree);
        all.append(&mut uses);
    }

    Ok(all)
}

fn path_str(u: &vyt::VYUse) -> String {
    u.path.0.join(".")
}

#[test]
fn signoz_otel_gateway_vyt_ir_phase_assertions() -> eyre::Result<()> {
    Builder::default().build();

    let chart_root_path =
        crate_root().join("testdata/charts/signoz-signoz/charts/signoz-otel-gateway");
    if !chart_root_path.exists() {
        return Ok(());
    }

    let chart_dir = VfsPath::new(vfs::PhysicalFS::new(env!("CARGO_MANIFEST_DIR")))
        .join("testdata/charts/signoz-signoz/charts/signoz-otel-gateway")?;

    let chart = load_chart(
        &chart_dir,
        &LoadOptions {
            include_tests: false,
            recurse_subcharts: false,
            auto_extract_tgz: true,
            respect_gitignore: false,
            include_hidden: false,
            ..Default::default()
        },
    )?;

    let uses = run_vyt_on_chart_templates(&chart)?;

    assert!(
        uses.iter()
            .any(|u| u.source_expr == "nodeSelector" && path_str(u) == "nodeSelector"),
        "expected .Values.nodeSelector to map to YAML path nodeSelector"
    );

    assert!(
        uses.iter().any(|u| u.source_expr == "affinity"
            && path_str(u) == "affinity"
            && u.guards.contains(&"affinity".to_string())),
        "expected .Values.affinity to map to YAML path affinity within with-guard"
    );

    assert!(
        !uses
            .iter()
            .any(|u| u.source_expr == "affinity" && path_str(u) == "nodeSelector"),
        "did not expect .Values.affinity to be attributed to YAML path nodeSelector"
    );

    assert!(
        !uses
            .iter()
            .any(|u| u.source_expr == "nodeSelector" && path_str(u) == "affinity"),
        "did not expect .Values.nodeSelector to be attributed to YAML path affinity"
    );

    assert!(
        uses.iter().any(|u| u.source_expr == "args.*"
            && path_str(u) == "args[*]"
            && u.guards.contains(&"args".to_string())),
        "expected .Values.args to be used in a range and normalized to args.* at YAML path args[*]"
    );

    Ok(())
}
