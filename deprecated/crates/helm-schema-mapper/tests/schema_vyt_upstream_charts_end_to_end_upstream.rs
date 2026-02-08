use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use helm_schema_mapper::vyt;
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

fn json_pointer_escape(seg: &str) -> String {
    seg.replace('~', "~0").replace('/', "~1")
}

fn schema_node_for_value_path<'a>(schema: &'a Value, vp: &str) -> Option<&'a Value> {
    let mut node = schema;
    for part in vp.split('.') {
        if part == "*" {
            node = node.pointer("/items")?;
            continue;
        }
        let esc = json_pointer_escape(part);
        node = node.pointer(&format!("/properties/{esc}"))?;
    }
    Some(node)
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

fn filename_for_resource(api_version: &str, kind: &str) -> String {
    let kind = kind.to_ascii_lowercase();
    let (group, version) = match api_version.split_once('/') {
        Some((g, v)) => (g.to_ascii_lowercase(), v.to_ascii_lowercase()),
        None => ("".to_string(), api_version.to_ascii_lowercase()),
    };

    if group.is_empty() {
        format!("{}-{}.json", kind, version)
    } else {
        let group = group.replace('.', "-");
        format!("{}-{}-{}.json", kind, group, version)
    }
}

fn assert_end_to_end_security_context_booleans(chart_dir_name: &str) -> eyre::Result<()> {
    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, chart_dir_name)?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let allow_download = std::env::var("HELM_SCHEMA_ALLOW_NET")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let version_dir = "v1.29.0-standalone-strict";
    let cache_dir = cache_dir_default_like_provider();

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    let wanted_last: BTreeSet<&str> = [
        "runAsNonRoot",
        "allowPrivilegeEscalation",
        "readOnlyRootFilesystem",
        "privileged",
    ]
    .into_iter()
    .collect();

    let mut candidates: Vec<&vyt::VYUse> = Vec::new();
    for u in &uses {
        if u.kind != vyt::VYKind::Scalar {
            continue;
        }
        let Some(resource) = u.resource.as_ref() else {
            continue;
        };
        if !u.path.0.iter().any(|s| s == "securityContext") {
            continue;
        }
        let Some(last) = u.path.0.last().map(|s| s.as_str()) else {
            continue;
        };
        if !wanted_last.contains(last) {
            continue;
        }

        if !allow_download {
            let fname = filename_for_resource(&resource.api_version, &resource.kind);
            if !cache_dir.join(version_dir).join(&fname).exists() {
                continue;
            }
        }

        candidates.push(u);
    }

    if candidates.is_empty() {
        return Ok(());
    }

    // Require that at least one candidate gets a boolean type in the final values schema.
    // Without upstream schemas, fallback inference would usually type these as string.
    let mut ok = false;
    let mut sample: Option<(String, String, Value)> = None;

    for u in candidates {
        let Some(node) = schema_node_for_value_path(&schema, &u.source_expr) else {
            continue;
        };
        if schema_allows_type(node, "boolean") {
            ok = true;
            break;
        }
        sample = Some((u.source_expr.clone(), u.path.0.join("."), node.clone()));
    }

    if !ok {
        return Err(eyre::eyre!(
            "expected at least one {chart_dir_name} securityContext boolean field to be typed as boolean via upstream schemas; sample: {:?}",
            sample
        ));
    }

    Ok(())
}

#[test]
fn upstream_bitnami_redis_end_to_end_security_context_booleans() -> eyre::Result<()> {
    Builder::default().build();
    assert_end_to_end_security_context_booleans("bitnami-redis")
}

#[test]
fn upstream_cert_manager_end_to_end_security_context_booleans() -> eyre::Result<()> {
    Builder::default().build();
    assert_end_to_end_security_context_booleans("cert-manager")
}
