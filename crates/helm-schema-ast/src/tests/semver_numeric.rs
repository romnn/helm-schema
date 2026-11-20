use color_eyre::eyre::{self, OptionExt as _};
use helm_schema_core::ValuePattern;
use test_util::prelude::sim_assert_eq;

fn constraint_regexes(constraint: &str) -> eyre::Result<regex::RegexSet> {
    let pattern = super::semver_constraint_match_pattern(constraint)
        .ok_or_eyre("supported constraint has a pattern")?;
    let mut pending = vec![&pattern];
    let mut languages = Vec::new();
    while let Some(pattern) = pending.pop() {
        match pattern {
            ValuePattern::String { pattern } => languages.push(pattern),
            ValuePattern::AllOf(parts) => pending.extend(parts),
            other => eyre::bail!("expected a conjunction of string patterns, got {other:?}"),
        }
    }
    Ok(regex::RegexSet::new(languages)?)
}

#[test]
fn unsupported_constraint_shapes_abstain() {
    let unsupported = [
        "",
        "*",
        "1.2.3",
        "=1.2.3",
        "!=1.2.3",
        "~1.2-0",
        "^1.2",
        ">1.33-0",
        "<=1.33-0",
        ">=1.33-1",
        ">=1.33-alpha",
        "<1.33-10",
        "<1.33-alpha",
        "<0.0.0-0",
        ">=1.2 <2.0",
        ">=1.2, <2",
        ">=1.2 || >=3",
        "<3.0.0.0",
        "<18446744073709551616.0.0",
        "<x.y.z",
        "<1.2.x",
        "<0.0.0",
    ];
    for constraint in unsupported {
        sim_assert_eq!(have: super::NumericConstraint::parse(constraint).is_none(), want: true, "{constraint}");
    }
}

/// Short exclusive bounds skip the entire omitted numeric suffix.
#[test]
fn short_greater_bound_excludes_its_written_series() {
    for (constraint, version, want) in [
        (">1.2", "1.2.1", false),
        (">1.2", "1.3.0", true),
        (">1", "1.99.99", false),
        (">1", "2.0.0", true),
        (">1.2.0", "1.2.1", true),
    ] {
        sim_assert_eq!(
            have: super::semver_constraint_matches_version(constraint, version),
            want: Some(want),
            "constraint={constraint} version={version}"
        );
    }
}

/// A short inclusive ceiling includes every patch in its written series.
#[test]
fn short_inclusive_bound_keeps_its_written_series() {
    for (constraint, version, want) in [
        ("<=1.2", "1.2.1", true),
        ("<=1.2", "1.3.0", false),
        ("<=1", "1.99.99", true),
        ("<=1", "2.0.0", false),
        ("<=1.2.0", "1.2.1", false),
    ] {
        sim_assert_eq!(
            have: super::semver_constraint_matches_version(constraint, version),
            want: Some(want),
            "constraint={constraint} version={version}"
        );
    }
}

/// Helm's fully written zero tilde is the stable-version universal range.
#[test]
fn zero_tilde_accepts_every_stable_version() {
    for (version, want) in [
        ("0.0.0", true),
        ("0.0.1", true),
        ("0.1.0", true),
        ("1.0.0", true),
        ("v99.8.7+build.1", true),
        ("1.0.0-alpha", false),
    ] {
        sim_assert_eq!(
            have: super::semver_constraint_matches_version("~0.0.0", version),
            want: Some(want),
            "version={version}"
        );
    }
}

/// A component beyond Helm's numeric domain cannot prove either comparison outcome.
#[test]
fn numeric_versions_outside_helm_component_range_abstain() {
    for (constraint, version, want) in [
        (">1.2", "1.2.18446744073709551616", None),
        ("<=1.2", "1.2.18446744073709551616", None),
        ("~0.0.0", "18446744073709551616.0.0", None),
        (">=1.0.0", "18446744073709551616.0.0", None),
        (">1.2", "1.2.18446744073709551615", Some(false)),
        ("<=1.2", "1.2.18446744073709551615", Some(true)),
        ("~0.0.0", "18446744073709551615.0.0", Some(true)),
    ] {
        sim_assert_eq!(
            have: super::semver_constraint_matches_version(constraint, version),
            want: want,
            "constraint={constraint} version={version}"
        );
    }
}

