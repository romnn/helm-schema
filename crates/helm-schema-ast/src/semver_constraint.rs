//! Concrete Sprig semantic-version comparison through one parsed numeric transfer.

use nom::Parser as _;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while1};
use nom::character::complete::{char, digit1, space0};
use nom::combinator::{all_consuming, map_res, opt, recognize, value};
use nom::multi::{many0, separated_list1};
use nom::sequence::{delimited, pair, preceded};
use semver::{Version, VersionReq};

/// The comparison operator of a single bounded comparator constraint.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ComparisonOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone)]
struct NumericVersion<'a> {
    core: [&'a str; 3],
    written: usize,
    prerelease: Option<&'a str>,
    build: Option<&'a str>,
}

impl NumericVersion<'_> {
    fn compare_core(&self, other: &Self) -> std::cmp::Ordering {
        self.core
            .iter()
            .zip(other.core)
            .map(|(left, right)| decimal_order(left, right))
            .find(|order| !order.is_eq())
            .unwrap_or(std::cmp::Ordering::Equal)
    }
}

fn decimal_order(left: &str, right: &str) -> std::cmp::Ordering {
    let left = left.trim_start_matches('0');
    let right = right.trim_start_matches('0');
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn numeric_version(input: &str) -> nom::IResult<&str, NumericVersion<'_>> {
    let (input, (_, parts, prerelease, build)) = (
        opt(char('v')),
        separated_list1(char('.'), digit1),
        opt(preceded(char('-'), semver_identifiers)),
        opt(preceded(char('+'), semver_identifiers)),
    )
        .parse(input)?;
    if parts.len() > 3 {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Count,
        )));
    }
    let mut core = ["0"; 3];
    for (slot, part) in core.iter_mut().zip(&parts) {
        *slot = part;
    }
    Ok((
        input,
        NumericVersion {
            core,
            written: parts.len(),
            prerelease,
            build,
        },
    ))
}

enum NumericConstraint<'a> {
    Compare {
        op: ComparisonOp,
        bound: NumericVersion<'a>,
    },
    Tilde {
        bound: NumericVersion<'a>,
    },
}

impl<'a> NumericConstraint<'a> {
    fn parse(input: &'a str) -> Option<Self> {
        let operator = alt((
            value(Some(ComparisonOp::Ge), tag(">=")),
            value(Some(ComparisonOp::Le), tag("<=")),
            value(Some(ComparisonOp::Gt), tag(">")),
            value(Some(ComparisonOp::Lt), tag("<")),
            value(None, tag("~")),
        ));
        let (_, (op, bound)) = all_consuming((operator, preceded(space0, numeric_version)))
            .parse(input.trim())
            .ok()?;
        let constraint = match op {
            Some(op) => Self::Compare { op, bound },
            None => Self::Tilde { bound },
        };
        let bound = match &constraint {
            Self::Compare { op: _, bound } | Self::Tilde { bound } => bound,
        };
        if bound.build.is_some() || bound.core.iter().any(|part| part.parse::<u64>().is_err()) {
            return None;
        }
        let supported = match &constraint {
            Self::Tilde { bound } => bound.written == 3 && bound.prerelease.is_none(),
            Self::Compare { op, bound } => {
                let floor = match bound.prerelease {
                    None => true,
                    Some("0") if *op == ComparisonOp::Ge => true,
                    Some(pre) if *op == ComparisonOp::Lt => {
                        pre.len() == 1 && pre.bytes().all(|byte| byte.is_ascii_digit())
                    }
                    Some(_) => false,
                };
                floor
                    && !(*op == ComparisonOp::Lt
                        && bound
                            .core
                            .iter()
                            .all(|part| part.bytes().all(|byte| byte == b'0'))
                        && bound.prerelease.is_none_or(|pre| pre == "0"))
            }
        };
        supported.then_some(constraint)
    }

    fn matches(&self, version: &NumericVersion<'_>) -> bool {
        match self {
            Self::Tilde { bound } => {
                let [major, minor, patch] = version.core;
                let [bound_major, bound_minor, bound_patch] = bound.core;
                version.prerelease.is_none()
                    && decimal_order(major, bound_major).is_eq()
                    && decimal_order(minor, bound_minor).is_eq()
                    && !decimal_order(patch, bound_patch).is_lt()
            }
            Self::Compare { op, bound } => {
                let order = version.compare_core(bound);
                if let Some(pre) = bound.prerelease {
                    if *op == ComparisonOp::Ge {
                        return !order.is_lt();
                    }
                    let first = version.prerelease.and_then(|pre| pre.split('.').next());
                    return order.is_lt()
                        || order.is_eq()
                            && first.is_some_and(|first| {
                                first.len() == 1
                                    && first.bytes().all(|byte| byte.is_ascii_digit())
                                    && first < pre
                            });
                }
                version.prerelease.is_none()
                    && match op {
                        ComparisonOp::Eq => order.is_eq(),
                        ComparisonOp::Ne => !order.is_eq(),
                        ComparisonOp::Lt => order.is_lt(),
                        ComparisonOp::Le => !order.is_gt(),
                        ComparisonOp::Gt => order.is_gt(),
                        ComparisonOp::Ge => !order.is_lt(),
                    }
            }
        }
    }
}

