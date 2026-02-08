use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use helm_schema_mapper::vyt;
use serde_json::Value;
use std::collections::BTreeSet;
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
    let ap = node.get("additionalProperties");
    match ap {
        Some(Value::Object(_)) => schema_allows_type(ap.unwrap(), "string"),
        _ => false,
    }
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

fn unique_value_paths_matching<'a>(
    uses: &'a [vyt::VYUse],
    pred: impl Fn(&'a str) -> bool,
) -> Vec<&'a str> {
    let mut set: BTreeSet<&'a str> = BTreeSet::new();
    for u in uses {
        let vp = u.source_expr.as_str();
        if vp.is_empty() {
            continue;
        }
        if pred(vp) {
            set.insert(vp);
        }
    }
    set.into_iter().collect()
}

fn assert_min_typed_paths(
    schema: &Value,
    paths: &[&str],
    min_ok: usize,
    check: impl Fn(&Value) -> bool,
    label: &str,
) -> eyre::Result<()> {
    let mut ok = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    let mut bad: Vec<(&str, String)> = Vec::new();

    for vp in paths {
        let Some(node) = schema_node_for_value_path(schema, vp) else {
            missing.push(*vp);
            continue;
        };
        if check(node) {
            ok += 1;
        } else {
            bad.push((*vp, node.to_string()));
        }
    }

    if ok < min_ok {
        return Err(eyre::eyre!(
            "schema typing check failed for {label}: ok={ok} < min={min_ok}. missing={} bad={}\nmissing sample: {:?}\nbad sample: {:?}",
            missing.len(),
            bad.len(),
            missing.iter().take(10).collect::<Vec<_>>(),
            bad.iter().take(5).collect::<Vec<_>>()
        ));
    }

    Ok(())
}

fn assert_chart_schema_matches_ir(
    chart_dir_name: &str,
    min_enabled_bools: usize,
    min_ints: usize,
    min_string_maps: usize,
) -> eyre::Result<()> {
    let root = VfsPath::new(vfs::PhysicalFS::new("."));
    let chart_dir = chart_root(&root, chart_dir_name)?;
    if !chart_dir.exists()? {
        return Ok(());
    }

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let uses = run_vyt_on_chart_templates(&chart)?;
    let schema = generate_values_schema_for_chart_vyt(&chart)?;

    // Boolean flags
    let enabled = unique_value_paths_matching(&uses, |vp| {
        vp == "installCRDs" || vp.ends_with(".enabled") || vp.ends_with("Enabled")
    });
    assert!(
        enabled.len() >= min_enabled_bools,
        "expected at least {min_enabled_bools} enabled flags in IR for {chart_dir_name}, got {}",
        enabled.len()
    );
    assert_min_typed_paths(
        &schema,
        &enabled,
        min_enabled_bools,
        |n| schema_allows_type(n, "boolean"),
        "boolean enabled flags",
    )?;

    // Integer-ish paths
    let ints = unique_value_paths_matching(&uses, |vp| {
        let last = vp.rsplit('.').next().unwrap_or(vp);
        matches!(
            last,
            "replicas"
                | "replicaCount"
                | "revisionHistoryLimit"
                | "terminationGracePeriodSeconds"
                | "port"
                | "targetPort"
                | "nodePort"
                | "containerPort"
                | "hostPort"
                | "number"
        )
    });
    assert!(
        ints.len() >= min_ints,
        "expected at least {min_ints} int-ish paths in IR for {chart_dir_name}, got {}",
        ints.len()
    );
    assert_min_typed_paths(
        &schema,
        &ints,
        min_ints,
        |n| schema_allows_type(n, "integer"),
        "integer fields",
    )?;

    // String maps (annotations/labels)
    let maps = unique_value_paths_matching(&uses, |vp| {
        vp.ends_with("annotations") || vp.ends_with("labels")
    });
    assert!(
        maps.len() >= min_string_maps,
        "expected at least {min_string_maps} annotation/label maps in IR for {chart_dir_name}, got {}",
        maps.len()
    );
    assert_min_typed_paths(
        &schema,
        &maps,
        min_string_maps,
        is_string_map_schema,
        "string map fields",
    )?;

    Ok(())
}

#[test]
fn upstream_bitnami_redis_schema_types_from_ir() -> eyre::Result<()> {
    Builder::default().build();
    assert_chart_schema_matches_ir("bitnami-redis", 5, 5, 3)
}

#[test]
fn upstream_cert_manager_schema_types_from_ir() -> eyre::Result<()> {
    Builder::default().build();
    assert_chart_schema_matches_ir("cert-manager", 5, 5, 3)
}
