//! Helm's `IsCompatibleRange`: Masterminds semver v3.5.0 constraint checks.
//!
//! Helm matches a declared dependency to a vendored chart only when the
//! chart's version satisfies the declared constraint. This ports the
//! constraint grammar the corpus uses (operators, `x`/`*` wildcards,
//! comma/space conjunctions, `||` alternatives, and `a - b` ranges); any
//! spelling outside it is refused as unmodelled instead of guessed.

use std::cmp::Ordering;

use super::ValuesError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: String,
}

impl Version {
    /// Masterminds `NewVersion` in its default coercing mode; `None` when
    /// the text is not a version.
    pub(super) fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, metadata) = match text.split_once('+') {
            Some((core, metadata)) => (core, Some(metadata)),
            None => (text, None),
        };
        let (numbers, pre) = match core.split_once('-') {
            Some((numbers, pre)) => (numbers, pre),
            None => (core, ""),
        };
        let mut segments = numbers.split('.');
        let major = parse_number(segments.next()?)?;
        let minor = match segments.next() {
            Some(segment) => parse_number(segment)?,
            None => 0,
        };
        let patch = match segments.next() {
            Some(segment) => parse_number(segment)?,
            None => 0,
        };
        if segments.next().is_some() {
            return None;
        }
        if core.contains('-') && !valid_identifiers(pre, true) {
            return None;
        }
        if let Some(metadata) = metadata
            && !valid_identifiers(metadata, false)
        {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
            pre: pre.to_string(),
        })
    }

    fn compare(&self, other: &Self) -> Ordering {
        let numbers =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if numbers != Ordering::Equal {
            return numbers;
        }
        match (self.pre.is_empty(), other.pre.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => compare_prerelease(&self.pre, &other.pre),
        }
    }
}

fn parse_number(segment: &str) -> Option<u64> {
    if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    segment.parse().ok()
}

/// Dot-separated non-empty `[0-9A-Za-z-]` identifiers; a prerelease's
/// numeric identifiers carry no leading zero.
fn valid_identifiers(text: &str, prerelease: bool) -> bool {
    text.split('.').all(|identifier| {
        let charset = !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        let numeric = identifier.bytes().all(|byte| byte.is_ascii_digit());
        charset && !(prerelease && numeric && identifier.len() > 1 && identifier.starts_with('0'))
    })
}

