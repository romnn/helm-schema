//! Helm controls for argument evaluation boundaries preserved by schema inference.

use std::process::{Command, Output};

use color_eyre::eyre::{self, WrapErr as _};
use test_util::prelude::sim_assert_eq;

fn render(extra_args: &[&str]) -> eyre::Result<Output> {
    let chart = test_util::workspace_testdata().join("charts/grouped-argument-evaluation");
    Command::new("helm")
        .arg("template")
        .arg("grouped-argument-evaluation")
        .arg(chart)
        .arg("--skip-schema-validation")
        .arg("--kube-version")
        .arg("1.29.0")
        .args(extra_args)
        .output()
        .wrap_err("run grouped-argument Helm reproducer")
}

fn check_case(extra_args: &[&str], want_success: bool) -> eyre::Result<()> {
    let output = render(extra_args)?;
    sim_assert_eq!(
        have: output.status.success(),
        want: want_success,
        "args={extra_args:?}; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    if want_success {
        let stdout = String::from_utf8(output.stdout).wrap_err("decode Helm output")?;
        color_eyre::eyre::ensure!(stdout.contains("result: \"false\""));
    }
    Ok(())
}

fn check_status(extra_args: &[&str], want_success: bool) -> eyre::Result<()> {
    let output = render(extra_args)?;
    sim_assert_eq!(
        have: output.status.success(),
        want: want_success,
        "args={extra_args:?}; stderr={}",
        String::from_utf8_lossy(&output.stderr),
    );
    Ok(())
}

#[test]
fn direct_and_evaluated_binding_selectors_keep_distinct_nil_boundaries() -> eyre::Result<()> {
    check_case(&["--set-string", "mode=original"], false)?;
    check_case(
        &["--set-string", "mode=original", "--set-json", "probe=null"],
        false,
    )?;
    check_case(
        &["--set-string", "mode=original", "--set", "probe.marker=1"],
        true,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=original",
            "--set-string",
            "probe=wrong",
        ],
        false,
    )?;

    check_case(&["--set-string", "mode=named"], true)?;
    check_case(
        &["--set-string", "mode=named", "--set-json", "probe=null"],
        true,
    )?;
    check_case(
        &["--set-string", "mode=named", "--set", "probe.marker=1"],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=named",
            "--set",
            "probe.marker=1",
            "--set-json",
            "probe.child=null",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=named",
            "--set",
            "probe.marker=1",
            "--set-string",
            "probe.child=wrong",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=named",
            "--set",
            "probe.marker=1",
            "--set",
            "probe.child.marker=1",
        ],
        true,
    )?;

    check_case(&["--set-string", "mode=rebound"], true)?;
    check_case(
        &["--set-string", "mode=rebound", "--set-json", "probe=null"],
        true,
    )?;
    check_case(
        &["--set-string", "mode=rebound", "--set", "probe.marker=1"],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=rebound",
            "--set",
            "probe.marker=1",
            "--set-json",
            "probe.child=null",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=rebound",
            "--set",
            "probe.marker=1",
            "--set-string",
            "probe.child=wrong",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=rebound",
            "--set",
            "probe.marker=1",
            "--set",
            "probe.child.marker=1",
        ],
        true,
    )?;
    Ok(())
}

#[test]
fn evaluated_helper_dot_selector_keeps_receiver_and_leaf_boundaries() -> eyre::Result<()> {
    check_case(&["--set-string", "mode=helper-dot"], true)?;
    check_case(
        &[
            "--set-string",
            "mode=helper-dot",
            "--set-json",
            "probe=null",
        ],
        true,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-dot",
            "--set-string",
            "probe=wrong",
        ],
        false,
    )?;
    check_case(
        &["--set-string", "mode=helper-dot", "--set", "probe.marker=1"],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-dot",
            "--set",
            "probe.marker=1",
            "--set-json",
            "probe.child=null",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-dot",
            "--set",
            "probe.marker=1",
            "--set-string",
            "probe.child=wrong",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-dot",
            "--set",
            "probe.marker=1",
            "--set",
            "probe.child.marker=1",
        ],
        true,
    )?;
    Ok(())
}

