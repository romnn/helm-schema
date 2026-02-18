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
use std::path::PathBuf;
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

    let ir_json = uses_to_json(&uses);
    let ir_pretty = json_pretty(&ir_json)?;
    let schema_pretty = json_pretty(&schema)?;

    if std::env::var("HELM_SCHEMA_PRINT_ACTUAL_IR").is_ok() {
        println!("=== ACTUAL IR (hard-fixture) ===\n{}\n", ir_pretty);
    }
    if std::env::var("HELM_SCHEMA_PRINT_ACTUAL_SCHEMA").is_ok() {
        println!("=== ACTUAL SCHEMA (hard-fixture) ===\n{}\n", schema_pretty);
    }

    let expected_ir: Value = serde_json::from_str(include_str!("snapshots/hard-fixture.ir.json"))
        .map_err(|e| eyre::eyre!(e))?;
    let expected_schema: Value =
        serde_json::from_str(include_str!("snapshots/hard-fixture.values.schema.json"))
            .map_err(|e| eyre::eyre!(e))?;

    let expected_ir_pretty = json_pretty(&expected_ir)?;
    let expected_schema_pretty = json_pretty(&expected_schema)?;

    sim_assert_eq!(expected_ir_pretty, ir_pretty);
    sim_assert_eq!(expected_schema_pretty, schema_pretty);

    Ok(())
}