/// Masterminds `comparePrerelease`, including its handling of missing parts.
fn compare_prerelease(left: &str, right: &str) -> Ordering {
    let left: Vec<&str> = left.split('.').collect();
    let right: Vec<&str> = right.split('.').collect();
    for index in 0..left.len().max(right.len()) {
        let left = left.get(index).copied().unwrap_or_default();
        let right = right.get(index).copied().unwrap_or_default();
        let order = compare_prerelease_part(left, right);
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

fn compare_prerelease_part(left: &str, right: &str) -> Ordering {
    if left == right {
        return Ordering::Equal;
    }
    if left.is_empty() {
        return Ordering::Less;
    }
    if right.is_empty() {
        return Ordering::Greater;
    }
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => left.cmp(right),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    TildeOrEqual,
    NotEqual,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
    Tilde,
    Caret,
}

struct Constraint {
    operator: Operator,
    version: Version,
    dirty: bool,
    minor_dirty: bool,
    patch_dirty: bool,
}

/// Helm's `IsCompatibleRange(constraint, version)`.
pub(super) fn is_compatible_range(constraint: &str, version: &str) -> Result<bool, ValuesError> {
    let Some(version) = Version::parse(version) else {
        return Ok(false);
    };
    if constraint.trim().is_empty() {
        // Masterminds rejects an empty constraint, and Helm reads a
        // rejected constraint as incompatible.
        return Ok(false);
    }
    let unmodelled =
        || ValuesError::Unmodelled(format!("dependency version constraint {constraint:?}"));
    for group in constraint.split("||") {
        let constraints = parse_group(group).ok_or_else(unmodelled)?;
        let include_pre = constraints
            .iter()
            .any(|constraint| !constraint.version.pre.is_empty());
        if constraints
            .iter()
            .all(|constraint| check(&version, constraint, include_pre))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// One `,`/space-separated conjunction; `None` for any spelling outside
/// the ported grammar.
fn parse_group(group: &str) -> Option<Vec<Constraint>> {
    let tokens: Vec<&str> = group
        .split([',', ' ', '\t'])
        .filter(|token| !token.is_empty())
        .collect();
    let mut constraints = Vec::new();
    let mut index = 0;
    while let Some(token) = tokens.get(index) {
        // `a - b` is Masterminds' inclusive range.
        if tokens.get(index + 1) == Some(&"-") {
            let low = tokens.get(index)?;
            let high = tokens.get(index + 2)?;
            constraints.push(parse_constraint(&format!(">={low}"))?);
            constraints.push(parse_constraint(&format!("<={high}"))?);
            index += 3;
            continue;
        }
        let mut text = (*token).to_string();
        // An operator may stand apart from its version: `>= 1.2`.
        if OPERATORS.contains(&text.as_str()) {
            let version = tokens.get(index + 1)?;
            if version.starts_with(['=', '<', '>', '!', '~', '^']) {
                return None;
            }
            text.push_str(version);
            index += 1;
        }
        constraints.push(parse_constraint(&text)?);
        index += 1;
    }
    (!constraints.is_empty()).then_some(constraints)
}

const OPERATORS: [&str; 11] = ["!=", ">=", "=>", "<=", "=<", "~>", ">", "<", "=", "~", "^"];

fn parse_constraint(text: &str) -> Option<Constraint> {
    let (operator, rest) = OPERATORS
        .iter()
        .find_map(|spelling| text.strip_prefix(spelling).map(|rest| (*spelling, rest)))
        .unwrap_or(("", text));
    let operator = match operator {
        "" | "=" => Operator::TildeOrEqual,
        "!=" => Operator::NotEqual,
        ">" => Operator::Greater,
        "<" => Operator::Less,
        ">=" | "=>" => Operator::GreaterOrEqual,
        "<=" | "=<" => Operator::LessOrEqual,
        "~" | "~>" => Operator::Tilde,
        _ => Operator::Caret,
    };
    let rest = rest.strip_prefix('v').unwrap_or(rest);
    let (core, metadata) = match rest.split_once('+') {
        Some((core, metadata)) => (core, Some(metadata)),
        None => (rest, None),
    };
    let (numbers, pre) = match core.split_once('-') {
        Some((numbers, pre)) => (numbers, Some(pre)),
        None => (core, None),
    };
    let segments: Vec<&str> = numbers.split('.').collect();
    if segments.len() > 3
        || segments
            .iter()
            .any(|segment| !is_wildcard(segment) && parse_number(segment).is_none())
    {
        return None;
    }
    let first = segments.first().copied().unwrap_or_default();
    let second = segments.get(1).copied();
    let third = segments.get(2).copied();
    let pre_suffix = pre.map(|pre| format!("-{pre}")).unwrap_or_default();
    let (version, dirty, minor_dirty, patch_dirty) = if is_wildcard(first) {
        (format!("0.0.0{pre_suffix}"), true, false, false)
    } else if second.is_none_or(is_wildcard) {
        (format!("{first}.0.0{pre_suffix}"), true, true, false)
    } else if third.is_none_or(is_wildcard) {
        let second = second.unwrap_or_default();
        (format!("{first}.{second}.0{pre_suffix}"), true, false, true)
    } else {
        let version = match metadata {
            Some(metadata) => format!("{core}+{metadata}"),
            None => core.to_string(),
        };
        (version, false, false, false)
    };
    Some(Constraint {
        operator,
        version: Version::parse(&version)?,
        dirty,
        minor_dirty,
        patch_dirty,
    })
}

fn is_wildcard(segment: &str) -> bool {
    matches!(segment, "x" | "X" | "*")
}

fn check(version: &Version, constraint: &Constraint, include_pre: bool) -> bool {
    if !version.pre.is_empty() && !include_pre {
        return false;
    }
    let target = &constraint.version;
    match constraint.operator {
        Operator::TildeOrEqual => {
            if constraint.dirty {
                tilde(version, constraint)
            } else {
                version.compare(target) == Ordering::Equal
            }
        }
        Operator::NotEqual => not_equal(version, constraint),
        Operator::Greater => greater(version, constraint),
        Operator::Less => version.compare(target) == Ordering::Less,
        Operator::GreaterOrEqual => version.compare(target) != Ordering::Less,
        Operator::LessOrEqual => {
            if !constraint.dirty {
                return version.compare(target) != Ordering::Greater;
            }
            if version.major > target.major {
                return false;
            }
            !(version.major == target.major
                && version.minor > target.minor
                && !constraint.minor_dirty)
        }
        Operator::Tilde => tilde(version, constraint),
        Operator::Caret => caret(version, constraint),
    }
}

fn not_equal(version: &Version, constraint: &Constraint) -> bool {
    let target = &constraint.version;
    if constraint.dirty {
        if target.major != version.major {
            return true;
        }
        if target.minor != version.minor && !constraint.minor_dirty {
            return true;
        } else if constraint.minor_dirty {
            return false;
        } else if target.patch != version.patch && !constraint.patch_dirty {
            return true;
        } else if constraint.patch_dirty {
            if !version.pre.is_empty() || !target.pre.is_empty() {
                return compare_prerelease(&version.pre, &target.pre) != Ordering::Equal;
            }
            return false;
        }
    }
    version.compare(target) != Ordering::Equal
}

fn greater(version: &Version, constraint: &Constraint) -> bool {
    let target = &constraint.version;
    if !constraint.dirty {
        return version.compare(target) == Ordering::Greater;
    }
    if version.major > target.major {
        true
    } else if version.major < target.major || constraint.minor_dirty {
        false
    } else if constraint.patch_dirty {
        version.minor > target.minor
    } else {
        version.compare(target) == Ordering::Greater
    }
}

fn tilde(version: &Version, constraint: &Constraint) -> bool {
    let target = &constraint.version;
    if version.compare(target) == Ordering::Less {
        return false;
    }
    if target.major == 0
        && target.minor == 0
        && target.patch == 0
        && !constraint.minor_dirty
        && !constraint.patch_dirty
    {
        return true;
    }
    if version.major != target.major {
        return false;
    }
    version.minor == target.minor || constraint.minor_dirty
}

fn caret(version: &Version, constraint: &Constraint) -> bool {
    let target = &constraint.version;
    if version.compare(target) == Ordering::Less {
        return false;
    }
    if target.major > 0 || constraint.minor_dirty {
        return version.major == target.major;
    }
    if version.major > 0 {
        return false;
    }
    if target.minor > 0 || constraint.patch_dirty {
        return version.minor == target.minor;
    }
    if version.minor > 0 {
        return false;
    }
    target.patch == version.patch
}
