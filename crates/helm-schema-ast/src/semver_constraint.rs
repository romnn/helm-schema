//! Exact lexical encodings of Sprig `semverCompare` comparator constraints.
//!
//! A bounded comparator constraint (`<3.0.0`, `>=1.8`) partitions version
//! strings into "guard holds" and "guard does not hold". Both sides are
//! regular: a version is `v?MAJOR(.MINOR(.PATCH)?)?` with numeric components
//! (leading zeros tolerated by Masterminds' loose parser), missing components
//! read as zero, and build metadata ignored, so a numeric comparison against
//! a literal bound lowers to a finite alternation over digit prefixes. The
//! encoding here is exact — it matches precisely the versions the comparator
//! accepts — which lets condition lowering treat the derived pattern as a
//! structural guard rather than a heuristic.

/// The comparison operator of a single bounded comparator constraint.
#[derive(Clone, Copy)]
enum ComparisonOp {
    Lt,
    Le,
    Gt,
    Ge,
}

/// Lowers a `semverCompare` constraint to a regex matching exactly the
/// version strings that satisfy it.
///
/// Returns `None` for constraint shapes outside the supported subset: a
/// single `<`/`<=`/`>`/`>=` comparator against a plain numeric bound, plus
/// the two prerelease-FLOOR idioms charts use to opt prereleases in:
/// `>=X-0` (every version whose core satisfies `>= X`, prereleases
/// included — no prerelease identifier sorts below `0`) and `<X-D` with a
/// single-digit prerelease `D` (every version whose core satisfies `< X`,
/// prereleases included, plus the prereleases of `X` itself whose first
/// identifier is a numeric below `D` — longer numerics and alphanumerics
/// sort above any single digit). Other prerelease bounds, wildcards,
/// ranges, and comparator lists change Masterminds' matching rules and
/// abstain rather than risk an inexact encoding. Without a prerelease
/// suffix the produced pattern never matches prerelease versions,
/// mirroring Masterminds' bare-comparator rule.
#[must_use]
pub fn semver_constraint_match_pattern(constraint: &str) -> Option<String> {
    let text = constraint.trim();
    let (op, rest) = if let Some(rest) = text.strip_prefix(">=") {
        (ComparisonOp::Ge, rest)
    } else if let Some(rest) = text.strip_prefix("<=") {
        (ComparisonOp::Le, rest)
    } else if let Some(rest) = text.strip_prefix('>') {
        (ComparisonOp::Gt, rest)
    } else {
        (ComparisonOp::Lt, text.strip_prefix('<')?)
    };
    let version = rest.trim();
    let version = version.strip_prefix('v').unwrap_or(version);
    if version.contains('+') {
        return None;
    }
    let (core, prerelease) = match version.split_once('-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (version, None),
    };
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut bound = [0u64; 3];
    for (position, part) in parts.iter().enumerate() {
        if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        bound[position] = part.parse().ok()?;
    }
    let body = match prerelease {
        None => {
            let alternatives = core_alternatives(op, bound);
            if alternatives.is_empty() {
                return None;
            }
            alternatives.join("|")
        }
        Some(prerelease) => prerelease_floor_body(op, bound, prerelease)?,
    };
    Some(format!(
        "^v?(?:{body})(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?$"
    ))
}

/// One semver prerelease tail (`-alpha.1`): a dash followed by dot-separated
/// alphanumeric identifiers.
const PRERELEASE_TAIL: &str = "-[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*";

/// The alternation body for the prerelease-floor comparators (see
/// [`semver_constraint_match_pattern`]).
fn prerelease_floor_body(op: ComparisonOp, bound: [u64; 3], prerelease: &str) -> Option<String> {
    match op {
        // `>= X-0`: the core decides alone — every prerelease of a
        // satisfying core (X itself included) sorts at or above `X-0`.
        ComparisonOp::Ge if prerelease == "0" => {
            let alternatives = core_alternatives(ComparisonOp::Ge, bound);
            if alternatives.is_empty() {
                return None;
            }
            Some(format!(
                "(?:{})(?:{PRERELEASE_TAIL})?",
                alternatives.join("|")
            ))
        }
        // `< X-D` (single digit D): cores below X with any prerelease,
        // plus X's own prereleases whose FIRST identifier is a numeric
        // below D. A multi-digit numeric is at least ten and an
        // alphanumeric identifier sorts above every numeric, so "one
        // digit in [0, D)" is the exact first-identifier language; any
        // continuation after a decided first identifier stays below.
        ComparisonOp::Lt
            if prerelease.len() == 1 && prerelease.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let alternatives = core_alternatives(ComparisonOp::Lt, bound);
            if alternatives.is_empty() && prerelease == "0" {
                return None;
            }
            let mut body = if alternatives.is_empty() {
                Vec::new()
            } else {
                vec![format!(
                    "(?:{})(?:{PRERELEASE_TAIL})?",
                    alternatives.join("|")
                )]
            };
            if let Some(limit) = prerelease.parse::<u32>().ok().filter(|limit| *limit > 0) {
                for core in equal_cores(bound) {
                    body.push(format!(
                        "{core}-{}(?:\\.[0-9A-Za-z-]+)*",
                        digit_span(0, limit - 1)
                    ));
                }
            }
            if body.is_empty() {
                return None;
            }
            Some(body.join("|"))
        }
        _ => None,
    }
}

