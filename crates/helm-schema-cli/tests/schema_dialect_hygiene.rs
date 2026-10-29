//! Output-wide dialect gate over every committed schema artifact the
//! generator owns: each fixture must validate against its declared
//! metaschema, and every `pattern` / `patternProperties` key in a schema
//! position must compile under ECMA-262 — the dialect Draft-07 actually
//! specifies. The Rust validator's engine accepts RE2-isms like a leading
//! `(?i)` that conforming validators reject, so this gate compiles with a
//! real ECMA engine instead of trusting the validator to notice.
//!
//! Shipped third-party `values.schema.json` files under `testdata/charts/`
//! are deliberately out of scope: they are other authors' artifacts, not
//! generator output.

use std::path::{Path, PathBuf};

use color_eyre::eyre::{self, OptionExt as _, WrapErr as _};
use helm_schema_json_schema_walk::{SchemaTraversalContext, schema_child_context_for_keyword};
use helm_schema_test_support::registry::{self, ArtifactId};
use serde_json::Value;
use test_util::prelude::sim_assert_eq;

const OWNED_FIXTURE_DIRS: &[&str] = &[
    "testdata/chart-corpus-schemas",
    "crates/helm-schema-gen/tests/fixtures",
    "crates/helm-schema-cli/tests/fixtures",
];

fn owned_schema_fixtures() -> eyre::Result<Vec<PathBuf>> {
    let root = test_util::workspace_root();
    let mut fixtures = Vec::new();
    for dir in OWNED_FIXTURE_DIRS {
        collect_schema_files(&root.join(dir), &mut fixtures)
            .wrap_err_with(|| format!("scan {dir}"))?;
    }
    fixtures.sort();
    assert!(
        fixtures.len() >= 55,
        "fixture scan lost the corpus: found {}",
        fixtures.len()
    );
    Ok(fixtures)
}

fn collect_schema_files(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect_schema_files(&path, out)?;
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".schema.json"))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Collect `(json_pointer, pattern)` pairs from schema positions only:
/// `pattern` keyword values and `patternProperties` keys. Instance-data
/// keywords (`const`, `enum`, `default`, …) map to the walker's `Data`
/// context and are never entered, so a chart value that happens to look
/// like a regex cannot trip the gate.
fn collect_schema_patterns(value: &Value, pointer: &str, out: &mut Vec<(String, String)>) {
    let Value::Object(object) = value else {
        return;
    };
    if let Some(Value::String(pattern)) = object.get("pattern") {
        out.push((format!("{pointer}/pattern"), pattern.clone()));
    }
    if let Some(Value::Object(pattern_properties)) = object.get("patternProperties") {
        for key in pattern_properties.keys() {
            out.push((format!("{pointer}/patternProperties"), key.clone()));
        }
    }
    for (key, child) in object {
        let child_pointer = || {
            format!(
                "{pointer}/{}",
                helm_schema_json_schema_walk::escape_json_pointer_segment(key)
            )
        };
        match schema_child_context_for_keyword(key) {
            SchemaTraversalContext::Schema => match child {
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        collect_schema_patterns(item, &format!("{}/{index}", child_pointer()), out);
                    }
                }
                other => collect_schema_patterns(other, &child_pointer(), out),
            },
            SchemaTraversalContext::SchemaArray => {
                if let Value::Array(items) = child {
                    for (index, item) in items.iter().enumerate() {
                        collect_schema_patterns(item, &format!("{}/{index}", child_pointer()), out);
                    }
                }
            }
            SchemaTraversalContext::SchemaMapValues => {
                if let Value::Object(entries) = child {
                    for (entry_key, entry) in entries {
                        let entry_pointer = format!(
                            "{}/{}",
                            child_pointer(),
                            helm_schema_json_schema_walk::escape_json_pointer_segment(entry_key)
                        );
                        collect_schema_patterns(entry, &entry_pointer, out);
                    }
                }
            }
            SchemaTraversalContext::Ref | SchemaTraversalContext::Data => {}
        }
    }
}

#[test]
fn owned_schema_artifacts_validate_against_their_metaschema() -> eyre::Result<()> {
    for path in owned_schema_fixtures()? {
        let schema: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", path.display()))?;
        if let Err(error) = jsonschema::meta::validate(&schema) {
            panic!("{}: metaschema violation: {error}", path.display());
        }
    }
    Ok(())
}

