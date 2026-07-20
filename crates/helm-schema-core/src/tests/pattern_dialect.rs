use test_util::prelude::sim_assert_eq;

use super::{ecma_case_folded_pattern, normalize_schema_pattern_dialects};

#[test]
fn leading_case_insensitive_group_folds_exactly() {
    sim_assert_eq!(
        have: ecma_case_folded_pattern("^(?i)(abort|warn)?$"),
        want: Some("^([aA][bB][oO][rR][tT]|[wW][aA][rR][nN])?$".to_string()),
    );
    sim_assert_eq!(
        have: ecma_case_folded_pattern("(?i)x-.*"),
        want: Some("[xX]-.*".to_string()),
    );
    sim_assert_eq!(
        have: ecma_case_folded_pattern("^(?i)(?:no|off)$"),
        want: Some("^(?:[nN][oO]|[oO][fF][fF])$".to_string()),
    );
}

#[test]
fn fold_keeps_unicode_simple_fold_partners() {
    sim_assert_eq!(
        have: ecma_case_folded_pattern("^(?i)ks$"),
        want: Some("^[kK\u{212A}][sS\u{017F}]$".to_string()),
    );
    sim_assert_eq!(
        have: ecma_case_folded_pattern("^(?i)[ks]$"),
        want: Some("^[kK\u{212A}sS\u{017F}]$".to_string()),
    );
}

#[test]
fn fold_handles_classes_and_negations_member_wise() {
    sim_assert_eq!(
        have: ecma_case_folded_pattern("^(?i)[abc7_.-]+$"),
        want: Some("^[aAbBcC7_.-]+$".to_string()),
    );
    sim_assert_eq!(
        have: ecma_case_folded_pattern("^(?i)[^ab]$"),
        want: Some("^[^aAbB]$".to_string()),
    );
    sim_assert_eq!(
        have: ecma_case_folded_pattern(r"^(?i)a\.b\d+$"),
        want: Some(r"^[aA]\.[bB]\d+$".to_string()),
    );
}

#[test]
fn unfoldable_constructs_abstain() {
    for pattern in [
        // no leading global flag group at all
        "^(abort|warn)?$",
        // letter ranges cannot fold member-wise
        "^(?i)[a-z]+$",
        // indirect letter escapes and group references
        r"^(?i)\x41$",
        r"^(?i)\p{L}$",
        r"^(?i)(a)\1$",
        // lookaround, named groups, mid-pattern flags
        "^(?i)(?=a)b$",
        "^(?i)(?P<name>a)$",
        "^(?i)a(?m)b$",
        // non-ASCII case orbits (σ/Σ/ς) are not char-wise foldable
        "^(?i)σ$",
        // unterminated class
        "^(?i)[ab$",
    ] {
        sim_assert_eq!(
            have: ecma_case_folded_pattern(pattern),
            want: None,
            "pattern {pattern:?} must abstain",
        );
    }
}

#[test]
fn normalization_walks_schema_positions_only() {
    let mut schema = serde_json::json!({
        "type": "object",
        "properties": {
            "strategy": { "type": "string", "pattern": "^(?i)(abort|warn)?$" },
            // A property NAMED pattern: its value is a schema, and the
            // instance-data spelling inside `enum`/`const`/`default`
            // stays untouched.
            "pattern": {
                "type": "string",
                "enum": ["^(?i)raw$"],
                "default": "^(?i)raw$",
            },
        },
        "patternProperties": {
            "^(?i)x-": { "type": "string" },
        },
        "$defs": {
            "shared": {
                "allOf": [ { "pattern": "(?i)abc" } ],
            },
        },
        "const": { "pattern": "^(?i)raw$" },
    });
    normalize_schema_pattern_dialects(&mut schema);
    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "type": "object",
            "properties": {
                "strategy": {
                    "type": "string",
                    "pattern": "^([aA][bB][oO][rR][tT]|[wW][aA][rR][nN])?$",
                },
                "pattern": {
                    "type": "string",
                    "enum": ["^(?i)raw$"],
                    "default": "^(?i)raw$",
                },
            },
            "patternProperties": {
                "^[xX]-": { "type": "string" },
            },
            "$defs": {
                "shared": {
                    "allOf": [ { "pattern": "[aA][bB][cC]" } ],
                },
            },
            "const": { "pattern": "^(?i)raw$" },
        }),
    );
}

#[test]
fn folded_patterns_accept_exactly_the_re2_language() {
    // Differential check against Rust's regex crate, whose `(?i)` follows
    // the same simple-fold semantics as Go's RE2.
    let cases: &[(&str, &[&str])] = &[
        (
            "^(?i)(abort|warn)?$",
            &[
                "abort", "ABORT", "Abort", "warn", "WaRn", "", "abort ", "x", "warnn",
            ],
        ),
        ("^(?i)[abc7_.-]+$", &["aB7", "CCC", "-._", "d", "", "A-b"]),
        (
            "^(?i)ks$",
            &["ks", "KS", "kS", "\u{212A}s", "k\u{017F}", "kss"],
        ),
    ];
    for (pattern, inputs) in cases {
        let folded = ecma_case_folded_pattern(pattern).expect("foldable");
        let original = regex::Regex::new(pattern).expect("compile RE2 spelling");
        let rewritten = regex::Regex::new(&folded).expect("compile folded spelling");
        for input in *inputs {
            sim_assert_eq!(
                have: rewritten.is_match(input),
                want: original.is_match(input),
                "pattern {pattern:?} folded {folded:?} input {input:?}",
            );
        }
    }
}

#[test]
fn pattern_properties_key_collision_keeps_original() {
    let mut schema = serde_json::json!({
        "patternProperties": {
            "^(?i)x-": { "maxLength": 3 },
            "^[xX]-": { "type": "string" },
        },
    });
    normalize_schema_pattern_dialects(&mut schema);
    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "patternProperties": {
                "^(?i)x-": { "maxLength": 3 },
                "^[xX]-": { "type": "string" },
            },
        }),
    );
}

#[test]
fn value_is_unchanged_when_no_pattern_present() {
    let mut schema = serde_json::json!({ "type": "string", "minLength": 1 });
    let expected = schema.clone();
    normalize_schema_pattern_dialects(&mut schema);
    sim_assert_eq!(have: schema, want: expected);
}