/// The written core forms whose numeric value equals `bound` (missing
/// components read as zero).
fn equal_cores(bound: [u64; 3]) -> Vec<String> {
    let mut cores = Vec::new();
    for written in 1..=3usize {
        if bound[written..].iter().all(|component| *component == 0) {
            let components: Vec<String> = bound[..written]
                .iter()
                .copied()
                .map(decimal_eq_pattern)
                .collect();
            cores.push(components.join("\\."));
        }
    }
    cores
}

/// Version-core alternations satisfying `value OP bound`.
///
/// A core writes one to three components; missing components read as zero.
/// The comparison decomposes into "the first differing component decides"
/// cases plus, for inclusive operators, full equality. A case constraining a
/// component the core does not write survives only when zero already
/// satisfies it, which makes those cases statically decidable here.
fn core_alternatives(op: ComparisonOp, bound: [u64; 3]) -> Vec<String> {
    let decided_less = matches!(op, ComparisonOp::Lt | ComparisonOp::Le);
    let inclusive = matches!(op, ComparisonOp::Le | ComparisonOp::Ge);
    let mut alternatives = Vec::new();
    for written in 1..=3usize {
        for decisive in 0..3usize {
            if let Some(core) = first_difference_core(bound, written, decisive, decided_less) {
                alternatives.push(core);
            }
        }
        if inclusive && bound[written..].iter().all(|component| *component == 0) {
            let components: Vec<String> = bound[..written]
                .iter()
                .copied()
                .map(decimal_eq_pattern)
                .collect();
            alternatives.push(components.join("\\."));
        }
    }
    alternatives
}

/// The core alternation where components before `decisive` equal the bound
/// and the `decisive` component decides the comparison, or `None` when the
/// case is impossible (an unwritten component is zero and cannot exceed the
/// bound, and nothing is below zero).
fn first_difference_core(
    bound: [u64; 3],
    written: usize,
    decisive: usize,
    decided_less: bool,
) -> Option<String> {
    // Unwritten components read as zero: equality demands a zero bound, a
    // decisive "less" demands a positive bound, and a decisive "greater" is
    // impossible.
    for (position, component) in bound.iter().enumerate().skip(written) {
        match position.cmp(&decisive) {
            std::cmp::Ordering::Less if *component != 0 => return None,
            std::cmp::Ordering::Equal if !(decided_less && *component > 0) => return None,
            _ => {}
        }
    }
    let mut components = Vec::new();
    for (position, component) in bound.iter().enumerate().take(written) {
        let pattern = match position.cmp(&decisive) {
            std::cmp::Ordering::Less => decimal_eq_pattern(*component),
            std::cmp::Ordering::Equal if decided_less => decimal_lt_pattern(*component)?,
            std::cmp::Ordering::Equal => decimal_gt_pattern(*component),
            std::cmp::Ordering::Greater => "[0-9]+".to_string(),
        };
        components.push(pattern);
    }
    Some(components.join("\\."))
}

/// A single digit, or a character class when the endpoints differ.
fn digit_span(low: u32, high: u32) -> String {
    if low == high {
        low.to_string()
    } else {
        format!("[{low}-{high}]")
    }
}

/// Decimal strings (leading zeros tolerated) whose numeric value equals
/// `bound`.
fn decimal_eq_pattern(bound: u64) -> String {
    format!("0*{bound}")
}

/// Decimal strings (leading zeros tolerated) whose numeric value is below
/// `bound`; `None` when nothing is (the bound is zero).
fn decimal_lt_pattern(bound: u64) -> Option<String> {
    if bound == 0 {
        return None;
    }
    let digits = bound_digits(bound);
    let count = digits.len();
    let mut alternatives = Vec::new();
    if count == 1 {
        alternatives.push(digit_span(0, digits[0] - 1));
    } else {
        // Fewer significant digits than the bound is always below it; equal
        // length splits on the first digit that drops below the bound's.
        alternatives.push("[0-9]".to_string());
        for length in 2..count {
            alternatives.push(format!("[1-9][0-9]{{{}}}", length - 1));
        }
        for split in 0..count {
            let low = u32::from(split == 0);
            if digits[split] <= low {
                continue;
            }
            let mut pattern: String = digits[..split].iter().map(ToString::to_string).collect();
            pattern.push_str(&digit_span(low, digits[split] - 1));
            let remaining = count - split - 1;
            if remaining > 0 {
                pattern.push_str(&format!("[0-9]{{{remaining}}}"));
            }
            alternatives.push(pattern);
        }
    }
    Some(format!("0*(?:{})", alternatives.join("|")))
}

/// Decimal strings (leading zeros tolerated) whose numeric value is above
/// `bound`.
fn decimal_gt_pattern(bound: u64) -> String {
    let digits = bound_digits(bound);
    let count = digits.len();
    // More significant digits than the bound is always above it; equal
    // length splits on the first digit that rises above the bound's.
    let mut alternatives = vec![format!("[1-9][0-9]{{{count},}}")];
    for split in 0..count {
        let low = digits[split].saturating_add(1).max(u32::from(split == 0));
        if low > 9 {
            continue;
        }
        let mut pattern: String = digits[..split].iter().map(ToString::to_string).collect();
        pattern.push_str(&digit_span(low, 9));
        let remaining = count - split - 1;
        if remaining > 0 {
            pattern.push_str(&format!("[0-9]{{{remaining}}}"));
        }
        alternatives.push(pattern);
    }
    format!("0*(?:{})", alternatives.join("|"))
}

fn bound_digits(bound: u64) -> Vec<u32> {
    bound
        .to_string()
        .chars()
        .filter_map(|digit| digit.to_digit(10))
        .collect()
}
