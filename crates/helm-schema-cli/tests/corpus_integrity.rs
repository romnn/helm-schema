//! Corpus fixture integrity: every locked chart dependency must be vendored.
//!
//! A missing library dependency silently hides its helpers from analysis, so
//! the corpus fixture stops representing the packaged chart while its tests
//! keep passing (bitnami-redis shipped without its locked `common` library
//! this way).
//!
//! Both lock formats count: Helm v3 `Chart.lock` and the legacy Helm v2
//! `requirements.lock` (datadog still ships the latter, and skipping it left
//! that chart's four dependencies entirely unchecked). An unpacked dependency
//! directory only counts when its own `Chart.yaml` records the locked
//! version — presence alone would let a stale vendored copy drift away from
//! the packaged chart while this gate keeps passing.
//!
//! Discovery is recursive: an unpacked vendored dependency is itself a chart
//! and may carry its own lock (airflow's postgresql, signoz's clickhouse →
//! zookeeper chain), so every `charts/` subdirectory is walked as a chart
//! root of its own. Packaged `.tgz` dependencies are immutable committed
//! archives; their internal locks are not extracted.

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

#[derive(Deserialize)]
struct VendoredChartVersion {
    version: String,
}

/// Helm v3 writes `Chart.lock`; v2-era charts (datadog) still ship
/// `requirements.lock`. Prefer `Chart.lock` when both exist, as Helm does.
fn chart_lock_path(chart_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    ["Chart.lock", "requirements.lock"]
        .iter()
        .map(|name| chart_dir.join(name))
        .find(|path| path.is_file())
}

/// A locked dependency counts as vendored when `charts/` holds its packaged
/// archive at the locked version, or its unpacked directory (under the real
/// or aliased name) whose `Chart.yaml` records the locked version.
fn dependency_is_vendored(chart_dir: &std::path::Path, names: &[&str], version: &str) -> bool {
    let charts_dir = chart_dir.join("charts");
    names.iter().any(|name| {
        if charts_dir.join(format!("{name}-{version}.tgz")).is_file() {
            return true;
        }
        let manifest_path = charts_dir.join(name).join("Chart.yaml");
        let Ok(manifest_text) = std::fs::read_to_string(&manifest_path) else {
            return false;
        };
        serde_yaml::from_str::<VendoredChartVersion>(&manifest_text)
            .is_ok_and(|manifest| manifest.version == version)
    })
}

/// Every chart directory reachable from the corpus roots through unpacked
/// `charts/` dependencies, each visited exactly once.
fn locked_chart_roots(
    corpus_root: &std::path::Path,
) -> color_eyre::eyre::Result<Vec<std::path::PathBuf>> {
    let mut roots = Vec::new();
    let mut stack: Vec<std::path::PathBuf> = std::fs::read_dir(corpus_root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()?;
    while let Some(chart_dir) = stack.pop() {
        if !chart_dir.is_dir() {
            continue;
        }
        let charts_dir = chart_dir.join("charts");
        if charts_dir.is_dir() {
            for entry in std::fs::read_dir(&charts_dir)? {
                stack.push(entry?.path());
            }
        }
        roots.push(chart_dir);
    }
    roots.sort();
    Ok(roots)
}

fn missing_locked_dependencies(
    chart_dir: &std::path::Path,
    corpus_root: &std::path::Path,
) -> color_eyre::eyre::Result<Vec<String>> {
    let Some(lock_path) = chart_lock_path(chart_dir) else {
        return Ok(Vec::new());
    };
    let mut missing = Vec::new();
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
        if !dependency_is_vendored(chart_dir, &names, &dependency.version) {
            missing.push(format!(
                "{chart}: locked dependency {name} {version} is not vendored under charts/",
                chart = chart_dir
                    .strip_prefix(corpus_root)
                    .unwrap_or(chart_dir)
                    .display(),
                name = dependency.name,
                version = dependency.version,
            ));
        }
    }
    Ok(missing)
}

#[test]
fn corpus_charts_vendor_every_locked_dependency() -> color_eyre::eyre::Result<()> {
    let corpus_root = test_util::workspace_testdata().join("charts");
    let mut missing = Vec::new();
    for chart_dir in locked_chart_roots(&corpus_root)? {
        missing.extend(missing_locked_dependencies(&chart_dir, &corpus_root)?);
    }
    assert!(
        missing.is_empty(),
        "missing vendored dependencies:\n{}",
        missing.join("\n")
    );
    Ok(())
}

/// Nested vendored charts carry their own locks (signoz's clickhouse →
/// zookeeper chain sits two levels deep); the walk must reach them, or
/// deleting a nested dependency would be invisible to the gate.
#[test]
fn nested_dependency_locks_are_discovered() -> color_eyre::eyre::Result<()> {
    let corpus_root = test_util::workspace_testdata().join("charts");
    let roots = locked_chart_roots(&corpus_root)?;
    let nested = corpus_root
        .join("signoz-signoz")
        .join("charts")
        .join("clickhouse");
    assert!(
        roots.contains(&nested),
        "nested chart root {} not discovered",
        nested.display()
    );
    assert!(
        chart_lock_path(&nested).is_some(),
        "expected the nested clickhouse chart to carry its own lock"
    );
    Ok(())
}

/// A stale unpacked dependency directory must not satisfy the gate: only the
/// locked version proves the vendored copy matches the packaged chart.
#[test]
fn unpacked_dependency_with_wrong_version_is_not_vendored() -> color_eyre::eyre::Result<()> {
    let chart_dir = tempfile::tempdir()?;
    let dependency_dir = chart_dir.path().join("charts").join("common");
    std::fs::create_dir_all(&dependency_dir)?;
    std::fs::write(
        dependency_dir.join("Chart.yaml"),
        "name: common\nversion: 1.0.0\n",
    )?;
    assert!(dependency_is_vendored(
        chart_dir.path(),
        &["common"],
        "1.0.0"
    ));
    assert!(!dependency_is_vendored(
        chart_dir.path(),
        &["common"],
        "2.0.0"
    ));
    Ok(())
}

/// datadog is the corpus chart that still locks its dependencies through the
/// Helm v2 `requirements.lock`; discovery must not skip it.
#[test]
fn legacy_requirements_lock_is_discovered() {
    let datadog = test_util::workspace_testdata()
        .join("charts")
        .join("datadog");
    let lock_path = chart_lock_path(&datadog).expect("datadog lock file discovered");
    assert!(lock_path.ends_with("requirements.lock"));
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
