use color_eyre::eyre::{self, OptionExt as _};
use test_util::helm_values::AcceptanceDocument;
use test_util::prelude::sim_assert_eq;

use super::HelmAdjudicationCoverage;
use super::known_false_acceptances::{ContextSelector, ContextTransport};
use super::known_inherited_failures::{
    FailureClass, FailureSelector, KnownInheritedFailure, ObservedFailure,
    validate_inherited_failures,
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
    );
    sim_assert_eq!(have: &failure.reason, want: "exact failure");
    sim_assert_eq!(have: failure.evidence.as_deref(), want: Some(std::path::Path::new("/preserved/control")));
    assert!(row().lists(&failure)?);
    let changed = ObservedFailure::from_adjudication(
        case.clone(),
        "1.29.0".into(),
        &format!("{case}: different failure; case={probe}; evidence=/preserved/control"),
    );
    assert!(!row().lists(&changed)?);
    Ok(())
}

#[test]
fn inherited_oracle_false_alarm_requires_exact_saved_evidence() -> eyre::Result<()> {
    use super::known_false_acceptances::{Baseline, Rejection};
    use super::known_inherited_failures::oracle_false_acceptance_is_listed;

    let evidence = test_util::scratch::ScratchDir::new("inherited-oracle")?;
    let rows = [KnownInheritedFailure {
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
        "kubernetes_version": "1.29.0", "transport": "values-file"
    });
    std::fs::write(&report, serde_json::to_vec(&exact)?)?;
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
