use color_eyre::eyre::{self, OptionExt as _};
use test_util::helm_values::AcceptanceDocument;
use test_util::prelude::sim_assert_eq;

use super::HelmAdjudicationCoverage;
use super::known_false_acceptances::{ContextSelector, ContextTransport};
use super::known_inherited_failures::{
    FailureClass, FailureInput, FailureSelector, KnownInheritedFailure, ObservedFailure,
    lint_diagnostics_sha256, validate_inherited_failures,
};

fn selector() -> ContextSelector {
    ContextSelector {
        document: AcceptanceDocument::LintRaw,
        overlay: r#"{"dependency":{"enabled":null}}"#,
        transport: ContextTransport::ValuesFile,
        kubernetes_version: "1.29.0",
    }
}

fn row() -> KnownInheritedFailure {
    KnownInheritedFailure {
        chart: "control",
        selector: FailureSelector::Context(selector()),
        failure: FailureClass::Adjudication {
            reason: "exact failure",
        },
        input: FailureInput {
            values_file_sha256: "original".into(),
            controls_sha256: "controls".into(),
        },
        lint_diagnostics_sha256: None,
        owner: "generator: null deletion",
        observed_on: "landing10a/c23467f4",
    }
}

fn coverage() -> eyre::Result<HelmAdjudicationCoverage> {
    Ok(HelmAdjudicationCoverage {
        enabled: true,
        charts_adjudicated: ["control".into()].into(),
        adjudication_failures: vec![ObservedFailure {
            case: selector().case("control")?,
            kubernetes_version: "1.29.0".into(),
            reason: "exact failure".into(),
            input: Some(row().input),
            evidence: None,
        }],
        ..Default::default()
    })
}

#[test]
fn inherited_listed_hard_failure_passes() -> eyre::Result<()> {
    validate_inherited_failures(&coverage()?, &[row()])?;
    Ok(())
}

#[test]
fn inherited_unlisted_hard_failure_fails() -> eyre::Result<()> {
    assert!(validate_inherited_failures(&coverage()?, &[]).is_err());
    Ok(())
}

#[test]
fn inherited_stale_entry_fails() -> eyre::Result<()> {
    let mut coverage = coverage()?;
    coverage.adjudication_failures.clear();
    assert!(validate_inherited_failures(&coverage, &[row()]).is_err());
    Ok(())
}

#[test]
fn inherited_near_miss_identity_fails() -> eyre::Result<()> {
    // Each change preserves the chart while changing one bound part of the case.
    for changed in [
        ContextSelector {
            overlay: r#"{"dependency":{"enabled":false}}"#,
            ..selector()
        },
        ContextSelector {
            document: AcceptanceDocument::Template,
            ..selector()
        },
        ContextSelector {
            kubernetes_version: "1.33.0",
            ..selector()
        },
    ] {
        let row = KnownInheritedFailure {
            selector: FailureSelector::Context(changed),
            ..row()
        };
        assert!(validate_inherited_failures(&coverage()?, &[row]).is_err());
    }
    let mut coverage = coverage()?;
    coverage.adjudication_failures[0].reason = "different failure".into();
    assert!(validate_inherited_failures(&coverage, &[row()]).is_err());
    Ok(())
}

#[test]
fn inherited_unselected_entry_is_stale() -> eyre::Result<()> {
    let mut coverage = coverage()?;
    coverage.adjudication_failures.clear();
    coverage.charts_adjudicated.clear();
    // Omitted charts cannot make entries disappear from the shrink-only contract.
    assert!(validate_inherited_failures(&coverage, &[row()]).is_err());
    coverage.charts_adjudicated.insert("control".into());
    assert!(validate_inherited_failures(&coverage, &[row()]).is_err());
    Ok(())
}

#[test]
fn inherited_duplicate_or_unowned_entry_fails() -> eyre::Result<()> {
    assert!(validate_inherited_failures(&coverage()?, &[row(), row()]).is_err());
    for incomplete in [
        KnownInheritedFailure { owner: "", ..row() },
        KnownInheritedFailure {
            observed_on: "",
            ..row()
        },
    ] {
        assert!(validate_inherited_failures(&coverage()?, &[incomplete]).is_err());
    }
    Ok(())
}

