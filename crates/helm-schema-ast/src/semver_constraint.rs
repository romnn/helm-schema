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
/// Returns `None` for constraint shapes outside the supported subset: only a
/// single `<`/`<=`/`>`/`>=` comparator against a plain numeric bound is
/// encoded. Constraints carrying prerelease or build components, wildcards,
/// ranges, or comparator lists change Masterminds' matching rules (a bare
/// comparator never matches prerelease versions, while a `-0` suffix opts
/// them in), so they abstain rather than risk an inexact encoding. The
/// produced pattern likewise never matches prerelease versions.
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
    if version.contains(['-', '+']) {
        return None;
    }
    let parts: Vec<&str> = version.split('.').collect();
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
    let alternatives = core_alternatives(op, bound);
    if alternatives.is_empty() {
        return None;
    }
    Some(format!(
        "^v?(?:{})(?:\\+[0-9A-Za-z-]+(?:\\.[0-9A-Za-z-]+)*)?$",
        alternatives.join("|")
    ))
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
