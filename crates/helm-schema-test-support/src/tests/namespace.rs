//! Namespace language boundaries and acceptance accounting.

use color_eyre::eyre::{self, OptionExt as _};
use test_util::prelude::sim_assert_eq;

use crate::acceptance::HelmAdjudicationCoverage;
use crate::acceptance::adjudicate::{
    FailureKind, HelmFlipVerdict, NAMESPACE_UNCERTAINTY, OracleFinding, combine_namespace_verdicts,
    namespace_disagreement,
};
use crate::helm::adjudication::{RENDER_NAMESPACE, SECOND_RENDER_NAMESPACE, is_oracle_namespace};

fn observation_directory(label: &str) -> eyre::Result<test_util::scratch::ScratchDir> {
    let directory = test_util::scratch::ScratchDir::new(label)?;
    std::fs::write(directory.path().join("schema-verdicts.json"), b"{}")?;
    Ok(directory)
}

#[test]
fn namespace_language_covers_exactly_rfc1123_labels() {
    sim_assert_eq!(have: SECOND_RENDER_NAMESPACE.len(), want: 63);
    for namespace in [RENDER_NAMESPACE, SECOND_RENDER_NAMESPACE, "a", "0", "a-b0"] {
        assert!(is_oracle_namespace(namespace), "{namespace:?}");
    }
    for namespace in [
        "",
        "A",
        "-a",
        "a-",
        "a.b",
        "a_b",
        "a\n",
        "a\"",
        "é",
        &"a".repeat(64),
    ] {
        assert!(!is_oracle_namespace(namespace), "{namespace:?}");
        assert!(namespace_disagreement(namespace, true, RENDER_NAMESPACE, false).is_err());
        assert!(namespace_disagreement(RENDER_NAMESPACE, true, namespace, false).is_err());
    }
}

#[test]
fn equal_namespace_verdicts_keep_the_ordinary_classification() -> eyre::Result<()> {
    let scratch = observation_directory("namespace-equal")?;
    for verdict in [
        HelmFlipVerdict::CandidateAcceptsHelmAborts,
        HelmFlipVerdict::TighteningMatchedHelmAbort,
        HelmFlipVerdict::LooseningMatchedKubernetesValidation,
        HelmFlipVerdict::LooseningWithUncertainKubernetes(vec!["missing schema".into()]),
        HelmFlipVerdict::Collapsed,
    ] {
        let observation = || Ok((verdict.clone(), scratch.path().to_owned()));
        let (actual, _) = combine_namespace_verdicts(observation(), observation())?;
        sim_assert_eq!(have: actual, want: verdict);
    }
    let first = HelmFlipVerdict::LooseningWithUncertainKubernetes(vec!["first diagnostic".into()]);
    let second =
        HelmFlipVerdict::LooseningWithUncertainKubernetes(vec!["second diagnostic".into()]);
    let (actual, _) = combine_namespace_verdicts(
        Ok((first.clone(), scratch.path().to_owned())),
        Ok((second, scratch.path().to_owned())),
    )?;
    sim_assert_eq!(have: actual, want: first);
    Ok(())
}

#[test]
fn different_namespace_verdicts_are_exact_undecided_acceptances() -> eyre::Result<()> {
    let scratch = observation_directory("namespace-split")?;
    for reverse in [false, true] {
        let rejected = Ok((
            HelmFlipVerdict::CandidateAcceptsHelmAborts,
            scratch.path().to_owned(),
        ));
        let accepted = Ok((
            HelmFlipVerdict::LooseningMatchedKubernetesValidation,
            scratch.path().to_owned(),
        ));
        let (first, second) = if reverse {
            (accepted, rejected)
        } else {
            (rejected, accepted)
        };
        let (verdict, _) = combine_namespace_verdicts(first, second)?;
        sim_assert_eq!(have: verdict.clone(), want: HelmFlipVerdict::EnvironmentDependentAcceptance(vec![NAMESPACE_UNCERTAINTY.into()]));
        let mut coverage = HelmAdjudicationCoverage::default();
        sim_assert_eq!(have: coverage.record_verdict(verdict, "chart: probe".into(), None), want: Some(true));
        sim_assert_eq!(have: coverage.flips_adjudicated, want: 1);
        assert!(coverage.false_acceptances.is_empty());
        sim_assert_eq!(have: coverage.tightenings_matched_helm_abort, want: 0);
        sim_assert_eq!(have: coverage.loosenings_with_uncertain_kubernetes.len(), want: 1);
        sim_assert_eq!(have: coverage.loosenings_with_uncertain_kubernetes[0].uncertain.clone(), want: vec![NAMESPACE_UNCERTAINTY.to_string()]);
    }
    Ok(())
}

#[test]
fn environment_dependent_tightenings_fail_and_infrastructure_is_never_uncertainty()
-> eyre::Result<()> {
    let scratch = observation_directory("namespace-rejection")?;
    let accepted = || {
        Err(eyre::Report::new(OracleFinding {
            kind: FailureKind::UnattributedRejection,
            reason: "oracle accepts".into(),
        }))
    };
    let rejected = || {
        Ok((
            HelmFlipVerdict::TighteningMatchedHelmAbort,
            scratch.path().to_owned(),
        ))
    };
    for reverse in [false, true] {
        let result = if reverse {
            combine_namespace_verdicts(accepted(), rejected())
        } else {
            combine_namespace_verdicts(rejected(), accepted())
        };
        let err = result
            .err()
            .ok_or_eyre("environment-dependent tightening counted as matched")?;
        sim_assert_eq!(have: FailureKind::of(&err), want: FailureKind::UnattributedRejection);
        assert!(err.to_string().contains(NAMESPACE_UNCERTAINTY));
    }
    for reverse in [false, true] {
        let failed = Err(eyre::eyre!("process failed"));
        let result = if reverse {
            combine_namespace_verdicts(accepted(), failed)
        } else {
            combine_namespace_verdicts(failed, accepted())
        };
        let err = result.err().ok_or_eyre("infrastructure failure counted")?;
        sim_assert_eq!(have: FailureKind::of(&err), want: FailureKind::Infrastructure);
    }
    Ok(())
}
