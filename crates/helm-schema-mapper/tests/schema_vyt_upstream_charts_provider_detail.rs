use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use helm_schema_mapper::vyt;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use test_util::prelude::*;
use vfs::VfsPath;

fn chart_root(root: &VfsPath, name: &str) -> eyre::Result<VfsPath> {
    root.join("crates/helm-schema-mapper/testdata/charts")?
        .join(name)
        .map_err(Into::into)
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

fn is_string_map_schema(node: &Value) -> bool {
    if node.get("type").and_then(|v| v.as_str()) != Some("object") {
        return false;
    }
    let Some(ap) = node.get("additionalProperties") else {
        return false;
    };
    schema_allows_type(ap, "string")
}

fn ypath_pattern(path: &vyt::YPath) -> String {
    path.0.join(".")
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

fn collect_value_paths_by_ypath<'a>(
    uses: &'a [vyt::VYUse],
    wanted_ypaths: &[&str],
) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
    let wanted: BTreeSet<&str> = wanted_ypaths.iter().copied().collect();
    let mut out: BTreeMap<&'a str, BTreeSet<&'a str>> = BTreeMap::new();

    for u in uses {
        let pat = ypath_pattern(&u.path);
        if wanted.contains(pat.as_str()) {
            out.entry(Box::leak(pat.into_boxed_str()))
                .or_default()
                .insert(u.source_expr.as_str());
        }
    }

    out
}

fn assert_provider_detail_for_chart(chart_dir_name: &str) -> eyre::Result<()> {
    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, chart_dir_name)?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // These are CommonK8sSchema patterns we want to see come through as detailed types.
    let expectations: Vec<(&str, &str)> = vec![
        ("spec.template.spec.containers.*.image", "string"),
        ("spec.template.spec.containers.*.name", "string"),
        ("spec.template.spec.containers.*.ports.*.containerPort", "integer"),
        ("spec.template.spec.tolerations.*.key", "string"),
        ("spec.template.spec.tolerations.*.tolerationSeconds", "integer"),
        ("spec.ports.*.port", "integer"),
        ("spec.ports.*.targetPort", "integer"),
        ("metadata.annotations", "string_map"),
        ("metadata.labels", "string_map"),
    ];

    let wanted: Vec<&str> = expectations.iter().map(|(p, _)| *p).collect();
    let by_pat = collect_value_paths_by_ypath(&uses, &wanted);

    // We require that the chart hits at least N of these patterns (chart version drift safe),
    // AND that at least one `.Values.*` path for those patterns is typed correctly.
    let mut patterns_seen = 0usize;
    let mut patterns_typed_ok = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    let mut bad: Vec<(&str, &str, String)> = Vec::new();

    for (pat, expected) in expectations {
        let Some(vps) = by_pat.get(pat) else {
            missing.push(pat);
            continue;
        };
        patterns_seen += 1;

        // If any one vp is typed correctly, consider the pattern satisfied.
        let mut ok_for_pat = false;
        for vp in vps {
            let Some(node) = schema_node_for_value_path(&schema, vp) else {
                continue;
            };
            let ok = match expected {
                "string" => schema_allows_type(node, "string"),
                "integer" => schema_allows_type(node, "integer"),
                "string_map" => is_string_map_schema(node),
                _ => false,
            };
            if ok {
                ok_for_pat = true;
                break;
            }
        }

        if ok_for_pat {
            patterns_typed_ok += 1;
        } else {
            // capture one example vp (if any) for debugging
            if let Some(vp) = vps.iter().next() {
                let node = schema_node_for_value_path(&schema, vp)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "<missing schema node>".to_string());
                bad.push((pat, vp, node));
            }
        }
    }

    // Require decent coverage and correctness.
    // These charts should hit most of these patterns in practice.
    if patterns_seen < 5 {
        return Err(eyre::eyre!(
            "expected {chart_dir_name} to hit at least 5 common k8s patterns; hit {patterns_seen}. missing sample: {:?}",
            missing
        ));
    }
    if patterns_typed_ok < 4 {
        return Err(eyre::eyre!(
            "expected {chart_dir_name} to have at least 4 correctly typed common k8s patterns; ok={patterns_typed_ok}, seen={patterns_seen}. bad sample: {:?}",
            bad
        ));
    }

    Ok(())
}

fn schema_node_for_prefix<'a>(schema: &'a Value, prefix: &[&str]) -> Option<&'a Value> {
    let mut node = schema;
    for part in prefix {
        if *part == "*" {
            node = node.pointer("/items")?;
            continue;
        }
        let esc = json_pointer_escape(part);
        node = node.pointer(&format!("/properties/{esc}"))?;
    }
    Some(node)
}

fn has_array_schema_at_star(schema: &Value, vp: &str) -> bool {
    let parts: Vec<&str> = vp.split('.').filter(|s| !s.is_empty()).collect();
    let mut prefix: Vec<&str> = Vec::new();
    for part in parts {
        if part == "*" {
            let Some(node) = schema_node_for_prefix(schema, &prefix) else {
                return false;
            };
            if node.get("type").and_then(|v| v.as_str()) != Some("array") {
                return false;
            }
            if node.get("items").is_none() {
                return false;
            }
        }
        prefix.push(part);
    }
    true
}

fn assert_array_wildcards_materialize(chart_dir_name: &str) -> eyre::Result<()> {
    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, chart_dir_name)?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    let star_vps: Vec<&str> = uses
        .iter()
        .map(|u| u.source_expr.as_str())
        .filter(|vp| vp.contains(".*."))
        .collect();

    // For robustness, require at least some wildcard paths exist.
    assert!(
        star_vps.len() >= 5,
        "expected at least 5 wildcard value paths for {chart_dir_name}, got {}",
        star_vps.len()
    );

    let mut ok = 0usize;
    for vp in star_vps.iter().take(50) {
        if has_array_schema_at_star(&schema, vp) {
            ok += 1;
        }
    }

    if ok < 5 {
        return Err(eyre::eyre!(
            "expected array schemas for wildcards in {chart_dir_name}; ok={ok} out of {} checked",
            star_vps.iter().take(50).count()
        ));
    }

    Ok(())
}

#[test]
fn upstream_bitnami_redis_provider_detail() -> eyre::Result<()> {
    Builder::default().build();
    assert_provider_detail_for_chart("bitnami-redis")
}

#[test]
fn upstream_cert_manager_provider_detail() -> eyre::Result<()> {
    Builder::default().build();
    assert_provider_detail_for_chart("cert-manager")
}

#[test]
fn upstream_bitnami_redis_arrays_from_wildcards() -> eyre::Result<()> {
    Builder::default().build();
    assert_array_wildcards_materialize("bitnami-redis")
}

#[test]
fn upstream_cert_manager_arrays_from_wildcards() -> eyre::Result<()> {
    Builder::default().build();
    assert_array_wildcards_materialize("cert-manager")
}
