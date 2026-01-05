use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use color_eyre::eyre;
use helm_schema_chart::{load_chart, LoadOptions};
use helm_schema_mapper::generate_values_schema_for_chart_vyt;
use test_util::prelude::*;
use vfs::VfsPath;

fn default_reference_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("dev/luup/deployment/charts/signoz/schemas/signoz.schema.json"),
    )
}

fn resolve_ref<'a>(root: &'a serde_json::Value, r: &str) -> Option<&'a serde_json::Value> {
    if let Some(ptr) = r.strip_prefix("#") {
        if ptr.is_empty() {
            return Some(root);
        }
        if let Some(ptr) = ptr.strip_prefix('/') {
            let full = format!("/{ptr}");
            return root.pointer(&full);
        }
    }
    None
}

fn collect_paths(schema: &serde_json::Value) -> BTreeSet<String> {
    fn rec(
        node: &serde_json::Value,
        root: &serde_json::Value,
        path: &mut Vec<String>,
        out: &mut BTreeSet<String>,
        ref_stack: &mut Vec<String>,
    ) {
        let Some(o) = node.as_object() else {
            return;
        };

        if let Some(r) = o.get("$ref").and_then(|v| v.as_str()) {
            if !ref_stack.iter().any(|x| x == r) {
                ref_stack.push(r.to_string());
                if let Some(target) = resolve_ref(root, r) {
                    rec(target, root, path, out, ref_stack);
                }
                ref_stack.pop();
            }
        }

        for kw in [
            "allOf",
            "anyOf",
            "oneOf",
            "not",
            "if",
            "then",
            "else",
            "contains",
            "propertyNames",
        ] {
            if let Some(v) = o.get(kw) {
                match v {
                    serde_json::Value::Array(arr) => {
                        for it in arr {
                            rec(it, root, path, out, ref_stack);
                        }
                    }
                    serde_json::Value::Object(_) => rec(v, root, path, out, ref_stack),
                    _ => {}
                }
            }
        }

        if let Some(props) = o.get("properties").and_then(|v| v.as_object()) {
            for (k, v) in props {
                path.push(k.clone());
                out.insert(path.join("."));
                rec(v, root, path, out, ref_stack);
                path.pop();
            }
        }

        if let Some(items) = o.get("items") {
            path.push("*".to_string());
            match items {
                serde_json::Value::Array(arr) => {
                    for it in arr {
                        rec(it, root, path, out, ref_stack);
                    }
                }
                _ => rec(items, root, path, out, ref_stack),
            }
            path.pop();
        }

        if let Some(pitems) = o.get("prefixItems") {
            path.push("*".to_string());
            if let Some(arr) = pitems.as_array() {
                for it in arr {
                    rec(it, root, path, out, ref_stack);
                }
            } else {
                rec(pitems, root, path, out, ref_stack);
            }
            path.pop();
        }

        if let Some(ap) = o.get("additionalProperties") {
            if ap.is_object() {
                path.push("__any__".to_string());
                rec(ap, root, path, out, ref_stack);
                path.pop();
            }
        }

        if let Some(pp) = o.get("patternProperties").and_then(|v| v.as_object()) {
            for (_pat, v) in pp {
                path.push("__any__".to_string());
                rec(v, root, path, out, ref_stack);
                path.pop();
            }
        }
    }

    let mut out = BTreeSet::new();
    let mut path = Vec::new();
    let mut ref_stack = Vec::new();
    rec(schema, schema, &mut path, &mut out, &mut ref_stack);
    out
}

fn sample(set: &BTreeSet<String>, n: usize) -> Vec<String> {
    set.iter().take(n).cloned().collect()
}

fn read_json_file(p: &Path) -> eyre::Result<serde_json::Value> {
    let raw = std::fs::read_to_string(p)?;
    Ok(serde_json::from_str(&raw)?)
}

#[test]
fn signoz_reference_schema_coverage_diff() -> eyre::Result<()> {
    Builder::default().build();

    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let chart_root_path = crate_root.join("testdata/charts/signoz-signoz");
    if !chart_root_path.exists() {
        return Ok(());
    }

    let ref_path = std::env::var("SIGNOZ_SCHEMA_REF")
        .ok()
        .map(PathBuf::from)
        .or_else(default_reference_path);

    let Some(ref_path) = ref_path else {
        return Ok(());
    };
    if !ref_path.exists() {
        return Ok(());
    }

    let reference = read_json_file(&ref_path)?;

    let chart_root = VfsPath::new(vfs::PhysicalFS::new(env!("CARGO_MANIFEST_DIR")))
        .join("testdata/charts/signoz-signoz")?;

    let chart = load_chart(
        &chart_root,
        &LoadOptions {
            include_tests: false,
            recurse_subcharts: true,
            auto_extract_tgz: true,
            respect_gitignore: false,
            include_hidden: false,
        },
    )?;

    let ours = generate_values_schema_for_chart_vyt(&chart)?;

    if let Ok(p) = std::env::var("DUMP_SIG_NOZ_OURS") {
        let out = serde_json::to_string_pretty(&ours)?;
        std::fs::write(&p, out)?;
    }
    if let Ok(p) = std::env::var("DUMP_SIG_NOZ_REF") {
        let out = serde_json::to_string_pretty(&reference)?;
        std::fs::write(&p, out)?;
    }

    let ref_paths = collect_paths(&reference);
    let our_paths = collect_paths(&ours);

    let covered = ref_paths.intersection(&our_paths).count();
    let total = ref_paths.len().max(1);
    let ratio = covered as f64 / total as f64;

    let missing: BTreeSet<String> = ref_paths.difference(&our_paths).cloned().collect();

    eprintln!(
        "SigNoz ref coverage: {covered}/{total} = {ratio:.3} (missing: {})",
        missing.len()
    );
    eprintln!("SigNoz ref missing sample: {:?}", sample(&missing, 50));

    eyre::ensure!(
        ratio >= 0.40,
        "coverage too low vs reference: {ratio:.3} (covered {covered}/{total}); missing sample: {:?}",
        sample(&missing, 100)
    );

    Ok(())
}
