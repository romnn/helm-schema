//! Lint classifier parity and separation of values and template rule evidence.

use color_eyre::eyre;
use helm_schema_test_support::helm::lint::{
    LintClassification, LintErrorKind, LintRule, classify_lint,
};
use serde::Deserialize;
use test_util::prelude::sim_assert_eq;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    mode: String,
    rc: i32,
    log: String,
    class: String,
}

#[test]
fn lint_classes_match_sweep_fixtures() -> eyre::Result<()> {
    let fixtures: Vec<Fixture> = serde_json::from_str(include_str!(
        "../../../tools/helmsweep/testdata/helm-classes.json"
    ))?;
    let mut checked = 0;
    for fixture in fixtures
        .into_iter()
        .filter(|fixture| fixture.mode == "lint")
    {
        sim_assert_eq!(
            have: (fixture.name.clone(), classify_lint(fixture.rc, fixture.log.as_bytes()).class()),
            want: (fixture.name.clone(), fixture.class)
        );
        checked += 1;
    }
    assert!(checked > 20);
    Ok(())
}

#[test]
fn lint_errors_retain_each_rules_document_and_diagnostic() -> eyre::Result<()> {
    let log = indoc::indoc! {"
        ==> Linting chart
        [ERROR] values.yaml: raw document rejected
        [ERROR] templates/: values don't meet the specifications of the schema(s)
        [ERROR] templates/: execution error at (chart/templates/a.yaml:1:3): boom
        Error: 1 chart(s) linted, 1 chart(s) failed
    "};
    let LintClassification::Reject { errors } = classify_lint(1, log.as_bytes()) else {
        eyre::bail!("recognized lint rules did not yield rejection evidence");
    };
    let observed: Vec<_> = errors
        .iter()
        .map(|error| (error.rule, error.kind))
        .collect();
    sim_assert_eq!(have: observed, want: vec![
        (LintRule::Values, LintErrorKind::RawValuesSchema),
        (LintRule::Templates, LintErrorKind::TemplatesSchema),
        (LintRule::Templates, LintErrorKind::Semantic),
    ]);
    assert!(errors.iter().all(|error| log.contains(&error.diagnostic)));
    Ok(())
}

#[cfg(unix)]
#[test]
fn lint_uses_cli_identity_schema_validation_and_its_own_replay_key() -> eyre::Result<()> {
    use helm_schema_test_support::helm::invocation::{
        Cacheability, HelmRunner, InvocationRequest, LINT_TIMEOUT, Outcome, TemplateRequest,
    };
    use std::fs;
    use std::os::unix::fs::PermissionsExt as _;
    let scratch = test_util::scratch::ScratchDir::new("lint-invocation")?;
    let program = scratch.path().join("helm");
    fs::write(
        &program,
        indoc::indoc! {r#"
        #!/bin/sh
        case "$1" in
          version) printf v4.2.3 ;;
          lint) printf '1 chart(s) linted, 0 chart(s) failed\n' ;;
          template) printf 'rendered\n' ;;
          *) exit 2 ;;
        esac
    "#},
    )?;
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755))?;
    let runner = HelmRunner::with_program(&scratch.path().join("runner"), true, program)?;
    let staged = runner.staging_dir()?;
    fs::write(
        staged.join("Chart.yaml"),
        "apiVersion: v2\nname: lint-test\nversion: 1.0.0\n",
    )?;
    let chart = runner.publish_tree(&staged)?;
    let case = scratch.path().join("case");
    fs::create_dir(&case)?;
    let request = TemplateRequest {
        chart: &chart,
        values: b"{}",
        kubernetes_version: "1.35.0",
    };
    let policy = helm_schema_test_support::helm::cache_policy::render_cacheability(chart.path())?;
    let rendered = runner.template(&request, &case, "render", &policy)?;
    let lint = runner.lint(&request, &case, "lint", &policy)?;
    let replay = runner.lint(&request, &case, "lint-again", &policy)?;
    assert_ne!(lint.record.key, rendered.record.key);
    sim_assert_eq!(have: replay.record.outcome, want: Outcome::Replayed);
    sim_assert_eq!(have: LINT_TIMEOUT.as_secs(), want: 1200);
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(case.join("lint.invocation.json"))?)?;
    let invocation: InvocationRequest = serde_json::from_value(record["request"].clone())?;
    sim_assert_eq!(have: invocation.engine, want: "helm-cli");
    sim_assert_eq!(have: invocation.arguments.first().map(String::as_str), want: Some("lint"));
    assert!(
        !invocation
            .arguments
            .iter()
            .any(|argument| argument == "--skip-schema-validation")
    );
    let bypass = Cacheability::bypass("test".to_string());
    let uncached = runner.lint(&request, &case, "uncached", &bypass)?;
    sim_assert_eq!(have: uncached.record.outcome, want: Outcome::Bypassed("test".to_string()));
    Ok(())
}
