//! Where Helm's YAML 1.1 reading of a plain scalar differs from YAML 1.2.
//!
//! Helm decodes YAML with `sigs.k8s.io/yaml` (YAML 1.1 resolution, then
//! JSON); `serde_yaml` resolves with YAML 1.2. The two agree on every
//! document without a divergence listed here. Each was confirmed with Helm
//! v4.2.3: `yes`/`off`/`y` are booleans, `012` and `0o12` are 10, `1_000`
//! is 1000, `0x1f` and `0b101` are numbers, integers beyond 2^53 are
//! rounded to float64, integer and boolean keys are stringified, `<<`
//! merges, and `.inf`/`.nan` and null keys abort.

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
    /// A YAML 1.1 number spelling: a base prefix, a leading zero, or `_`.
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
    let digits = |text: &str| {
        !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'_')
    };
    let base_prefixed = ["0x", "0X", "0o", "0O", "0b", "0B"]
        .iter()
        .any(|prefix| unsigned.strip_prefix(prefix).is_some_and(digits));
    let decimal_like = unsigned.starts_with(|character: char| character.is_ascii_digit())
        && unsigned
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'_' || byte == b'.');
    let octal_like = unsigned.len() > 1
        && unsigned.starts_with('0')
        && unsigned.bytes().all(|byte| byte.is_ascii_digit());
    if base_prefixed || octal_like || (decimal_like && unsigned.contains('_')) {
        return Some(DialectDivergence::Number);
    }
    if !unsigned.is_empty()
        && unsigned.bytes().all(|byte| byte.is_ascii_digit())
        && unsigned
            .parse::<u64>()
            .map_or(true, |value| value > MAX_EXACT_INTEGER)
    {
        return Some(DialectDivergence::ImpreciseInteger);
    }
    if key && resolves_to_non_string(spelling) {
        return Some(DialectDivergence::NonStringKey);
    }
    None
}

/// Whether a plain scalar is a YAML 1.2 null, boolean, or number.
fn resolves_to_non_string(spelling: &str) -> bool {
    matches!(
        spelling,
        "" | "~"
            | "null"
            | "Null"
            | "NULL"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
    ) || (spelling.starts_with(|character: char| {
        character.is_ascii_digit() || matches!(character, '-' | '+' | '.')
    }) && spelling.parse::<f64>().is_ok())
}