#[test]
fn inherited_legacy_selector_binds_exact_probe_and_version() -> eyre::Result<()> {
    let row = KnownInheritedFailure {
        selector: FailureSelector::Probe {
            probe: super::Probe {
                path: "scope.enabled",
                value: "false",
            },
            kubernetes_version: "1.29.0",
        },
        ..row()
    };
    let mut coverage = coverage()?;
    coverage.adjudication_failures[0].case = "control: scope.enabled <- false".into();
    assert!(row.lists(&coverage.adjudication_failures[0])?);
    coverage.adjudication_failures[0].kubernetes_version = "1.33.0".into();
    assert!(!row.lists(&coverage.adjudication_failures[0])?);
    coverage.adjudication_failures[0].kubernetes_version = "1.29.0".into();
    coverage.adjudication_failures[0].case =
        "control: guard [targeted: scope.enabled <- false]".into();
    assert!(!row.lists(&coverage.adjudication_failures[0])?);
    Ok(())
}

#[test]
fn inherited_reason_separates_exact_diagnostics_from_evidence_locations() -> eyre::Result<()> {
    let case = selector().case("control")?;
    let (_, probe) = case.split_once(": ").ok_or_eyre("case has no chart")?;
    let failure = ObservedFailure::from_adjudication(
        case.clone(),
        "1.29.0".into(),
        &format!("{case}: exact failure; case={probe}; evidence=/preserved/control"),
        Some(row().input),
    );
    sim_assert_eq!(have: &failure.reason, want: "exact failure");
    sim_assert_eq!(have: failure.evidence.as_deref(), want: Some(std::path::Path::new("/preserved/control")));
    assert!(row().lists(&failure)?);
    let changed = ObservedFailure::from_adjudication(
        case.clone(),
        "1.29.0".into(),
        &format!("{case}: different failure; case={probe}; evidence=/preserved/control"),
        Some(row().input),
    );
    assert!(!row().lists(&changed)?);
    Ok(())
}

#[test]
fn inherited_oracle_false_alarm_requires_exact_saved_evidence() -> eyre::Result<()> {
    use super::known_false_acceptances::{Baseline, Rejection};
    use super::known_inherited_failures::oracle_false_acceptance_is_listed;

    let evidence = test_util::scratch::ScratchDir::new("inherited-oracle")?;
    let mut rows = [KnownInheritedFailure {
        failure: FailureClass::OracleFalseAcceptance {
            rejection: Rejection::KubernetesRejects,
            baseline: Baseline::RejectsItsDefaults,
            kubernetes_errors: &["CRD A v1 is not served", "CRD B v1 is not served"],
        },
        ..row()
    }];
    let mut coverage = coverage()?;
    coverage.adjudication_failures.clear();
    coverage.record_verdict(
        super::HelmFlipVerdict::UninformativeBaselineFalseAcceptance(Rejection::KubernetesRejects),
        selector().case("control")?,
        Some(evidence.path().to_path_buf()),
    );
    let report = evidence.path().join("schema-verdicts.json");
    let exact = serde_json::json!({
        "new_kubernetes_errors": ["CRD A v1 is not served", "CRD B v1 is not served"],
        "kubernetes_version": "1.29.0", "transport": "values-file",
        "failure_input": FailureInput::new(b"{}", &[serde_json::json!({})])?
    });
    std::fs::write(&report, serde_json::to_vec(&exact)?)?;
    write_lint_evidence(evidence.path(), b"{}", "")?;
    rows[0].input = FailureInput::new(b"{}", &[serde_json::json!({})])?;
    rows[0].lint_diagnostics_sha256 = Some(lint_diagnostics_sha256(evidence.path())?.into());
    sim_assert_eq!(have: validate_inherited_failures(&coverage, &rows)?, want: 1);
    for (field, replacement) in [
        (
            "new_kubernetes_errors",
            serde_json::json!(["different resource is invalid"]),
        ),
        (
            "new_kubernetes_errors",
            serde_json::json!(["CRD B v1 is not served", "CRD A v1 is not served"]),
        ),
        ("kubernetes_version", serde_json::json!("1.33.0")),
        ("transport", serde_json::json!("set-string")),
    ] {
        let mut changed = exact.clone();
        changed[field] = replacement;
        std::fs::write(&report, serde_json::to_vec(&changed)?)?;
        assert!(validate_inherited_failures(&coverage, &rows).is_err());
    }
    std::fs::write(&report, serde_json::to_vec(&exact)?)?;
    for observed in &mut coverage.false_acceptances {
        observed.evidence = None;
        assert!(!oracle_false_acceptance_is_listed(&rows, observed)?);
    }
    assert!(validate_inherited_failures(&coverage, &rows).is_err());
    Ok(())
}

