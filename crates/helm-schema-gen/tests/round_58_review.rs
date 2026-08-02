//! Helm adjudications for the round-58 analyzer regressions.

use std::process::{Command, Output};

use color_eyre::eyre::{self, WrapErr as _};

fn render(extra_args: &[&str]) -> eyre::Result<Output> {
    let chart = test_util::workspace_testdata().join("charts/round-58-review");
    Command::new("helm")
        .arg("template")
        .arg("round-58")
        .arg(chart)
        .arg("--skip-schema-validation")
        .args(extra_args)
        .output()
        .wrap_err("run round-58 Helm reproducer")
}

fn rendered(extra_args: &[&str]) -> eyre::Result<String> {
    let output = render(extra_args)?;
    color_eyre::eyre::ensure!(
        output.status.success(),
        "Helm unexpectedly aborted: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).wrap_err("decode round-58 Helm output")
}

#[test]
fn encoded_printf_and_dormant_default_operands_render() -> eyre::Result<()> {
    rendered(&["--set", "case=printf-encoded,user=null"])?;
    rendered(&["--set", "case=printf-default,fallback=null"])?;

    let abort = render(&["--set", "case=printf-default,primary=null,fallback=null"])?;
    color_eyre::eyre::ensure!(
        !abort.status.success(),
        "Helm accepted a token-opening printf diagnostic"
    );
    Ok(())
}

#[test]
fn invalid_kind_ternary_uses_the_selected_literal_arm() -> eyre::Result<()> {
    rendered(&["--set", "case=kind-invalid,value=null,enabled=false"])?;
    Ok(())
}

#[test]
fn else_with_tests_its_own_subject_before_the_body() -> eyre::Result<()> {
    let output = rendered(&["--set", "case=else-with,second=,payload.invalid=ignored"])?;
    color_eyre::eyre::ensure!(output.contains("payload: dormant"));
    Ok(())
}

#[test]
fn hyphenated_literal_patterns_match_helm_text() -> eyre::Result<()> {
    rendered(&["--set", "case=hyphen-regex,payload=left-right"])?;
    Ok(())
}

#[test]
fn signed_underscored_radix_spellings_render_as_plain_scalars() -> eyre::Result<()> {
    for spelling in ["+_0x1f", "+_08"] {
        rendered(&["--set-string", &format!("case=radix,radix={spelling}")])?;
    }
    Ok(())
}

#[test]
fn quoted_negative_zero_preserves_its_helm_spelling() -> eyre::Result<()> {
    let output = rendered(&["--set-string", "case=negative-zero,zero=-0"])?;
    color_eyre::eyre::ensure!(output.contains("contains-hyphen: \"true\""));
    Ok(())
}

#[test]
fn whole_global_range_sees_each_coalesced_source() -> eyre::Result<()> {
    let output = rendered(&["--set", "global.root=root"])?;
    for key in ["root", "mid", "agent"] {
        color_eyre::eyre::ensure!(
            output.contains(&format!("name: global-{key}")),
            "missing coalesced global source {key}"
        );
    }
    Ok(())
}

#[test]
fn dependency_global_default_supplies_whole_map_operands() -> eyre::Result<()> {
    let output = rendered(&[])?;
    color_eyre::eyre::ensure!(output.contains("has-agent: \"true\""));
    Ok(())
}
