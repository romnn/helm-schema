//! Where Helm's YAML 1.1 reading of a plain scalar differs from YAML 1.2.
//!
//! Helm decodes YAML with `sigs.k8s.io/yaml` (go-yaml v2's YAML 1.1
//! resolution, then JSON); `serde_yaml` resolves with YAML 1.2. The two
//! agree on every document without a divergence listed here. Each was
//! confirmed with Helm v4.2.3 (`phase2/helm/{dialect,numeric}.log`):
//! - `yes`/`off`/`y` are booleans;
//! - `012` and `0o12` are 10, and `0x1f` and `0b101` are numbers;
//! - underscores anywhere in a number are dropped (`1_000` is 1000, `1_0e2`
//!   1000, `.1_0` 0.1);
//! - integers beyond 2^53 are rounded to float64;
//! - number and boolean keys are stringified;
//! - `<<` merges;
//! - `.inf`/`.nan` and null keys abort.
//!
//! Every number is a float64 in Helm, so `1e2` and `100.0` agree by value.

/// The plain scalars YAML 1.1 reads as booleans.
pub const YAML_1_1_BOOLEANS: [&str; 16] = [
    "y", "Y", "yes", "Yes", "YES", "n", "N", "no", "No", "NO", "on", "On", "ON", "off", "Off",
    "OFF",
];

/// The largest integer a float64 holds exactly.
const MAX_EXACT_INTEGER: u64 = 1 << 53;

/// Why Helm reads a plain scalar differently from a YAML 1.2 decoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialectDivergence {
    /// The merge key `<<`.
    MergeKey,
    /// A YAML 1.1 boolean such as `yes` or `off`.
    Boolean,
    /// `.inf` or `.nan`.
    SpecialFloat,
    /// A YAML 1.1 number spelling: a base prefix, a leading zero, or `_`
    /// anywhere.
    Number,
    /// An integer beyond float64 precision.
    ImpreciseInteger,
    /// A null, boolean or number as a mapping key, which Helm stringifies
    /// or refuses.
    NonStringKey,
}

/// How Helm reads the plain scalar `spelling` differently, if it does; `key`
/// says whether it is a mapping key.
#[must_use]
pub fn plain_scalar_divergence(spelling: &str, key: bool) -> Option<DialectDivergence> {
    if key && spelling == "<<" {
        return Some(DialectDivergence::MergeKey);
    }
    if YAML_1_1_BOOLEANS.contains(&spelling) {
        return Some(DialectDivergence::Boolean);
    }
    let unsigned = spelling.strip_prefix(['-', '+']).unwrap_or(spelling);
    let lowered = unsigned.to_ascii_lowercase();
    if matches!(lowered.as_str(), ".inf" | ".nan") {
        return Some(DialectDivergence::SpecialFloat);
    }
    if reads_as_number(spelling) {
        let digits: String = unsigned.chars().filter(|&c| c != '_').collect();
        let base_prefixed = ["0x", "0X", "0o", "0O", "0b", "0B"]
            .iter()
            .any(|prefix| digits.starts_with(prefix));
        let all_digits = digits.bytes().all(|byte| byte.is_ascii_digit());
        let leading_zero = all_digits && digits.len() > 1 && digits.starts_with('0');
        if spelling.contains('_') || base_prefixed || leading_zero {
            return Some(DialectDivergence::Number);
        }
        if all_digits
            && digits
                .parse::<u64>()
                .map_or(true, |value| value > MAX_EXACT_INTEGER)
        {
            return Some(DialectDivergence::ImpreciseInteger);
        }
        if key {
            return Some(DialectDivergence::NonStringKey);
        }
    }
    if key && NULL_AND_BOOLEAN_SPELLINGS.contains(&spelling) {
        return Some(DialectDivergence::NonStringKey);
    }
    None
}

/// The YAML 1.2 null and boolean spellings, which Helm also reads as null
/// or boolean.
const NULL_AND_BOOLEAN_SPELLINGS: [&str; 11] = [
    "", "~", "null", "Null", "NULL", "true", "True", "TRUE", "false", "False", "FALSE",
];

/// Whether Helm's decoder (go-yaml v2) resolves the plain scalar as a
/// number. It tries numbers only for a scalar starting with a digit, sign
/// or `.`, and removes every `_` first. What remains is a `0x`, `0o` or `0b`
/// integer, or YAML 1.1's
/// `[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`, which also
/// covers decimal integers. Out-of-range values stay strings in Helm but
/// count as numbers here, which only ever withholds a decoded value.
fn reads_as_number(spelling: &str) -> bool {
    if !spelling.starts_with(|c: char| c.is_ascii_digit() || matches!(c, '-' | '+' | '.')) {
        return false;
    }
    let plain: String = spelling.chars().filter(|&c| c != '_').collect();
    let unsigned = plain.strip_prefix(['-', '+']).unwrap_or(&plain);
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = unsigned.strip_prefix(prefix) {
            return !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix));
        }
    }
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, Some(exponent)),
        None => (unsigned, None),
    };
    let mantissa_is_number = match mantissa.split_once('.') {
        None => digits(mantissa),
        Some(("", fraction)) => digits(fraction),
        Some((integer, fraction)) => digits(integer) && (fraction.is_empty() || digits(fraction)),
    };
    let exponent_is_number = exponent
        .is_none_or(|exponent| digits(exponent.strip_prefix(['-', '+']).unwrap_or(exponent)));
    mantissa_is_number && exponent_is_number
}
