//! Plan coverage matrix for CLI flag handling: per-axis conflicts,
//! validation rules, and strict-mode invariants.

use clap::Parser;
use helm_schema_cli::Cli;
use helm_schema_cli::cli::{CrdVersionLookup, DiagFormat, K8sVersionFallback};
use test_util::prelude::sim_assert_eq;

fn parse(args: &[&str]) -> Result<Cli, String> {
    let mut full = vec!["helm-schema"];
    full.extend_from_slice(args);
    full.push("/tmp/chart");
    Cli::try_parse_from(full).map_err(|e| e.to_string())
}

#[test]
fn cli_diag_format_text_is_default() {
    let cli = parse(&[]).expect("parse");
    sim_assert_eq!(have: cli.diag.diag_format, want: DiagFormat::Text);
}

#[test]
fn cli_perf_flags_parse() {
    let cli = parse(&["--trace-output", "/tmp/helm-schema.trace"]).expect("parse");
    sim_assert_eq!(
        have: cli.perf.trace_output.as_deref(),
        want: Some(std::path::Path::new("/tmp/helm-schema.trace"))
    );
}

#[test]
fn cli_output_minimize_flag_parses() {
    let cli = parse(&["--minimize"]).expect("parse");
    assert!(cli.output.minimize);
}

#[test]
fn cli_output_strip_descriptions_flag_parses() {
    let cli = parse(&["--strip-descriptions"]).expect("parse");
    assert!(cli.output.strip_descriptions);
}

#[test]
fn cli_output_inline_refs_flag_parses() {
    let cli = parse(&["--inline-refs"]).expect("parse");
    assert!(cli.output.inline_refs);
}

#[test]
fn cli_output_ref_modes_conflict() {
    let err = parse(&["--keep-refs", "--inline-refs"]).expect_err("expected clap conflict");
    assert!(err.contains("--keep-refs") || err.contains("--inline-refs"));
}

#[test]
fn cli_values_files_flag_is_repeatable() {
    let cli = parse(&["-f", "/tmp/base.yaml", "--values", "/tmp/override.yaml"]).expect("parse");
    sim_assert_eq!(
        have: cli.chart.values_files,
        want: vec![
            std::path::PathBuf::from("/tmp/base.yaml"),
            std::path::PathBuf::from("/tmp/override.yaml")
        ]
    );
}

#[test]
fn cli_repeated_k8s_version_preserves_order() {
    let cli = parse(&["--k8s-version", "v1.24.0", "--k8s-version", "v1.35.0"]).expect("parse");
    sim_assert_eq!(
        have: cli.k8s.k8s_version,
        want: vec!["v1.24.0".to_string(), "v1.35.0".to_string()]
    );
}

#[test]
fn cli_rejects_auto_with_multiple_explicit_versions() {
    let cli = parse(&[
        "--k8s-version",
        "v1.35.0",
        "--k8s-version",
        "v1.30.0",
        "--k8s-version-fallback=auto",
    ])
    .expect("parse should succeed");
    assert!(
        cli.k8s.resolved_fallback_window().is_err(),
        "--k8s-version-fallback=auto + multiple --k8s-version must be rejected"
    );
}

#[test]
fn cli_strict_and_heuristic_flags_conflict() {
    // strict + fallback flag must conflict
    let err = parse(&["--strict-k8s-version", "--k8s-version-fallback=auto"])
        .expect_err("expected clap conflict");
    assert!(err.contains("--k8s-version-fallback") || err.contains("strict_k8s_version"));

    // strict-api-versions + --api-version-guess must conflict
    let err = parse(&["--strict-api-versions", "--api-version-guess"])
        .expect_err("expected clap conflict");
    assert!(err.contains("--api-version-guess") || err.contains("strict_api_versions"));
}

#[test]
fn k8s_strict_does_not_conflict_with_mirror_flag() {
    let cli = parse(&[
        "--strict-k8s-version",
        "--k8s-schema-mirror",
        "https://example/",
    ])
    .expect("strict + mirror must be accepted at parse time");
    assert!(cli.k8s.strict_k8s_version);
    sim_assert_eq!(
        have: cli.k8s.k8s_schema_mirror,
        want: vec!["https://example/".to_string()]
    );
}

#[test]
fn crd_strict_does_not_conflict_with_mirror_flag() {
    let cli = parse(&[
        "--strict-crd-version",
        "--crd-catalog-mirror",
        "https://example/",
    ])
    .expect("strict + mirror must be accepted at parse time");
    assert!(cli.crd.strict_crd_version);
    sim_assert_eq!(
        have: cli.crd.crd_catalog_mirror,
        want: vec!["https://example/".to_string()]
    );
    sim_assert_eq!(have: cli.crd.lookup_mode(), want: CrdVersionLookup::Strict);
}

#[test]
fn cli_rejects_removed_crd_catalog_dir_flag() {
    let cli = parse(&["--crd-catalog-dir", "/some/path"]).expect("parse");
    let err = cli.crd.validate().expect_err("removed flag must error");
    assert!(err.contains("--crd-catalog-dir is removed"));
    assert!(err.contains("--crd-override-dir"));
    assert!(err.contains("--crd-catalog-cache-dir"));
}

#[test]
fn cli_rejects_override_and_cache_dir_same_path() {
    let cli = parse(&[
        "--crd-override-dir",
        "/foo",
        "--crd-catalog-cache-dir",
        "/foo",
    ])
    .expect("parse");
    let err = cli.crd.validate().expect_err("same path must error");
    assert!(err.contains("must point at different paths"));
}

#[test]
fn k8s_strict_collapses_chain_to_explicit_versions() {
    let cli = parse(&["--strict-k8s-version", "--k8s-version", "v1.35.0"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    sim_assert_eq!(have: window, want: None, "strict mode must disable auto-fallback");
}

#[test]
fn k8s_version_fallback_auto_resolves_to_default_window() {
    let cli = parse(&["--k8s-version-fallback=auto"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    sim_assert_eq!(have: window, want: Some(helm_schema_cli::cli::DEFAULT_AUTO_WINDOW));
    sim_assert_eq!(have: cli.k8s.k8s_version_fallback, want: Some(K8sVersionFallback::Auto));
}

// Pins the auto-fallback policy: charts using policy/v1beta1 (removed
// in v1.25) must resolve from the current default primary (v1.35.0).
// If this fails it means DEFAULT_AUTO_WINDOW is no longer wide enough
// to reach the historical deprecation floor — bump the constant rather
// than weakening this test.
#[test]
fn k8s_auto_fallback_default_reaches_v1_24_from_v1_35() {
    let cli = parse(&["--k8s-version-fallback=auto"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    let versions =
        helm_schema::provider::K8sVersionChain::new(cli.k8s.k8s_version.clone(), window).ordered();
    assert!(
        versions.contains(&"v1.24.0".to_string()),
        "auto window must reach v1.24.0 from default v1.35.0; got {versions:?}"
    );
}

#[test]
fn k8s_version_fallback_explicit_window() {
    let cli = parse(&["--k8s-version-fallback=3"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    sim_assert_eq!(have: window, want: Some(3));
}

#[test]
fn k8s_version_fallback_zero_means_disabled() {
    let cli = parse(&["--k8s-version-fallback=0"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    sim_assert_eq!(have: window, want: None);
}