#[test]
fn exact_range_binding_keeps_direct_member_boundaries() -> eyre::Result<()> {
    check_case(&["--set-string", "mode=range-list"], false)?;
    check_case(
        &[
            "--set-string",
            "mode=range-list",
            "--set-json",
            "probe=null",
        ],
        false,
    )?;
    check_case(
        &["--set-string", "mode=range-list", "--set", "probe.marker=1"],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=range-list",
            "--set",
            "probe.marker=1",
            "--set-json",
            "probe.child=null",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=range-list",
            "--set",
            "probe.marker=1",
            "--set-string",
            "probe.child=wrong",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=range-list",
            "--set",
            "probe.marker=1",
            "--set",
            "probe.child.marker=1",
        ],
        true,
    )?;
    Ok(())
}

#[test]
fn helper_range_binding_keeps_direct_member_boundaries() -> eyre::Result<()> {
    check_case(&["--set-string", "mode=helper-range"], true)?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=null",
        ],
        true,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=[]",
        ],
        true,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=[null]",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=[{}]",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=[{\"child\":null}]",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=[{\"child\":\"wrong\"}]",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=helper-range",
            "--set-json",
            "items=[{\"child\":{}}]",
        ],
        true,
    )?;
    Ok(())
}

#[test]
fn branch_root_reassignment_keeps_mode_with_each_arm() -> eyre::Result<()> {
    check_case(&["--set-string", "mode=branch-root"], false)?;
    check_case(
        &["--set-string", "mode=branch-root", "--set", "rebind=true"],
        true,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=branch-root",
            "--set",
            "rebind=true",
            "--set-json",
            "probe=null",
        ],
        true,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=branch-root",
            "--set",
            "rebind=true",
            "--set",
            "probe.marker=1",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=branch-root",
            "--set",
            "rebind=true",
            "--set",
            "probe.marker=1",
            "--set-json",
            "probe.child=null",
        ],
        false,
    )?;
    check_case(
        &[
            "--set-string",
            "mode=branch-root",
            "--set",
            "rebind=true",
            "--set",
            "probe.marker=1",
            "--set",
            "probe.child.marker=1",
        ],
        true,
    )?;
    Ok(())
}

#[test]
fn with_join_conditions_the_direct_range_fallthrough() -> eyre::Result<()> {
    check_status(
        &[
            "--set-string",
            "mode=with-range",
            "--set-json",
            "items=[null]",
            "--set-json",
            "override={\"a\":1}",
        ],
        true,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=with-range",
            "--set-json",
            "items=[null]",
        ],
        false,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=with-range",
            "--set-json",
            "items=[{}]",
        ],
        true,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=with-range",
            "--set-json",
            "items=[{}]",
            "--set-string",
            "override=wrong",
        ],
        false,
    )?;
    Ok(())
}

#[test]
fn pristine_root_values_selector_requires_intermediate_hosts() -> eyre::Result<()> {
    for mode in ["root-document", "root-helper"] {
        check_status(&["--set-string", &format!("mode={mode}")], false)?;
        check_status(
            &[
                "--set-string",
                &format!("mode={mode}"),
                "--set-json",
                "a=null",
            ],
            false,
        )?;
        check_status(
            &[
                "--set-string",
                &format!("mode={mode}"),
                "--set-string",
                "a=wrong",
            ],
            false,
        )?;
        check_status(
            &[
                "--set-string",
                &format!("mode={mode}"),
                "--set",
                "a.marker=1",
            ],
            true,
        )?;
        check_status(
            &[
                "--set-string",
                &format!("mode={mode}"),
                "--set",
                "a.marker=1",
                "--set-json",
                "a.b=null",
            ],
            true,
        )?;
    }
    Ok(())
}

#[test]
fn range_header_assignment_preserves_zero_and_last_write_semantics() -> eyre::Result<()> {
    check_status(&["--set-string", "mode=range-assign-empty"], false)?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-one",
            "--set-json",
            "only={}",
        ],
        true,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-one",
            "--set-string",
            "only=wrong",
        ],
        false,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-order",
            "--set-string",
            "first=wrong",
            "--set-json",
            "last={}",
        ],
        true,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-order",
            "--set-json",
            "first={}",
            "--set-string",
            "last=wrong",
        ],
        false,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-declare-shadow",
            "--set-json",
            "first={}",
            "--set-string",
            "last=wrong",
        ],
        true,
    )?;
    Ok(())
}

