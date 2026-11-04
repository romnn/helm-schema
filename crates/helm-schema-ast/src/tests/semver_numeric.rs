use test_util::prelude::sim_assert_eq;

#[test]
fn unsupported_constraint_shapes_abstain() {
    let unsupported = [
        "",
        "*",
        "1.2.3",
        "=1.2.3",
        "!=1.2.3",
        "~1.2",
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
        ("junk", false),
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
