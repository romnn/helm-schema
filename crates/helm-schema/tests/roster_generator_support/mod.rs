//! Uses the battery's diagnostic normalization when generating exact oracle-gap rows.

use color_eyre::eyre;
use std::collections::BTreeMap;
use std::path::PathBuf;
use test_util::prelude::sim_assert_eq;

pub(crate) mod encoding_controls;

#[test]
#[ignore = "maintenance: tools/roster-generator/generate.py supplies the request and result paths"]
fn export_lint_digests() -> eyre::Result<()> {
    let request = std::env::var("ROSTER_GENERATOR_REQUEST")?;
    let result = std::env::var("ROSTER_GENERATOR_RESULT")?;
    let paths: Vec<PathBuf> = serde_json::from_slice(&std::fs::read(request)?)?;
    let mut digests = BTreeMap::new();
    for path in paths {
        digests.insert(
            path.clone(),
            super::known_inherited_failures::lint_diagnostics_sha256(&path)?,
        );
    }
    std::fs::write(result, serde_json::to_vec_pretty(&digests)?)?;
    Ok(())
}

#[test]
fn residual_family_labels_round_trip() -> eyre::Result<()> {
    use super::known_false_acceptances::Family;

    for family in [
        Family::R3bA,
        Family::R3bB,
        Family::R3bC,
        Family::R3bD,
        Family::R3bE,
        Family::R3bF,
    ] {
        let label = format!("{family:?}");
        sim_assert_eq!(have: label.parse::<Family>().map_err(eyre::Report::msg)?, want: family);
    }
    Ok(())
}

#[test]
fn exact_structural_rows_bind_names_verdicts_and_staleness() -> eyre::Result<()> {
    use super::known_false_acceptances::{Baseline, ExactFalseAcceptance, Family, Rejection};
    use super::known_undecided_acceptances::ExactUndecidedAcceptance;

    let false_row = ExactFalseAcceptance {
        chart: "control",
        probe: "root guard 1 satisfied [targeted: a + b]",
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsUnlikeItsDefaults,
        family: Family::R3bA,
    };
    let undecided_row = ExactUndecidedAcceptance {
        chart: "control",
        probe: "defaults",
        uncertain: &["first", "second"],
    };
    let mut coverage = super::HelmAdjudicationCoverage::default();
    coverage.charts_adjudicated.insert("control".into());
    let false_rows = std::slice::from_ref(&false_row);
    let undecided_rows = std::slice::from_ref(&undecided_row);
    sim_assert_eq!(have: super::exact_rows_not_observed(&coverage, false_rows, undecided_rows).len(), want: 2);
    let mut false_observation = super::ObservedFalseAcceptance {
        case: format!("control: {}", false_row.probe),
        rejection: false_row.rejection,
        baseline: false_row.baseline,
        evidence: None,
    };
    assert!(false_row.lists(&false_observation));
    false_observation.baseline = Baseline::RejectsItsDefaults;
    assert!(!false_row.lists(&false_observation));
    false_observation.baseline = false_row.baseline;
    false_observation.case.push_str(" extra");
    assert!(!false_row.lists(&false_observation));
    false_observation.case = format!("control: {}", false_row.probe);
    coverage.false_acceptances.push(false_observation);
    let mut undecided_observation = super::ObservedUndecidedAcceptance {
        case: "control: defaults".into(),
        uncertain: vec!["second".into(), "first".into()],
        evidence: None,
    };
    assert!(!undecided_row.lists(&undecided_observation));
    undecided_observation.uncertain.reverse();
    assert!(undecided_row.lists(&undecided_observation));
    coverage
        .loosenings_with_uncertain_kubernetes
        .push(undecided_observation);
    sim_assert_eq!(have: super::exact_rows_not_observed(&coverage, false_rows, undecided_rows), want: Vec::<String>::new());
    coverage.flips_adjudicated = 2;
    super::validate_helm_adjudication_coverage_with_rosters(
        &coverage,
        &[],
        &[],
        &[],
        &[],
        super::ExactRosters {
            false_acceptances: false_rows,
            undecided_acceptances: undecided_rows,
        },
    )?;
    assert!(
        super::validate_helm_adjudication_coverage_with_contexts(&coverage, &[], &[], &[], &[],)
            .is_err()
    );
    coverage.false_acceptances.clear();
    coverage.flips_adjudicated = 1;
    assert!(
        super::validate_helm_adjudication_coverage_with_rosters(
            &coverage,
            &[],
            &[],
            &[],
            &[],
            super::ExactRosters {
                false_acceptances: false_rows,
                undecided_acceptances: undecided_rows,
            },
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn exact_structural_rows_cannot_authorize_context_documents() {
    use super::known_false_acceptances::{Baseline, ExactFalseAcceptance, Family, Rejection};
    use super::known_undecided_acceptances::ExactUndecidedAcceptance;

    let probe = "document=Template transport=values-file kube=1.29.0 overlay={}";
    let false_row = ExactFalseAcceptance {
        chart: "control",
        probe,
        rejection: Rejection::HelmAborts,
        baseline: Baseline::RejectsItsDefaults,
        family: Family::R3bA,
    };
    assert!(!false_row.lists(&super::ObservedFalseAcceptance {
        case: format!("control: {probe}"),
        rejection: false_row.rejection,
        baseline: false_row.baseline,
        evidence: None,
    }));
    let undecided_row = ExactUndecidedAcceptance {
        chart: "control",
        probe,
        uncertain: &["unknown resource"],
    };
    assert!(!undecided_row.lists(&super::ObservedUndecidedAcceptance {
        case: format!("control: {probe}"),
        uncertain: vec!["unknown resource".into()],
        evidence: None,
    }));
}

#[test]
fn original_control_encodings_remain_distinct() {
    use super::known_inherited_failures::FailureSelector;

    let mut inputs = BTreeMap::new();
    for row in encoding_controls::ENCODING_CONTROLS {
        sim_assert_eq!(have: row.input.values_file_sha256.len(), want: 64);
        sim_assert_eq!(have: row.input.controls_sha256.len(), want: 64);
        assert!(
            row.lint_diagnostics_sha256
                .as_ref()
                .is_some_and(|digest| digest.len() == 64)
        );
        if let FailureSelector::Context(selector) = &row.selector
            && row.chart == "oncall"
            && selector.overlay == r#"{"postgresql":{"enabled":true}}"#
        {
            *inputs
                .entry(row.input.values_file_sha256.to_string())
                .or_insert(0) += 1;
        }
    }
    sim_assert_eq!(have: inputs, want: BTreeMap::from([
        ("0e17ad3087e6ccb8fabde25fade9373058646399a5cc600251408d1c66660ecd".to_owned(), 2),
        ("1f81105527cccb13e752dba9ad9b2ee2040e6ce3020a3edd6b77c5f0bd2f820b".to_owned(), 2),
    ]));
}
