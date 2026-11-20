//! Concrete Sprig semantic-version comparison through one parsed numeric transfer.

use std::sync::LazyLock;

use helm_schema_core::ValuePattern;
use nom::Parser as _;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while1};
use nom::character::complete::{char, digit1, space0};
use nom::combinator::{all_consuming, map_res, opt, recognize, value};
use nom::multi::{many0, separated_list1};
use nom::sequence::{delimited, pair, preceded};
use semver::{Op, Version, VersionReq};

const MAX_VERSION_LEN: usize = 256;

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
    core: [u64; 3],
    written: usize,
    prerelease: Option<&'a str>,
    build: Option<&'a str>,
}

impl NumericVersion<'_> {
    /// Helm's fully written zero tilde (`~0.0.0`) accepts every stable version; a shorter zero
    /// bound keeps its written series.
    fn accepts_every_stable_version(&self) -> bool {
        self.written == 3 && self.core == [0; 3]
    }

    fn compare_core(&self, other: &Self, components: usize) -> std::cmp::Ordering {
        self.core
            .iter()
            .take(components)
            .cmp(other.core.iter().take(components))
    }
}

fn numeric_version(input: &str) -> nom::IResult<&str, NumericVersion<'_>> {
    if input.len() > MAX_VERSION_LEN {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::TooLarge,
        )));
    }
    let (input, (_, parts, prerelease, build)) = (
        opt(char('v')),
        separated_list1(char('.'), map_res(digit1, str::parse::<u64>)),
        opt(preceded(char('-'), semver_identifiers)),
        opt(preceded(char('+'), semver_identifiers)),
    )
        .parse(input)?;
    if parts.len() > 3
        || prerelease.is_some_and(|pre| {
            pre.split('.').any(|identifier| {
                identifier.len() > 1
                    && identifier.starts_with('0')
                    && identifier.bytes().all(|byte| byte.is_ascii_digit())
            })
        })
    {
        return Err(nom::Err::Error(nom::error::Error::new(
            input,
            nom::error::ErrorKind::Count,
        )));
    }
    let mut core = [0; 3];
    for (slot, &part) in core.iter_mut().zip(&parts) {
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
        if bound.build.is_some() {
            return None;
        }
        let supported = match &constraint {
            Self::Tilde { bound } => bound.prerelease.is_none(),
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
                        && bound.core == [0; 3]
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
                // Masterminds keeps the major, and the minor when it is written; an unwritten
                // patch is 0, so every patch of the series reaches the floor.
                version.prerelease.is_none()
                    && (bound.accepts_every_stable_version()
                        || major == bound_major
                            && (bound.written < 2 || minor == bound_minor && patch >= bound_patch))
            }
            Self::Compare { op, bound } => {
                // A missing suffix denotes a whole version series for > and <=.
                let components = match op {
                    ComparisonOp::Gt | ComparisonOp::Le => bound.written,
                    ComparisonOp::Eq | ComparisonOp::Ne | ComparisonOp::Lt | ComparisonOp::Ge => {
                        bound.core.len()
                    }
                };
                let order = version.compare_core(bound, components);
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
/// strings.
/// Unsupported constraints and invalid version spellings return `None`.
#[must_use]
pub fn semver_constraint_matches_version(constraint: &str, version: &str) -> Option<bool> {
    let (_, numeric) = all_consuming(numeric_version).parse(version).ok()?;
    if let Some(constraint) = NumericConstraint::parse(constraint) {
        return Some(constraint.matches(&numeric));
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
    // Masterminds includes prereleases on other cores in the tilde's series.
    if requirement
        .comparators
        .iter()
        .any(|comparator| comparator.op == Op::Tilde && !comparator.pre.is_empty())
    {
        return None;
    }
    let version = Version::parse(version.strip_prefix('v').unwrap_or(version)).ok()?;
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

/// A version parsed as Masterminds' `NewVersion` parses it, through the grammar `semverCompare`
/// uses: missing core components are 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemverParts {
    /// Major, minor and patch.
    pub core: [u64; 3],
    /// The prerelease identifiers, empty when absent.
    pub prerelease: String,
    /// The build metadata, empty when absent.
    pub metadata: String,
}

/// Parses a version spelling, returning `None` where Masterminds' `NewVersion` fails.
#[must_use]
pub fn parse_semver_version(text: &str) -> Option<SemverParts> {
    let (
        _,
        NumericVersion {
            core,
            written: _,
            prerelease,
            build,
        },
    ) = all_consuming(numeric_version).parse(text).ok()?;
    Some(SemverParts {
        core,
        prerelease: prerelease.unwrap_or_default().to_owned(),
        metadata: build.unwrap_or_default().to_owned(),
    })
}

/// The pattern of version spellings accepted by Masterminds' coercing parser.
#[must_use]
pub fn semver_version_pattern() -> &'static ValuePattern {
    static GRAMMAR: LazyLock<ValuePattern> = LazyLock::new(|| {
        let core = less_digits(u64::MAX)
            .into_iter()
            .chain([equal_digits(u64::MAX)])
            .collect::<Vec<_>>()
            .join("|");
        let prerelease = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)";
        ValuePattern::AllOf(vec![
            ValuePattern::String {
                pattern: format!(
                    r"^v?(?:{core})(?:\.(?:{core})){{0,2}}(?:-{prerelease}(?:\.{prerelease})*)?(?:\+{IDENTIFIERS})?$"
                ),
            },
            // The grammar is ASCII, so counting characters also counts bytes.
            ValuePattern::String {
                pattern: format!(r"^[0-9A-Za-z.+-]{{1,{MAX_VERSION_LEN}}}$"),
            },
        ])
    });
    &GRAMMAR
}

/// The language of valid version spellings that a supported constraint accepts.
///
/// It is the same [`NumericConstraint`] that decides concrete versions, encoded over the version
/// grammar: the pattern matches exactly where
/// [`semver_constraint_matches_version`] returns `Some(true)`. Unsupported constraints have no
/// pattern.
#[must_use]
pub fn semver_constraint_match_pattern(constraint: &str) -> Option<ValuePattern> {
    let alternatives = NumericConstraint::parse(constraint)?.pattern_alternatives();
    Some(ValuePattern::AllOf(vec![
        semver_version_pattern().clone(),
        ValuePattern::String {
            pattern: format!("^(?:{})$", alternatives.join("|")),
        },
    ]))
}

/// How one written version component relates to the bound's component.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Component {
    Any,
    Equal,
    Greater,
    Less,
}