#[test]
fn symbolic_range_assignment_uses_the_runtime_last_member() -> eyre::Result<()> {
    check_status(
        &[
            "--set-string",
            "mode=range-assign-symbolic",
            "--set-json",
            "items={}",
            "--set-json",
            "stable={}",
        ],
        true,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-symbolic",
            "--set-json",
            "items=[]",
            "--set-json",
            "stable={}",
        ],
        false,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-symbolic",
            "--set-json",
            "items=[null,{}]",
            "--set-json",
            "stable={}",
        ],
        true,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-symbolic",
            "--set-json",
            "items=[{},null]",
            "--set-json",
            "stable={}",
        ],
        false,
    )?;
    check_status(
        &[
            "--set-string",
            "mode=range-assign-symbolic",
            "--set-json",
            "items={}",
            "--set-json",
            "stable=null",
        ],
        false,
    )?;
    Ok(())
}

#[test]
fn decision_owned_consumers_and_control_exits_match_helm() -> eyre::Result<()> {
    for (mode, values, want) in [
        (
            "string-branch",
            r#"{"useB":true,"a":{"k":1},"b":"beta"}"#,
            true,
        ),
        (
            "string-branch",
            r#"{"useB":false,"a":"alpha","b":{"k":1}}"#,
            true,
        ),
        (
            "default-branch",
            r#"{"chooseP":true,"p":{"k":1},"q":"fallback"}"#,
            true,
        ),
        (
            "default-branch",
            r#"{"chooseP":false,"p":"fallback","q":{"k":1}}"#,
            true,
        ),
        (
            "if-declare-shadow",
            r#"{"before":"wrong","after":{}}"#,
            false,
        ),
        (
            "if-declare-shadow",
            r#"{"before":{},"after":"wrong"}"#,
            false,
        ),
        ("if-declare-shadow", r#"{"before":{},"after":{}}"#, true),
        ("if-assign-header", r#"{"before":"wrong","after":{}}"#, true),
        (
            "if-assign-header",
            r#"{"before":{},"after":"wrong"}"#,
            false,
        ),
        (
            "if-assign-no-else",
            r#"{"before":"wrong","after":{}}"#,
            true,
        ),
        (
            "if-assign-no-else",
            r#"{"before":{},"after":"wrong"}"#,
            false,
        ),
        ("with-shadow", r#"{"a":{},"b":"inside"}"#, true),
        ("with-shadow", r#"{"a":{},"b":false}"#, false),
        (
            "with-assign-header",
            r#"{"before":"wrong","after":{}}"#,
            true,
        ),
        (
            "with-assign-header",
            r#"{"before":{},"after":"wrong"}"#,
            false,
        ),
        ("range-declare-else", r#"{"before":{},"items":[]}"#, false),
        ("range-declare-else", r#"{"before":{},"items":[{}]}"#, true),
        (
            "escaped-interleaving",
            r#"{"enabled":true,"before":{},"after":{}}"#,
            true,
        ),
        (
            "escaped-interleaving",
            r#"{"enabled":true,"before":"wrong","after":{}}"#,
            false,
        ),
        (
            "escaped-interleaving",
            r#"{"enabled":true,"before":{},"after":"wrong"}"#,
            false,
        ),
        (
            "range-break",
            r#"{"stop":true,"first":{},"last":"wrong"}"#,
            true,
        ),
        (
            "range-break",
            r#"{"stop":false,"first":"wrong","last":{}}"#,
            true,
        ),
        (
            "range-continue",
            r#"{"skipLast":true,"last":{},"after":"wrong"}"#,
            true,
        ),
        (
            "range-continue",
            r#"{"skipLast":false,"last":"wrong","after":{}}"#,
            true,
        ),
        (
            "range-approx-break-exact",
            r#"{"stop":"yes","first":{},"last":{}}"#,
            true,
        ),
        (
            "range-approx-break-symbolic",
            r#"{"stop":"yes","items":[{}]}"#,
            true,
        ),
        ("range-choice", r#"{"pickA":true,"a":{},"b":"wrong"}"#, true),
        (
            "range-choice",
            r#"{"pickA":false,"a":"wrong","b":{}}"#,
            true,
        ),
    ] {
        check_status(
            &[
                "--set-string",
                &format!("mode={mode}"),
                "--set-json",
                values,
            ],
            want,
        )?;
    }
    Ok(())
}