/// Helm 4.2.3 verdicts (TRANSFER1 `transfer/probes/semver-rework-helm-4.2.3.txt`), decided by the
/// concrete transfer and by the symbolic pattern alike.
#[test]
fn constraint_language_follows_helm_partial_bounds_and_floors() -> eyre::Result<()> {
    let verdicts = [
        (">1.2", "1.2.1", false),
        ("<=1.2", "1.2.1", true),
        ("<=1", "1.5.0", true),
        ("~0.0.0", "5.1.0", true),
        (">1.2", "1.3.0", true),
        (">1.2", "1.2.0", false),
        ("<=1.2", "1.3.0", false),
        ("<1.2", "1.1.9", true),
        ("<1.2", "1.2.0", false),
        (">=1.2", "1.2.0-rc.1", false),
        (">=1.14.0-0", "1.14.0-rc.1", true),
        ("<1.19-0", "1.18.9", true),
        ("<1.19-0", "1.19.0-1", false),
        ("<1.19-0", "1.19.0", false),
        ("<1.19-0", "v1.18.0-beta.2", true),
        ("~3.0.0", "3.0.9", true),
        ("~3.0.0", "3.1.0", false),
        ("~3.0.0", "3.0.0-rc.1", false),
        (">=1.14.0-0", "v1.29.0", true),
        ("<=1.2", "1.2.0+build", true),
        (">01.2", "1.3", true),
        (">1", "2", true),
        ("<=0.0.0", "0.0.0", true),
        ("<1.2.3", "01.002.002", true),
        (">=1.2.3", "1.2.3-alpha", false),
    ];
    for (constraint, version, helm) in verdicts {
        sim_assert_eq!(
            have: super::semver_constraint_matches_version(constraint, version),
            want: Some(helm),
            "{constraint} vs {version}"
        );
        let language = constraint_regexes(constraint)?;
        sim_assert_eq!(
            have: language.matches(version).iter().count() == language.len(),
            want: helm,
            "{constraint} vs {version}"
        );
    }
    Ok(())
}

/// The symbolic pattern and the concrete transfer are one constraint: on every grammatical
/// version they decide alike.
#[test]
fn constraint_pattern_agrees_with_the_concrete_transfer() -> eyre::Result<()> {
    let constraints = [
        ">1.2",
        "<=1.2",
        "<=1",
        ">=1.2.3",
        "<1.2.3",
        ">1.2.3",
        "<=1.2.3",
        ">=1.2",
        "<1.2",
        ">1",
        "<1",
        ">=0",
        "<10.20",
        ">=1.14.0-0",
        ">=1.33-0",
        "<1.19-0",
        "<1.19-5",
        "~3.0.0",
        "~0.0.0",
        "~1.20.9",
        "~1.2",
        "~v1.2",
        "~1",
        "~0",
        "~0.0",
        "~3.0",
        ">01.2",
        "<0.0.1",
        ">=99.0.0",
    ];
    let mut versions = Vec::new();
    for prefix in ["", "v"] {
        for core in [
            "0",
            "1",
            "2",
            "9",
            "10",
            "01",
            "1.2",
            "1.3",
            "1.1",
            "1.20",
            "1.19",
            "1.18",
            "0.0",
            "1.2.0",
            "1.2.1",
            "1.2.3",
            "1.2.4",
            "1.2.2",
            "3.0.0",
            "3.0.9",
            "3.1.0",
            "2.9.9",
            "1.14.0",
            "1.13.99",
            "1.19.0",
            "1.18.9",
            "1.20.9",
            "1.20.10",
            "1.21.0",
            "0.0.0",
            "0.0.1",
            "99.0.0",
            "100.0.0",
            "001.002.003",
            "1.002",
            "10.20",
            "10.19.99",
            "10.21",
        ] {
            for suffix in [
                "", "-0", "-1", "-4", "-rc.1", "-alpha", "+build", "-0+b", "-5.x",
            ] {
                versions.push(format!("{prefix}{core}{suffix}"));
            }
        }
    }
    for constraint in constraints {
        let language = constraint_regexes(constraint)?;
        for version in &versions {
            let Some(concrete) = super::semver_constraint_matches_version(constraint, version)
            else {
                continue;
            };
            sim_assert_eq!(
                have: language.matches(version).iter().count() == language.len(),
                want: concrete,
                "{constraint} vs {version}"
            );
        }
    }
    Ok(())
}

#[test]
fn constraint_pattern_excludes_invalid_helm_versions() -> eyre::Result<()> {
    let pattern = super::semver_constraint_match_pattern(">=1.2-0")
        .ok_or_eyre("supported constraint has a pattern")?;
    for (version, want) in [
        (format!("1.2.3+{}", "b".repeat(250)), true),
        (format!("1.2.3+{}", "b".repeat(251)), false),
        ("1.2.3-01".to_owned(), false),
        ("18446744073709551616.0.0".to_owned(), false),
        ("18446744073709551615.0.0".to_owned(), true),
    ] {
        let operand = serde_json::json!(version);
        sim_assert_eq!(have: pattern.matches(&operand), want: Some(want), "{version}");
        sim_assert_eq!(
            have: super::semver_constraint_matches_version(">=1.2-0", &version),
            want: want.then_some(true),
            "{version}"
        );
    }
    Ok(())
}
