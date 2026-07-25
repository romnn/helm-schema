//! JSON-mode contract tests: after successful argv parse, every line
//! on stderr is a Diagnostic JSON object; CLI parse errors stay on
//! clap's plain-text stderr.

use std::process::{Command, Stdio};

use color_eyre::eyre::{self, WrapErr as _};

/// Cargo builds the binary before running this test and points
/// `CARGO_BIN_EXE_helm-schema` at it, with the platform's executable
/// extension already applied. Resolving the path by hand instead would miss
/// Windows' `.exe` and leave the tests looking at a file that never exists.
const HELM_SCHEMA_BIN: &str = env!("CARGO_BIN_EXE_helm-schema");

#[test]
fn cli_diag_format_text_is_default() -> eyre::Result<()> {
    // Invoke with an invalid path → run() produces an error before any
    // schema work happens. We only need stderr to be plain text by
    // default, so any path-based smoke is fine.
    let output = Command::new(HELM_SCHEMA_BIN)
        .arg("definitely/not/a/chart")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("run helm-schema CLI")?;
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
    Ok(())
}

#[test]
fn json_mode_parse_errors_stay_on_clap_stderr() -> eyre::Result<()> {
    // Invalid argv → clap emits its own plain-text usage error and
    // exits non-zero before our JSON-mode runtime ever starts.
    let output = Command::new(HELM_SCHEMA_BIN)
        .arg("--diag-format=json")
        .arg("--banana")
        .arg("some/chart")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .wrap_err("run helm-schema CLI with invalid arguments")?;
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
    Ok(())
}
