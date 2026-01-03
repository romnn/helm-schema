use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::vyt;
use test_util::prelude::*;
use vfs::VfsPath;

fn chart_root(root: &VfsPath, name: &str) -> eyre::Result<VfsPath> {
    root.join("crates/helm-schema-mapper/testdata/charts")?
        .join(name)
        .map_err(Into::into)
}

fn assert_template_parses(path: &VfsPath, src: &str) -> eyre::Result<()> {
    let Some(_) = helm_schema_template::parse::parse_gotmpl_document(src) else {
        return Err(eyre::eyre!("failed to parse template {}", path.as_str()));
    };
    Ok(())
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

fn uses_contain(uses: &[vyt::VYUse], vp: &str) -> bool {
    uses.iter().any(|u| u.source_expr == vp)
}

#[test]
fn upstream_bitnami_redis_all_templates_parse() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, "bitnami-redis")?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;

    let mut failed: Vec<String> = Vec::new();
    for p in &chart.templates {
        let Ok(src) = p.read_to_string() else {
            failed.push(format!("read failed: {}", p.as_str()));
            continue;
        };
        if let Err(e) = assert_template_parses(p, &src) {
            failed.push(format!("{e:?}"));
        }
    }

    if !failed.is_empty() {
        return Err(eyre::eyre!(
            "one or more templates failed to parse ({}):\n{}",
            failed.len(),
            failed.join("\n")
        ));
    }

    Ok(())
}

#[test]
fn upstream_cert_manager_all_templates_parse() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, "cert-manager")?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;

    let mut failed: Vec<String> = Vec::new();
    for p in &chart.templates {
        let Ok(src) = p.read_to_string() else {
            failed.push(format!("read failed: {}", p.as_str()));
            continue;
        };
        if let Err(e) = assert_template_parses(p, &src) {
            failed.push(format!("{e:?}"));
        }
    }

    if !failed.is_empty() {
        return Err(eyre::eyre!(
            "one or more templates failed to parse ({}):\n{}",
            failed.len(),
            failed.join("\n")
        ));
    }

    Ok(())
}

#[test]
fn upstream_bitnami_redis_vyt_ir_sanity() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, "bitnami-redis")?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;

    assert!(
        uses.len() >= 50,
        "expected substantial VYT IR for redis chart, got {}",
        uses.len()
    );

    assert!(uses_contain(&uses, "auth.enabled"));

    Ok(())
}

#[test]
fn upstream_cert_manager_vyt_ir_sanity() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, "cert-manager")?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;

    assert!(
        uses.len() >= 50,
        "expected substantial VYT IR for cert-manager chart, got {}",
        uses.len()
    );

    assert!(uses_contain(&uses, "installCRDs"));

    Ok(())
}
