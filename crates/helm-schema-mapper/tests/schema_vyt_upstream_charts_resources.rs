use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::schema::{UpstreamK8sSchemaProvider, VytSchemaProvider};
use helm_schema_mapper::vyt;
use helm_schema_mapper::vyt::{ResourceRef, VYKind, VYUse, YPath};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;
use test_util::prelude::*;
use vfs::VfsPath;

fn chart_root(root: &VfsPath, name: &str) -> eyre::Result<VfsPath> {
    root.join("crates/helm-schema-mapper/testdata/charts")?
        .join(name)
        .map_err(Into::into)
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

fn cache_dir_default_like_provider() -> PathBuf {
    if let Ok(p) = std::env::var("HELM_SCHEMA_K8S_SCHEMA_CACHE") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg)
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".cache")
            .join("helm-schema")
            .join("kubernetes-json-schema");
    }
    PathBuf::from(".cache")
        .join("helm-schema")
        .join("kubernetes-json-schema")
}

fn schema_allows_type(node: &Value, ty: &str) -> bool {
    if node.get("type").and_then(|v| v.as_str()) == Some(ty) {
        return true;
    }
    if let Some(any_of) = node.get("anyOf").and_then(|v| v.as_array()) {
        return any_of
            .iter()
            .any(|s| s.get("type").and_then(|v| v.as_str()) == Some(ty));
    }
    false
}

fn assert_resource_detection_for_chart(
    chart_dir_name: &str,
) -> eyre::Result<BTreeSet<ResourceRef>> {
    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, chart_dir_name)?;
    if !chart_dir.exists()? {
        return Ok(BTreeSet::new());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;

    let resources: BTreeSet<ResourceRef> = uses.iter().filter_map(|u| u.resource.clone()).collect();

    assert!(
        resources.len() >= 3,
        "expected to detect at least 3 resources in {chart_dir_name}, got {}: {:?}",
        resources.len(),
        resources
    );

    Ok(resources)
}

#[test]
fn upstream_bitnami_redis_detects_kinds() -> eyre::Result<()> {
    Builder::default().build();

    let resources = assert_resource_detection_for_chart("bitnami-redis")?;
    if resources.is_empty() {
        return Ok(());
    }

    let kinds: BTreeSet<&str> = resources.iter().map(|r| r.kind.as_str()).collect();
    assert!(
        kinds.contains("Deployment") || kinds.contains("StatefulSet"),
        "expected redis chart to contain a workload resource (Deployment/StatefulSet), got kinds: {:?}",
        kinds
    );

    Ok(())
}

#[test]
fn upstream_cert_manager_detects_kinds() -> eyre::Result<()> {
    Builder::default().build();

    let resources = assert_resource_detection_for_chart("cert-manager")?;
    if resources.is_empty() {
        return Ok(());
    }

    let kinds: BTreeSet<&str> = resources.iter().map(|r| r.kind.as_str()).collect();
    assert!(
        kinds.contains("Deployment"),
        "expected cert-manager chart to contain Deployment, got kinds: {:?}",
        kinds
    );

    Ok(())
}

#[test]
fn upstream_provider_can_use_detected_deployment_resource_when_cache_available() -> eyre::Result<()>
{
    Builder::default().build();

    let resources = assert_resource_detection_for_chart("cert-manager")?;
    if resources.is_empty() {
        return Ok(());
    }

    let Some(deploy) = resources
        .iter()
        .find(|r| r.kind == "Deployment" && r.api_version == "apps/v1")
        .cloned()
    else {
        return Ok(());
    };

    let version_dir = "v1.29.0-standalone-strict";
    let cache_dir = cache_dir_default_like_provider();

    let allow_download = std::env::var("HELM_SCHEMA_ALLOW_NET")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let has_cached = cache_dir
        .join(version_dir)
        .join("deployment-apps-v1.json")
        .exists();

    if !allow_download && !has_cached {
        return Ok(());
    }

    let provider = UpstreamK8sSchemaProvider::new(version_dir).with_allow_download(allow_download);

    let u = VYUse {
        source_expr: "dummy".to_string(),
        path: YPath(vec![
            "spec".into(),
            "template".into(),
            "spec".into(),
            "containers[*]".into(),
            "securityContext".into(),
            "runAsNonRoot".into(),
        ]),
        kind: VYKind::Scalar,
        guards: vec![],
        resource: Some(deploy),
    };

    let schema = provider
        .schema_for_use(&u)
        .ok_or_eyre("expected upstream schema lookup to return Some")?;

    assert!(
        schema_allows_type(&schema, "boolean"),
        "expected boolean schema for runAsNonRoot, got: {schema:?}"
    );

    Ok(())
}