fn write_lint_evidence(
    directory: &std::path::Path,
    values: &[u8],
    diagnostic: &str,
) -> eyre::Result<()> {
    std::fs::create_dir_all(directory)?;
    std::fs::write(directory.join("values.json"), values)?;
    for stage in ["baseline", "candidate", "schema-free"] {
        let rejects = stage == "candidate" && !diagnostic.is_empty();
        let output = if rejects {
            indoc::formatdoc! {"
                ==> Linting /temporary/chart
                {diagnostic}
            "}
        } else {
            indoc::indoc! {"
                ==> Linting /temporary/chart
                1 chart(s) linted, 0 chart(s) failed
            "}
            .to_owned()
        };
        std::fs::write(directory.join(format!("lint-{stage}.yaml")), output)?;
        std::fs::write(
            directory.join(format!("lint-{stage}.stderr")),
            if rejects {
                indoc::indoc! {"
                    Error: 1 chart(s) linted, 1 chart(s) failed
                "}
            } else {
                ""
            },
        )?;
        std::fs::write(
            directory.join(format!("lint-{stage}.status")),
            format!("exit status: {}", i32::from(rejects)),
        )?;
    }
    Ok(())
}

#[test]
fn inherited_original_bytes_and_controls_are_separate_obligations() -> eyre::Result<()> {
    let values = [
        br#"{"postgresql":{"enabled":true}}"#.as_slice(),
        br#"{"postgresql": {"enabled": true}}"#.as_slice(),
    ];
    let controls = [serde_json::json!({})];
    let first = KnownInheritedFailure {
        input: FailureInput::new(values[0], &controls)?,
        ..row()
    };
    let second = KnownInheritedFailure {
        input: FailureInput::new(values[1], &controls)?,
        ..row()
    };
    sim_assert_eq!(have: first.input.values_file_sha256.as_ref(), want: "0e17ad3087e6ccb8fabde25fade9373058646399a5cc600251408d1c66660ecd");
    sim_assert_eq!(have: second.input.values_file_sha256.as_ref(), want: "1f81105527cccb13e752dba9ad9b2ee2040e6ce3020a3edd6b77c5f0bd2f820b");
    let mut coverage = coverage()?;
    coverage.adjudication_failures[0].input = Some(first.input.clone());
    coverage.adjudication_failures.push(ObservedFailure {
        case: selector().case("control")?,
        kubernetes_version: "1.29.0".into(),
        reason: "exact failure".into(),
        input: Some(second.input.clone()),
        evidence: None,
    });
    validate_inherited_failures(&coverage, &[first, second])?;
    let first = KnownInheritedFailure {
        input: FailureInput::new(values[0], &controls)?,
        ..row()
    };
    let second = KnownInheritedFailure {
        input: FailureInput::new(values[1], &controls)?,
        ..row()
    };
    coverage.adjudication_failures.pop();
    assert!(validate_inherited_failures(&coverage, &[first, second]).is_err());
    for input in [
        FailureInput::new(br#"{ "postgresql": { "enabled": true } }"#, &controls)?,
        FailureInput::new(values[0], &[serde_json::json!({"context": true})])?,
        FailureInput::new(
            values[0],
            &[serde_json::json!({}), serde_json::json!({"context": true})],
        )?,
    ] {
        coverage.adjudication_failures[0].input = Some(input);
        let first = KnownInheritedFailure {
            input: FailureInput::new(values[0], &controls)?,
            ..row()
        };
        assert!(validate_inherited_failures(&coverage, &[first]).is_err());
    }
    Ok(())
}

#[test]
fn inherited_generic_reason_cannot_hide_a_replacement_lint_diagnostic() -> eyre::Result<()> {
    let evidence = test_util::scratch::ScratchDir::new("inherited-lint-diagnostic")?;
    let original = indoc::indoc! {r#"
        [ERROR] values.yaml: "file:///values.schema.json#" is not valid against metaschema
        - at '/properties/rabbitmq/properties/uri/pattern': '\u' is not valid regex
    "#};
    std::fs::write(evidence.path().join("schema-verdicts.json"), b"{}")?;
    write_lint_evidence(evidence.path(), b"{}", original)?;
    let row = KnownInheritedFailure {
        input: FailureInput::new(b"{}", &[serde_json::json!({})])?,
        lint_diagnostics_sha256: Some(lint_diagnostics_sha256(evidence.path())?.into()),
        ..row()
    };
    let mut coverage = coverage()?;
    coverage.adjudication_failures[0].input = Some(row.input.clone());
    coverage.adjudication_failures[0].evidence = Some(evidence.path().to_owned());
    validate_inherited_failures(&coverage, std::slice::from_ref(&row))?;
    for replacement in [
        original.replace("'\\u' is not valid regex", "'[' is not valid regex"),
        original
            .replace("/uri/pattern", "/uri/type")
            .replace("'\\u' is not valid regex", "42 is not valid type"),
        original.replace("[ERROR] values.yaml:", "[ERROR] templates/values.yaml:"),
    ] {
        write_lint_evidence(evidence.path(), b"{}", &replacement)?;
        assert!(validate_inherited_failures(&coverage, std::slice::from_ref(&row)).is_err());
    }
    write_lint_evidence(evidence.path(), b"{ }", original)?;
    assert!(validate_inherited_failures(&coverage, std::slice::from_ref(&row)).is_err());
    Ok(())
}

#[test]
fn inherited_frozen_roster_binds_every_context_diagnostic_and_original_encoding() {
    use super::known_inherited_failures::KNOWN_INHERITED_FAILURES;

    let mut postgresql_inputs = std::collections::BTreeMap::new();
    for row in KNOWN_INHERITED_FAILURES {
        sim_assert_eq!(have: row.input.values_file_sha256.len(), want: 64);
        sim_assert_eq!(have: row.input.controls_sha256.len(), want: 64);
        if let FailureSelector::Context(selector) = &row.selector {
            assert!(
                row.lint_diagnostics_sha256
                    .as_ref()
                    .is_some_and(|digest| digest.len() == 64)
            );
            if row.chart == "oncall" && selector.overlay == r#"{"postgresql":{"enabled":true}}"# {
                *postgresql_inputs
                    .entry(row.input.values_file_sha256.to_string())
                    .or_insert(0) += 1;
            }
        }
    }
    sim_assert_eq!(have: postgresql_inputs, want: std::collections::BTreeMap::<String, i32>::new());
}

#[test]
fn inherited_legacy_input_binds_the_file_actually_read_by_helm() -> eyre::Result<()> {
    let directory = test_util::scratch::ScratchDir::new("inherited-legacy-input")?;
    std::fs::write(
        directory.path().join("Chart.yaml"),
        indoc::indoc! {"
        apiVersion: v2
        name: inherited-input
        version: 0.1.0
    "},
    )?;
    std::fs::write(directory.path().join("values.yaml"), b"{}")?;
    let probe = super::lint_probes::ScreenedProbe {
        name: "postgresql.enabled <- true".to_owned(),
        overlay: serde_json::json!({"postgresql": {"enabled": true}}),
        document: None,
    };
    let chart = super::PinnedHelmChart::prepare(
        super::helm_invocation::HelmRunner::shared()?,
        directory.path(),
    )?;
    let execution = chart.adjudicate(&probe.overlay)?;
    let actual = std::fs::read(execution.evidence_dir.join("values.json"))?;
    let input = FailureInput::for_probe(&probe)?;
    sim_assert_eq!(have: input.values_file_sha256.as_ref(), want: helm_schema_test_support::source_digest::sha256_hex(&actual));
    assert_ne!(actual, serde_json::to_vec(&probe.overlay)?);
    Ok(())
}

#[test]
fn inherited_schema_diagnostics_ignore_sibling_order_but_preserve_parent_and_multiplicity()
-> eyre::Result<()> {
    let evidence = test_util::scratch::ScratchDir::new("inherited-diagnostic-tree")?;
    std::fs::write(evidence.path().join("schema-verdicts.json"), b"{}")?;
    let header = "[ERROR] values.yaml: - at '': 'allOf' failed";
    let left = indoc::indoc! {"
          - at '/left': validation failed
            - at '/leaf': got string, want boolean
    "};
    let right = indoc::indoc! {"
          - at '/right': validation failed
            - at '/leaf': got string, want integer
    "};
    let original = indoc::formatdoc! {"
        {header}
        {left}{right}
    "};
    write_lint_evidence(evidence.path(), b"{}", &original)?;
    let row = KnownInheritedFailure {
        input: FailureInput::new(b"{}", &[serde_json::json!({})])?,
        lint_diagnostics_sha256: Some(lint_diagnostics_sha256(evidence.path())?.into()),
        ..row()
    };
    let mut coverage = coverage()?;
    coverage.adjudication_failures[0].input = Some(row.input.clone());
    coverage.adjudication_failures[0].evidence = Some(evidence.path().to_owned());
    let reordered = indoc::formatdoc! {"
        {header}
        {right}{left}
    "};
    write_lint_evidence(evidence.path(), b"{}", &reordered)?;
    validate_inherited_failures(&coverage, std::slice::from_ref(&row))?;
    let moved = original
        .replace("want boolean", "want PLACEHOLDER")
        .replace("want integer", "want boolean")
        .replace("want PLACEHOLDER", "want integer");
    for replacement in [
        moved,
        format!("{original}{left}"),
        format!("{original}Error: another diagnostic"),
        format!("{original}[new keyword] another diagnostic"),
    ] {
        write_lint_evidence(evidence.path(), b"{}", &replacement)?;
        assert!(validate_inherited_failures(&coverage, std::slice::from_ref(&row)).is_err());
    }
    Ok(())
}
