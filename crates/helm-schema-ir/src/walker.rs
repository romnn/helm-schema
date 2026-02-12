use regex::Regex;

use crate::Guard;

/// Extract `.Values.foo.bar` references → `["foo.bar"]`.
pub fn extract_values_paths(text: &str) -> Vec<String> {
    let re = Regex::new(r"\.Values\.([\w]+(?:\.[\w]+)*)").unwrap();
    let mut result: Vec<String> = re.captures_iter(text).map(|c| c[1].to_string()).collect();
    result.sort();
    result.dedup();
    result
}

/// Parse a Go template condition string into structured `Guard`(s).
///
/// Supports patterns like:
/// - `.Values.X`                       → `[Truthy("X")]`
/// - `not .Values.X`                   → `[Not("X")]`
/// - `or .Values.A .Values.B`          → `[Or(["A", "B"])]`
/// - `eq .Values.X "value"`            → `[Eq("X", "value")]`
/// - `and (.Values.A) (.Values.B)`     → `[Truthy("A"), Truthy("B")]`
///
/// Returns an empty vec if no `.Values.*` references are found.
pub fn parse_condition(text: &str) -> Vec<Guard> {
    let trimmed = text.trim();

    // `not .Values.X` → Guard::Not
    if let Some(rest) = trimmed
        .strip_prefix("not ")
        .or_else(|| trimmed.strip_prefix("not\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() == 1 {
            return vec![Guard::Not {
                path: paths.into_iter().next().unwrap(),
            }];
        }
    }

    // `or .Values.A .Values.B` → Guard::Or
    if let Some(rest) = trimmed
        .strip_prefix("or ")
        .or_else(|| trimmed.strip_prefix("or\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() >= 2 {
            return vec![Guard::Or { paths }];
        }
    }

    // `eq .Values.X "value"` → Guard::Eq
    if let Some(rest) = trimmed
        .strip_prefix("eq ")
        .or_else(|| trimmed.strip_prefix("eq\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() == 1 {
            let eq_re = Regex::new(r#""([^"]*)""#).unwrap();
            if let Some(caps) = eq_re.captures(rest) {
                return vec![Guard::Eq {
                    path: paths.into_iter().next().unwrap(),
                    value: caps[1].to_string(),
                }];
            }
        }
    }

    // `ne .Values.X "value"` → treat as a truthy guard on the referenced path
    if let Some(rest) = trimmed
        .strip_prefix("ne ")
        .or_else(|| trimmed.strip_prefix("ne\t"))
    {
        let paths = extract_values_paths(rest);
        if paths.len() == 1 {
            return vec![Guard::Truthy {
                path: paths.into_iter().next().unwrap(),
            }];
        }
    }

    // Default: simple truthy check(s)
    // `and (.Values.A) (.Values.B)` or bare multiple .Values refs
    // each become a separate Truthy guard.
    let paths = extract_values_paths(trimmed);
    paths
        .into_iter()
        .map(|p| Guard::Truthy { path: p })
        .collect()
}

/// True when the expression likely produces a YAML fragment rather than a single scalar.
pub fn is_fragment_expr(text: &str) -> bool {
    text.contains("toYaml")
        || text.contains("nindent")
        || text.contains("indent")
        || text.contains("tpl")
        || {
            (text.contains("include") || text.contains("template"))
                && (text.contains("nindent") || text.contains("toYaml"))
        }
}
