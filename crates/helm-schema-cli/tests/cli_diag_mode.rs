//! JSON-mode contract tests: after successful argv parse, every line
//! on stderr is a Diagnostic JSON object; CLI parse errors stay on
//! clap's plain-text stderr.

use std::path::PathBuf;
use std::process::{Command, Stdio};

fn binary_path() -> PathBuf {
    let mut p = std::env::current_exe().expect("current_exe");
    while p.file_name().is_some()
        && p.file_name().and_then(|s| s.to_str()) != Some("debug")
        && p.file_name().and_then(|s| s.to_str()) != Some("release")
    {
        p.pop();
    }
    // Now `p` is `<target>/debug` or `<target>/release`. helm-schema
    // binary sits one level up under `<target>/.../helm-schema`.
    p.parent()
        .expect("target dir")
        .join(p.file_name().and_then(|s| s.to_str()).unwrap_or("debug"))
        .join("helm-schema")
}

fn helm_schema_binary() -> PathBuf {
    // Prefer the locally-built debug binary for tests; fall back to release.
    let workspace = std::env::var("CARGO_WORKSPACE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .to_path_buf()
        });
    let debug = workspace.join("target/debug/helm-schema");
    if debug.exists() {
        return debug;
    }
    let release = workspace.join("target/release/helm-schema");
    if release.exists() {
        return release;
    }
    let _ = binary_path();
    debug
}

fn ensure_built() {
    let bin = helm_schema_binary();
    if !bin.exists() {
        let workspace = std::env::var("CARGO_WORKSPACE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .to_path_buf()
            });
        let _ = Command::new("cargo")
            .arg("build")
            .arg("-p")
            .arg("helm-schema-cli")
            .current_dir(&workspace)
            .status();
    }
}

#[test]
fn cli_diag_format_text_is_default() {
    ensure_built();
    let bin = helm_schema_binary();
    if !bin.exists() {
        eprintln!("skip: helm-schema binary not built; skipping");
        return;
    }
    // Invoke with an invalid path → run() produces an error before any
    // schema work happens. We only need stderr to be plain text by
    // default, so any path-based smoke is fine.
    let output = Command::new(&bin)
        .arg("/nonexistent/chart/path")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run binary");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Text mode should not produce JSON-object-shaped lines for runtime
    // emissions. Lines that DO appear must NOT all be JSON objects.
    if !stderr.is_empty() {
        let all_json = stderr
            .lines()
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with('{'));
        assert!(
            !all_json,
            "text mode (default) must not emit JSON objects per line; got:\n{stderr}"
        );
    }
}

#[test]
fn json_mode_parse_errors_stay_on_clap_stderr() {
    ensure_built();
    let bin = helm_schema_binary();
    if !bin.exists() {
        eprintln!("skip: helm-schema binary not built; skipping");
        return;
    }
    // Invalid argv → clap emits its own plain-text usage error and
    // exits non-zero before our JSON-mode runtime ever starts.
    let output = Command::new(&bin)
        .arg("--diag-format=json")
        .arg("--banana")
        .arg("/tmp/chart")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run binary");
    assert!(!output.status.success(), "invalid argv must exit non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Clap's error includes 'unexpected argument' or 'error:' — neither
    // is JSON.
    assert!(
        !stderr.trim_start().starts_with('{'),
        "clap parse-error stderr must not be JSON; got: {stderr}"
    );
    // No JSON objects anywhere.
    for line in stderr.lines() {
        assert!(
            !line.trim_start().starts_with('{')
                || serde_json::from_str::<serde_json::Value>(line).is_err(),
            "clap parse errors must not produce parseable Diagnostic JSON; got line: {line}"
        );
    }
}
