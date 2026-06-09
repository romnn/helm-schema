//! Plan coverage matrix for CLI flag handling: per-axis conflicts,
//! validation rules, and strict-mode invariants.

use clap::Parser;
use helm_schema_cli::Cli;
use helm_schema_cli::cli::{CrdVersionLookup, DiagFormat, K8sVersionFallback};

fn parse(args: &[&str]) -> Result<Cli, String> {
    let mut full = vec!["helm-schema"];
    full.extend_from_slice(args);
    full.push("/tmp/chart");
    Cli::try_parse_from(full).map_err(|e| e.to_string())
}

#[test]
fn cli_diag_format_text_is_default() {
    let cli = parse(&[]).expect("parse");
    assert_eq!(cli.diag.diag_format, DiagFormat::Text);
}

#[test]
fn cli_perf_flags_parse() {
    let cli = parse(&[
        "--profile-phases",
        "--trace-output",
        "/tmp/helm-schema.trace",
    ])
    .expect("parse");
    assert!(cli.perf.profile_phases);
    assert_eq!(
        cli.perf.trace_output.as_deref(),
        Some(std::path::Path::new("/tmp/helm-schema.trace"))
    );
}

#[test]
fn cli_output_minimize_flag_parses() {
    let cli = parse(&["--minimize"]).expect("parse");
    assert!(cli.output.minimize);
}

#[test]
fn cli_repeated_k8s_version_preserves_order() {
    let cli = parse(&["--k8s-version", "v1.24.0", "--k8s-version", "v1.35.0"]).expect("parse");
    assert_eq!(
        cli.k8s.k8s_version,
        vec!["v1.24.0".to_string(), "v1.35.0".to_string()]
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
    assert_eq!(
        cli.k8s.k8s_schema_mirror,
        vec!["https://example/".to_string()]
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
    assert_eq!(
        cli.crd.crd_catalog_mirror,
        vec!["https://example/".to_string()]
    );
    assert_eq!(cli.crd.lookup_mode(), CrdVersionLookup::Strict);
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
    assert_eq!(window, None, "strict mode must disable auto-fallback");
}

#[test]
fn k8s_version_fallback_auto_resolves_to_default_window() {
    let cli = parse(&["--k8s-version-fallback=auto"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    assert_eq!(window, Some(helm_schema_cli::cli::DEFAULT_AUTO_WINDOW));
    assert_eq!(cli.k8s.k8s_version_fallback, Some(K8sVersionFallback::Auto));
}

// Pins the auto-fallback policy: charts using policy/v1beta1 (removed
// in v1.25) must resolve from the current default primary (v1.35.0).
// If this fails it means DEFAULT_AUTO_WINDOW is no longer wide enough
// to reach the historical deprecation floor — bump the constant rather
// than weakening this test.
#[test]
fn k8s_auto_fallback_default_reaches_v1_24_from_v1_35() {
    use helm_schema_k8s::K8sVersionChain;
    let cli = parse(&["--k8s-version-fallback=auto"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    let chain = K8sVersionChain::new(cli.k8s.k8s_version.clone(), window);
    let versions = chain.ordered();
    assert!(
        versions.contains(&"v1.24.0".to_string()),
        "auto window must reach v1.24.0 from default v1.35.0; got {versions:?}"
    );
}

#[test]
fn k8s_version_fallback_explicit_window() {
    let cli = parse(&["--k8s-version-fallback=3"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    assert_eq!(window, Some(3));
}

#[test]
fn k8s_version_fallback_zero_means_disabled() {
    let cli = parse(&["--k8s-version-fallback=0"]).expect("parse");
    let window = cli.k8s.resolved_fallback_window().expect("resolve");
    assert_eq!(window, None);
}