const IDENTIFIERS: &str = r"[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*";

impl NumericConstraint<'_> {
    fn pattern_alternatives(&self) -> Vec<String> {
        let build = format!(r"(?:\+{IDENTIFIERS})?");
        let any_prerelease = format!(r"(?:-{IDENTIFIERS})?{build}");
        let (bound, mut cores) = match self {
            Self::Tilde { bound } => {
                let cores = if bound.accepts_every_stable_version() {
                    vec![[Component::Any; 3]]
                } else if bound.written == 1 {
                    vec![[Component::Equal, Component::Any, Component::Any]]
                } else {
                    [Component::Greater, Component::Equal]
                        .map(|patch| [Component::Equal, Component::Equal, patch])
                        .to_vec()
                };
                (
                    bound,
                    cores
                        .into_iter()
                        .map(|core| (core, build.clone()))
                        .collect::<Vec<_>>(),
                )
            }
            Self::Compare { op, bound } => {
                let components = match op {
                    ComparisonOp::Gt | ComparisonOp::Le => bound.written,
                    ComparisonOp::Eq | ComparisonOp::Ne | ComparisonOp::Lt | ComparisonOp::Ge => 3,
                };
                let ordered = |relation| lexicographic(relation, components);
                let mut cores = Vec::new();
                match (bound.prerelease, op) {
                    (Some(_), ComparisonOp::Ge) => {
                        for core in
                            [ordered(Component::Greater), ordered(Component::Equal)].concat()
                        {
                            cores.push((core, any_prerelease.clone()));
                        }
                    }
                    (Some(pre), _) => {
                        for core in ordered(Component::Less) {
                            cores.push((core, any_prerelease.clone()));
                        }
                        if let Some(top) = pre
                            .bytes()
                            .next()
                            .filter(|digit| *digit > b'0')
                            .map(|digit| digit - 1)
                        {
                            let first =
                                format!(r"-[0-{}](?:\.[0-9A-Za-z-]+)*{build}", char::from(top));
                            for core in ordered(Component::Equal) {
                                cores.push((core, first.clone()));
                            }
                        }
                    }
                    (None, _) => {
                        let relations: &[Component] = match op {
                            ComparisonOp::Eq => &[Component::Equal],
                            ComparisonOp::Ne => &[Component::Less, Component::Greater],
                            ComparisonOp::Lt => &[Component::Less],
                            ComparisonOp::Le => &[Component::Less, Component::Equal],
                            ComparisonOp::Gt => &[Component::Greater],
                            ComparisonOp::Ge => &[Component::Greater, Component::Equal],
                        };
                        for relation in relations {
                            for core in ordered(*relation) {
                                cores.push((core, build.clone()));
                            }
                        }
                    }
                }
                (bound, cores)
            }
        };
        let mut alternatives = Vec::new();
        for (core, suffix) in cores.drain(..) {
            for written in 1..=3 {
                if let Some(text) = written_core(core, written, bound) {
                    alternatives.push(format!("v?{text}{suffix}"));
                }
            }
        }
        alternatives.sort();
        alternatives.dedup();
        alternatives
    }
}

