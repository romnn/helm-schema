//! Corpus fixture integrity: every locked chart dependency must be vendored.
//!
//! A missing library dependency silently hides its helpers from analysis, so
//! the corpus fixture stops representing the packaged chart while its tests
//! keep passing (bitnami-redis shipped without its locked `common` library
//! this way).

use serde::Deserialize;

#[derive(Deserialize)]
struct ChartLock {
    #[serde(default)]
    dependencies: Vec<LockedDependency>,
}

#[derive(Deserialize)]
struct LockedDependency {
    name: String,
    version: String,
}

#[derive(Deserialize)]
struct ChartManifest {
    #[serde(default)]
    dependencies: Vec<ManifestDependency>,
}

#[derive(Deserialize)]
struct ManifestDependency {
    name: String,
    #[serde(default)]
    alias: Option<String>,
}

/// A locked dependency counts as vendored when `charts/` holds its unpacked
/// directory (under the real or aliased name) or its packaged archive.
fn dependency_is_vendored(chart_dir: &std::path::Path, names: &[&str], version: &str) -> bool {
    let charts_dir = chart_dir.join("charts");
    names.iter().any(|name| {
        charts_dir.join(name).join("Chart.yaml").is_file()
            || charts_dir.join(format!("{name}-{version}.tgz")).is_file()
    })
}

#[test]
fn corpus_charts_vendor_every_locked_dependency() -> color_eyre::eyre::Result<()> {
    let corpus_root = test_util::workspace_testdata().join("charts");
    let mut missing = Vec::new();
    for entry in std::fs::read_dir(&corpus_root)? {
        let chart_dir = entry?.path();
        let lock_path = chart_dir.join("Chart.lock");
        if !lock_path.is_file() {
            continue;
        }
        let lock: ChartLock = serde_yaml::from_str(&std::fs::read_to_string(&lock_path)?)?;
        let manifest: ChartManifest =
            serde_yaml::from_str(&std::fs::read_to_string(chart_dir.join("Chart.yaml"))?)?;
        for dependency in &lock.dependencies {
            let mut names = vec![dependency.name.as_str()];
            names.extend(manifest.dependencies.iter().filter_map(|declared| {
                (declared.name == dependency.name)
                    .then_some(declared.alias.as_deref())
                    .flatten()
            }));
            if !dependency_is_vendored(&chart_dir, &names, &dependency.version) {
                missing.push(format!(
                    "{chart}: locked dependency {name} {version} is not vendored under charts/",
                    chart = chart_dir.file_name().unwrap_or_default().to_string_lossy(),
                    name = dependency.name,
                    version = dependency.version,
                ));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "missing vendored dependencies:\n{}",
        missing.join("\n")
    );
    Ok(())
}

/// The committed provider bundle is a deterministic test input: if it goes
/// missing (a stray gitignore, an overzealous cleanup), corpus generation
/// silently degrades to cold-cache output and every provider-backed fact
/// disappears from the expected schemas while their tests keep passing.
#[test]
fn provider_bundle_holds_kubernetes_schemas() -> color_eyre::eyre::Result<()> {
    let bundle = test_util::workspace_testdata().join("provider-bundle");
    let mut schema_files = 0usize;
    let mut stack = vec![bundle.join("kubernetes-json-schema-cache")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                schema_files += 1;
            }
        }
    }
    assert!(
        schema_files > 0,
        "the vendored Kubernetes schema bundle at {} holds no schema files; \
         corpus fixtures would silently lose their provider-backed facts",
        bundle.display()
    );
    Ok(())
}