/// Evaluates a supported Helm semantic-version constraint against a concrete
/// version.
///
/// A fixed analysis-policy version needs no regex language for possible
/// values-file overrides. Parsing the complete range and version directly
/// lets the control-flow evaluator decide forms such as `^1.6-0` without
/// pretending that the same constraint has been encoded for arbitrary user
/// strings. Unsupported constraint syntax returns `None`.
#[must_use]
pub fn semver_constraint_matches_version(constraint: &str, version: &str) -> Option<bool> {
    if let Some(constraint) = NumericConstraint::parse(constraint) {
        return Some(
            all_consuming(numeric_version)
                .parse(version)
                .is_ok_and(|(_, version)| constraint.matches(&version)),
        );
    }
    if let Ok((_, (op, bound))) =
        all_consuming(delimited(space0, concrete_comparator, space0)).parse(constraint)
        && !bound.pre.is_empty()
    {
        let version = Version::parse(version.strip_prefix('v').unwrap_or(version)).ok()?;
        // Masterminds treats overflowing numeric prerelease identifiers as
        // text, while Rust's semver crate keeps arbitrary-precision ordering.
        if [&bound, &version].into_iter().any(|value| {
            value.pre.as_str().split('.').any(|identifier| {
                !identifier.is_empty()
                    && identifier.bytes().all(|byte| byte.is_ascii_digit())
                    && identifier.parse::<u64>().is_err()
            })
        }) {
            return None;
        }
        let order = version.cmp_precedence(&bound);
        // A prerelease comparator opts every version core into Helm's ordering.
        // Cargo requirements instead exclude prereleases on other cores.
        return Some(match op {
            ComparisonOp::Eq => order.is_eq(),
            ComparisonOp::Ne => !order.is_eq(),
            ComparisonOp::Lt => order.is_lt(),
            ComparisonOp::Le => !order.is_gt(),
            ComparisonOp::Gt => order.is_gt(),
            ComparisonOp::Ge => !order.is_lt(),
        });
    }
    let requirement = VersionReq::parse(constraint).ok().or_else(|| {
        let (_, normalized) = all_consuming(delimited(space0, masterminds_loose_caret, space0))
            .parse(constraint)
            .ok()?;
        VersionReq::parse(&normalized).ok()
    })?;
    let version = Version::parse(version.strip_prefix(['v', 'V']).unwrap_or(version)).ok()?;
    Some(requirement.matches(&version))
}

fn concrete_comparator(input: &str) -> nom::IResult<&str, (ComparisonOp, Version)> {
    pair(
        alt((
            value(ComparisonOp::Ge, tag(">=")),
            value(ComparisonOp::Le, tag("<=")),
            value(ComparisonOp::Ne, tag("!=")),
            value(ComparisonOp::Gt, tag(">")),
            value(ComparisonOp::Lt, tag("<")),
            value(ComparisonOp::Eq, tag("=")),
        )),
        preceded(
            space0,
            map_res(
                preceded(
                    opt(char('v')),
                    take_while1(|character: char| !character.is_whitespace()),
                ),
                Version::parse,
            ),
        ),
    )
    .parse(input)
}

fn masterminds_loose_caret(input: &str) -> nom::IResult<&str, String> {
    let (input, _) = char('^')(input)?;
    let (input, _) = space0(input)?;
    let (input, _) = opt(tag("v")).parse(input)?;
    let (input, major) = digit1(input)?;
    let (input, minor) = opt(preceded(char('.'), digit1)).parse(input)?;
    let (input, _) = char('-')(input)?;
    let (input, prerelease) = semver_identifiers(input)?;
    let (input, build) = opt(preceded(char('+'), semver_identifiers)).parse(input)?;

    let minor = minor.unwrap_or("0");
    let build = build.map_or_else(String::new, |value| format!("+{value}"));
    Ok((input, format!("^{major}.{minor}.0-{prerelease}{build}")))
}

fn semver_identifiers(input: &str) -> nom::IResult<&str, &str> {
    recognize(pair(
        semver_identifier,
        many0(preceded(char('.'), semver_identifier)),
    ))
    .parse(input)
}

fn semver_identifier(input: &str) -> nom::IResult<&str, &str> {
    take_while1(|character: char| character.is_ascii_alphanumeric() || character == '-')(input)
}

#[cfg(test)]
#[path = "tests/semver_numeric.rs"]
mod tests;