/// The component relations under which a core compares to the bound by `relation` over its first
/// `components` components; later components are unconstrained.
fn lexicographic(relation: Component, components: usize) -> Vec<[Component; 3]> {
    let mut cores = Vec::new();
    if relation == Component::Equal {
        let mut core = [Component::Any; 3];
        for slot in core.iter_mut().take(components) {
            *slot = Component::Equal;
        }
        return vec![core];
    }
    for differing in 0..components {
        let mut core = [Component::Any; 3];
        for (index, slot) in core.iter_mut().enumerate().take(differing + 1) {
            *slot = if index == differing {
                relation
            } else {
                Component::Equal
            };
        }
        cores.push(core);
    }
    cores
}

/// One version shape with `written` components; a missing component reads as 0.
fn written_core(
    core: [Component; 3],
    written: usize,
    bound: &NumericVersion<'_>,
) -> Option<String> {
    let mut parts = Vec::with_capacity(written);
    for (index, (relation, value)) in core.iter().zip(bound.core).enumerate() {
        if index >= written {
            let holds = match relation {
                Component::Any => true,
                Component::Equal => value == 0,
                Component::Greater => false,
                Component::Less => value > 0,
            };
            if !holds {
                return None;
            }
            continue;
        }
        parts.push(match relation {
            Component::Any => "[0-9]+".to_owned(),
            Component::Equal => equal_digits(value),
            Component::Greater => greater_digits(value),
            Component::Less => less_digits(value)?,
        });
    }
    Some(parts.join(r"\."))
}

/// Exactly `count` digits.
fn any_digits(count: usize) -> String {
    match count {
        0 => String::new(),
        1 => "[0-9]".to_owned(),
        _ => format!("[0-9]{{{count}}}"),
    }
}

/// Decimal spellings (leading zeros allowed) of exactly `value`.
fn equal_digits(value: u64) -> String {
    if value == 0 {
        "0+".to_owned()
    } else {
        format!("0*{value}")
    }
}

/// Decimal spellings (leading zeros allowed) of values above `value`.
fn greater_digits(value: u64) -> String {
    let digits = value.to_string();
    let width = digits.len();
    let mut alternatives = vec![format!("[1-9][0-9]{{{width},}}")];
    for (index, digit) in digits.bytes().enumerate() {
        if digit < b'9' {
            alternatives.push(format!(
                "{}[{}-9]{}",
                &digits[..index],
                char::from(digit + 1),
                any_digits(width - index - 1)
            ));
        }
    }
    format!("0*(?:{})", alternatives.join("|"))
}

/// Decimal spellings (leading zeros allowed) of values below `value`, if any.
fn less_digits(value: u64) -> Option<String> {
    if value == 0 {
        return None;
    }
    let digits = value.to_string();
    let width = digits.len();
    let mut alternatives = vec!["0".to_owned()];
    match width {
        0 | 1 => {}
        2 => alternatives.push("[1-9]".to_owned()),
        _ => alternatives.push(format!("[1-9][0-9]{{0,{}}}", width - 2)),
    }
    for (index, digit) in digits.bytes().enumerate() {
        let low = if index == 0 { b'1' } else { b'0' };
        if digit > low {
            alternatives.push(format!(
                "{}[{}-{}]{}",
                &digits[..index],
                char::from(low),
                char::from(digit - 1),
                any_digits(width - index - 1)
            ));
        }
    }
    Some(format!("0*(?:{})", alternatives.join("|")))
}

#[cfg(test)]
#[path = "tests/semver_numeric.rs"]
mod tests;
