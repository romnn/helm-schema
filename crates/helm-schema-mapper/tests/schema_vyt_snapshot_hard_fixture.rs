use color_eyre::eyre;
use color_eyre::eyre::OptionExt;
use color_eyre::eyre::WrapErr;
use helm_schema_chart::{LoadOptions, load_chart};
use helm_schema_mapper::schema::{
    DefaultVytSchemaProvider, UpstreamK8sSchemaProvider, UpstreamThenDefaultVytSchemaProvider,
    generate_values_schema_vyt,
};
use helm_schema_mapper::vyt;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use test_util::prelude::*;
use vfs::VfsPath;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_root(root: &VfsPath, name: &str) -> eyre::Result<VfsPath> {
    root.join("testdata/fixture-charts")?
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

fn is_update_mode() -> bool {
    std::env::var("HELM_SCHEMA_UPDATE_SNAPSHOTS")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn fixture_k8s_schema_cache_dir() -> PathBuf {
    crate_root().join("testdata/kubernetes-json-schema")
}

fn canonicalize_json(v: &Value) -> Value {
    match v {
        Value::Object(obj) => {
            let mut keys: Vec<&String> = obj.keys().collect();
            keys.sort();
            let mut out = Map::new();
            for k in keys {
                if let Some(val) = obj.get(k) {
                    out.insert(k.clone(), canonicalize_json(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json).collect()),
        _ => v.clone(),
    }
}

fn json_pretty(v: &Value) -> eyre::Result<String> {
    let v = canonicalize_json(v);
    Ok(serde_json::to_string_pretty(&v).map_err(|e| eyre::eyre!(e))?)
}

fn use_sort_key(
    u: &vyt::VYUse,
) -> (
    String,
    String,
    String,
    Vec<String>,
    Option<(String, String)>,
) {
    let kind = match u.kind {
        vyt::VYKind::Scalar => "Scalar",
        vyt::VYKind::Fragment => "Fragment",
    }
    .to_string();

    let resource = u
        .resource
        .as_ref()
        .map(|r| (r.api_version.clone(), r.kind.clone()));

    (
        u.source_expr.clone(),
        u.path.0.join("."),
        kind,
        u.guards.clone(),
        resource,
    )
}

fn cmp_uses(a: &vyt::VYUse, b: &vyt::VYUse) -> Ordering {
    use_sort_key(a).cmp(&use_sort_key(b))
}

fn uses_to_json(uses: &[vyt::VYUse]) -> Value {
    let mut out: Vec<Value> = Vec::with_capacity(uses.len());
    for u in uses {
        let mut m = Map::new();
        m.insert(
            "source_expr".to_string(),
            Value::String(u.source_expr.clone()),
        );
        m.insert("path".to_string(), Value::String(u.path.0.join(".")));
        m.insert(
            "kind".to_string(),
            Value::String(match u.kind {
                vyt::VYKind::Scalar => "Scalar".to_string(),
                vyt::VYKind::Fragment => "Fragment".to_string(),
            }),
        );
        m.insert(
            "guards".to_string(),
            Value::Array(u.guards.iter().cloned().map(Value::String).collect()),
        );
        if let Some(r) = &u.resource {
            let mut rm = Map::new();
            rm.insert(
                "apiVersion".to_string(),
                Value::String(r.api_version.clone()),
            );
            rm.insert("kind".to_string(), Value::String(r.kind.clone()));
            m.insert("resource".to_string(), Value::Object(rm));
        } else {
            m.insert("resource".to_string(), Value::Null);
        }
        out.push(Value::Object(m));
    }
    Value::Array(out)
}

fn assert_snapshot(path: &Path, content: &str) -> eyre::Result<()> {
    if is_update_mode() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| eyre::eyre!(e))?;
        }
        fs::write(path, content).map_err(|e| eyre::eyre!(e))?;
        return Ok(());
    }

    if !path.exists() {
        return Err(eyre::eyre!(
            "snapshot missing: {} (re-run with HELM_SCHEMA_UPDATE_SNAPSHOTS=1)",
            path.display()
        ));
    }

    let expected = fs::read_to_string(path).map_err(|e| eyre::eyre!(e))?;
    if expected != content {
        return Err(eyre::eyre!("snapshot mismatch: {}", path.display()));
    }

    Ok(())
}

#[test]
fn snapshot_hard_fixture_ir_and_schema() -> eyre::Result<()> {
    Builder::default().build();

    let root = VfsPath::new(vfs::PhysicalFS::new(env!("CARGO_MANIFEST_DIR")));
    let chart_dir = fixture_root(&root, "hard-fixture")?;

    let chart = load_chart(&chart_dir, &LoadOptions::default())?;
    let mut uses = run_vyt_on_chart_templates(&chart)?;
    uses.sort_by(cmp_uses);

    let upstream = UpstreamK8sSchemaProvider::new("v1.29.0-standalone-strict")
        .with_cache_dir(fixture_k8s_schema_cache_dir())
        .with_allow_download(false);
    let provider = UpstreamThenDefaultVytSchemaProvider {
        upstream,
        fallback: DefaultVytSchemaProvider::default(),
    };

    let schema = generate_values_schema_vyt(&uses, &provider);

    let snaps_dir = crate_root().join("tests/snapshots");

    let ir_json = uses_to_json(&uses);
    let ir_pretty = json_pretty(&ir_json)?;
    let schema_pretty = json_pretty(&schema)?;

    let ir_path = snaps_dir.join("hard-fixture.ir.json");
    let schema_path = snaps_dir.join("hard-fixture.values.schema.json");

    assert_snapshot(&ir_path, &ir_pretty)?;
    assert_snapshot(&schema_path, &schema_pretty)?;

    Ok(())
}
