//! Pinned Helm parity for the render-YAML gaps in the value-algebra unit controls.

use std::process::Command;

use color_eyre::eyre;
use helm_schema_test_support::helm::invocation::find_helm;
use indoc::{formatdoc, indoc};
use test_util::prelude::sim_assert_eq;

/// Renders with the exact Helm release whose loader the adapter models.
fn render_with_pinned_helm(template: &str) -> eyre::Result<std::process::Output> {
    let helm = find_helm()?;
    let version = Command::new(&helm)
        .args(["version", "--template", "{{.Version}}"])
        .output()?;
    assert!(version.status.success(), "{version:?}");
    sim_assert_eq!(have: String::from_utf8(version.stdout)?.trim(), want: "v4.2.3");
    let chart = test_util::scratch::ScratchDir::new("ir-controls-helm-parity")?;
    std::fs::create_dir_all(chart.path().join("templates"))?;
    std::fs::write(
        chart.path().join("Chart.yaml"),
        indoc! {"
            apiVersion: v2
            name: c
            version: 0.1.0
        "},
    )?;
    std::fs::write(chart.path().join("templates/cm.yaml"), template)?;
    Ok(Command::new(&helm)
        .args(["template", "p"])
        .arg(chart.path())
        .args(["--kube-version", "1.29.0"])
        .output()?)
}

fn assert_helm_yaml_abort(template: &str) -> eyre::Result<()> {
    let rendered = render_with_pinned_helm(template)?;
    let stderr = String::from_utf8(rendered.stderr)?;
    sim_assert_eq!(have: rendered.status.code(), want: Some(1), "{template}: {stderr}");
    assert!(stderr.contains("YAML parse error"), "{template}: {stderr}");
    Ok(())
}

/// Go's folded struct lookup can reach typed fields through aliases Rust does not look up.
#[test]
fn case_folded_headers_abort_in_pinned_helm() -> eyre::Result<()> {
    for alias in [
        "Metadata: []",
        "APIVERSION: []",
        "KIND: []",
        "\u{212a}ind: []",
        indoc! {"
            metadata:
              Name: []
        "},
        indoc! {"
            metadata:
              Annotations: []
        "},
        indoc! {"
            metadata:
              annotationſ: []
        "},
    ] {
        assert_helm_yaml_abort(&formatdoc! {"
            apiVersion: v1
            kind: ConfigMap
            {alias}
        "})?;
    }
    Ok(())
}

/// Non-finite values are outside Helm's JSON domain, including nested mappings and sequences.
#[test]
fn non_finite_yaml_aborts_in_pinned_helm() -> eyre::Result<()> {
    for scalar in [".nan", ".inf", "-.inf"] {
        for body in [
            format!("data: {scalar}\n"),
            formatdoc! {"
                data:
                  nested:
                    - values: [{scalar}]
            "},
        ] {
            assert_helm_yaml_abort(&formatdoc! {"
                apiVersion: v1
                kind: ConfigMap
                {body}
            "})?;
        }
    }
    // Finite numbers and quoted spellings remain valid loader inputs.
    let finite = indoc! {r#"
        apiVersion: v1
        kind: ConfigMap
        data:
          nested: [1.5, 0, -1, ".nan", ".inf"]
    "#};
    let rendered = render_with_pinned_helm(finite)?;
    assert!(rendered.status.success(), "{rendered:?}");
    Ok(())
}
