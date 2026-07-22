//! Semantic-version constraint lowering regressions.

use helm_schema_ast::semver_constraint_match_pattern;
use test_util::prelude::sim_assert_eq;

#[derive(Clone, Copy)]
enum Op {
    Lt,
    Le,
    Gt,
    Ge,
}

/// Masterminds `semverCompare` semantics for the encoded subset: one bare
/// comparator against a numeric bound. Versions parse as
/// `v?MAJOR(.MINOR(.PATCH)?)?` with numeric components (leading zeros
/// tolerated), build metadata is validated but ignored, and a bare
/// comparator never matches a prerelease version. Unparseable versions
/// error at render time, so they must not match either.
fn reference_matches(op: Op, bound: (u64, u64, u64), version: &str) -> bool {
    let version = version.strip_prefix('v').unwrap_or(version);
    let core = match version.split_once('+') {
        Some((core, build)) => {
            let valid_build = !build.is_empty()
                && build.split('.').all(|identifier| {
                    !identifier.is_empty()
                        && identifier
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                });
            if !valid_build {
                return false;
            }
            core
        }
        None => version,
    };
    if core.contains('-') {
        return false;
    }
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() > 3 {
        return false;
    }
    let mut value = [0u64; 3];
    for (position, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return false;
        }
        let Ok(parsed) = part.parse() else {
            return false;
        };
        if let Some(slot) = value.get_mut(position) {
            *slot = parsed;
        }
    }
    let [major, minor, patch] = value;
    let value = (major, minor, patch);
    match op {
        Op::Lt => value < bound,
        Op::Le => value <= bound,
        Op::Gt => value > bound,
        Op::Ge => value >= bound,
    }
}

fn candidate_versions() -> Vec<String> {
    let mut candidates: Vec<String> = [
        "",
        "banana",
        "1.x",
        "1..2",
        "1.2.3.4",
        "1.2.3-rc.1",
        "3.0.0-alpha",
        "v3.0.0-alpha",
        "1.2.3+build",
        "1.2.3+build.7",
        "1.2.3+!!",
        "1.2.3+",
        "1.2.3+a..b",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for major in [0u64, 1, 2, 3, 4, 10, 33] {
        for minor in [0u64, 1, 2, 8, 9, 10, 23, 33] {
            for patch in [0u64, 1, 3] {
                candidates.push(format!("{major}"));
                candidates.push(format!("{major}.{minor}"));
                candidates.push(format!("{major}.{minor}.{patch}"));
                candidates.push(format!("v{major}.{minor}.{patch}"));
                candidates.push(format!("0{major}.0{minor}.{patch}"));
                candidates.push(format!("{major}.{minor}.{patch}-rc1"));
                candidates.push(format!("{major}.{minor}.{patch}+m.1"));
            }
        }
    }
    candidates
}

#[test]
fn encoded_comparators_match_reference_semantics() {
    let cases: [(&str, Op, (u64, u64, u64)); 9] = [
        ("<3.0.0", Op::Lt, (3, 0, 0)),
        (">=3.0.0", Op::Ge, (3, 0, 0)),
        (">=1.8", Op::Ge, (1, 8, 0)),
        ("<=2.1.3", Op::Le, (2, 1, 3)),
        (">1.2", Op::Gt, (1, 2, 0)),
        (">= 1.23", Op::Ge, (1, 23, 0)),
        ("<v3.7.0", Op::Lt, (3, 7, 0)),
        (">=0.0.0", Op::Ge, (0, 0, 0)),
        ("<0.10.0", Op::Lt, (0, 10, 0)),
    ];
    for (constraint, op, bound) in cases {
        let pattern = semver_constraint_match_pattern(constraint)
            .unwrap_or_else(|| panic!("constraint {constraint:?} must encode"));
        let regex = regex::Regex::new(&pattern)
            .unwrap_or_else(|error| panic!("pattern {pattern:?} must compile: {error}"));
        for version in candidate_versions() {
            sim_assert_eq!(
                have: regex.is_match(&version),
                want: reference_matches(op, bound, &version),
                "constraint={constraint} version={version} pattern={pattern}"
            );
        }
    }
}

/// Constraint shapes beyond one bare numeric comparator (or the two
/// supported prerelease-floor idioms) change Masterminds' matching rules
/// (general prerelease bounds, ranges, wildcards) and must abstain instead
/// of guessing.
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
        assert!(
            semver_constraint_match_pattern(constraint).is_none(),
            "constraint {constraint:?} must abstain"
        );
    }
}

/// The prerelease-floor idioms (`>=X-0`, `<X-D`) match exactly the versions
/// Sprig's `semverCompare` accepts; every row below is differential-verified
/// against `helm template` renderings of the same call.
#[test]
fn prerelease_floor_constraints_match_the_semver_compare_language() {
    use test_util::prelude::sim_assert_eq;
    let cases: &[(&str, &[(&str, bool)])] = &[
        (
            ">=1.14.0-0",
            &[
                ("1.14.0", true),
                ("v1.14.0", true),
                ("1.14.1", true),
                ("2.0.0", true),
                ("1.14.0-alpha", true),
                ("1.15.0-rc.1", true),
                ("1.14", true),
                ("1.14-0", true),
                ("1.13.9", false),
                ("1.13.9-rc.1", false),
                ("0.9", false),
                ("junk", false),
            ],
        ),
        (
            "<9.9.9-9",
            &[
                ("1.29.0", true),
                ("v1.31.2", true),
                ("9.9.8", true),
                ("9.9.8-alpha", true),
                ("9.9.9-0", true),
                ("9.9.9-8.junk", true),
                ("0.1", true),
                ("9.9.9", false),
                ("9.9.9-9", false),
                ("9.9.9-10", false),
                ("9.9.9-alpha", false),
                ("10.0.0", false),
                ("9.9.9-9.0", false),
            ],
        ),
        (
            "< 1.35-0",
            &[
                ("1.29.0", true),
                ("1.34.99", true),
                ("1.34.0-rc.0", true),
                ("1.35.0", false),
                ("1.35.0-0", false),
                ("1.35.1", false),
                ("2.0.0", false),
            ],
        ),
        (
            ">= 1.23-0",
            &[("1.23.0", true), ("1.23.0-rc.1", true), ("1.22.9", false)],
        ),
    ];
    for (constraint, rows) in cases {
        let pattern = semver_constraint_match_pattern(constraint)
            .unwrap_or_else(|| panic!("constraint {constraint:?} must encode"));
        let regex = regex::Regex::new(&pattern)
            .unwrap_or_else(|error| panic!("pattern {pattern:?} must compile: {error}"));
        for (version, want) in *rows {
            sim_assert_eq!(
                have: regex.is_match(version),
                want: *want,
                "constraint={constraint} version={version} pattern={pattern}"
            );
        }
    }
}