#[test]
fn emitted_schema_patterns_compile_under_go() -> eyre::Result<()> {
    let root = test_util::workspace_root();
    let taskfile: serde_yaml::Value =
        serde_yaml::from_slice(&std::fs::read(root.join("taskfile.yaml"))?)?;
    let version = taskfile["vars"]["HELMSWEEP_GO"]
        .as_str()
        .ok_or_eyre("taskfile.yaml must declare the HELMSWEEP_GO pin")?;
    // `where` resolves an installed mise tool without installing a missing version.
    let installed = std::process::Command::new("mise")
        .args(["where", &format!("go@{version}")])
        .current_dir(&root)
        .env("MISE_OFFLINE", "true")
        .output()
        .wrap_err("mise is required to resolve the installed HELMSWEEP_GO toolchain")?;
    eyre::ensure!(
        installed.status.success(),
        "install go@{version} with mise before running the pattern gate: {}",
        String::from_utf8_lossy(&installed.stderr)
    );
    let go = PathBuf::from(std::str::from_utf8(&installed.stdout)?.trim())
        .join("bin")
        .join(format!("go{}", std::env::consts::EXE_SUFFIX));
    let build = test_util::scratch::target_dir().join("helmsweep-build");
    for required in ["go.mod", "schema_test.go", "third_party/helm-v4.2.3/go.mod"] {
        eyre::ensure!(
            build.join(required).is_file(),
            "missing {}; run task build:helmsweep before the pattern gate",
            build.join(required).display()
        );
    }
    let directory = test_util::scratch::ScratchDir::new("go-patterns")?;
    let go_command = || {
        let mut command = std::process::Command::new(&go);
        command
            .current_dir(&build)
            .env_remove("GOROOT")
            .envs([
                ("GOTOOLCHAIN", "local"),
                ("GOPROXY", "off"),
                ("GONOPROXY", "none"),
                ("GOVCS", "*:off"),
                ("GOSUMDB", "off"),
                ("GOENV", "off"),
                ("GOWORK", "off"),
                ("GOFLAGS", "-mod=readonly"),
            ])
            .env("GOTMPDIR", directory.path());
        command
    };
    let actual_version = go_command()
        .args(["env", "GOVERSION"])
        .output()
        .wrap_err_with(|| format!("cannot run installed go@{version} at {}", go.display()))?;
    eyre::ensure!(
        actual_version.status.success()
            && std::str::from_utf8(&actual_version.stdout)?.trim() == format!("go{version}"),
        "pattern gate requires go{version}; installed tool reports {}{}",
        String::from_utf8_lossy(&actual_version.stdout),
        String::from_utf8_lossy(&actual_version.stderr)
    );
    let mut patterns = std::collections::BTreeSet::new();
    for spec in registry::registry() {
        match spec.id {
            ArtifactId::Ir(_) => continue,
            ArtifactId::Chart(_)
            | ArtifactId::Template(_)
            | ArtifactId::Lean(_)
            | ArtifactId::FinalPolicy(_)
            | ArtifactId::Authoring(_) => {}
        }
        let schema = helm_schema_test_support::consume(spec.id)?;
        let mut located = Vec::new();
        collect_schema_patterns(&schema, "", &mut located);
        patterns.extend(located.into_iter().map(|(_, pattern)| pattern));
    }
    assert!(!patterns.is_empty());
    let input = directory.path().join("patterns.json");
    std::fs::write(&input, serde_json::to_vec(&patterns)?)?;
    // Go test code preserves the oracle executable identity bound by CRD replay receipts.
    let output = go_command()
        .args([
            "test",
            "-v",
            "-run=^TestEmittedPatternsCompile$",
            "-count=1",
            ".",
        ])
        .env("HELM_SCHEMA_PATTERN_INPUT", input)
        .output()
        .wrap_err("cannot start the offline Go pattern gate; run task build:helmsweep first")?;
    sim_assert_eq!(
        have: output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .contains("--- PASS: TestEmittedPatternsCompile"),
        want: true,
        "offline Go pattern gate failed; run task build:helmsweep to populate prerequisites: {}{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr),
    );
    Ok(())
}

#[test]
fn owned_schema_patterns_compile_under_ecma_262() -> eyre::Result<()> {
    for path in owned_schema_fixtures()? {
        let schema: Value = serde_json::from_str(
            &std::fs::read_to_string(&path).wrap_err_with(|| format!("read {}", path.display()))?,
        )
        .wrap_err_with(|| format!("parse {}", path.display()))?;
        let mut patterns = Vec::new();
        collect_schema_patterns(&schema, "", &mut patterns);
        for (pointer, pattern) in patterns {
            if let Err(error) = regress::Regex::new(&pattern) {
                panic!(
                    "{} at {pointer}: pattern {pattern:?} is not ECMA-262: {error}",
                    path.display()
                );
            }
        }
    }
    Ok(())
}
