use color_eyre::eyre::{self, WrapErr as _};
use test_util::prelude::sim_assert_eq;

use super::{
    ecma_case_folded_pattern, escape_regex_literal, normalize_schema_pattern_dialects,
    unicode_mode_escaped_pattern,
};

#[test]
fn go_regex_literal_escaping_leaves_re2_hyphens_bare() {
    sim_assert_eq!(
        have: escape_regex_literal("prefix-with.+symbols"),
        want: r"prefix-with\.\+symbols"
    );
}

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

/// Keys that would meet at one spelling keep their originals, so neither
/// constraint is dropped and a `$ref` to either keeps its target.
#[test]
fn pattern_properties_keys_meeting_at_one_spelling_keep_their_originals() {
    let original = serde_json::json!({
        "patternProperties": {
            "^(?i)x-": { "maxLength": 3 },
            "^[xX]-": { "type": "string" },
            r"^a\ b c$": { "const": "left" },
            r"^a b\ c$": { "const": "right" },
        },
        "properties": {
            "other": { "$ref": "#/patternProperties/^a b\\ c$" },
        },
    });
    let mut schema = original.clone();
    normalize_schema_pattern_dialects(&mut schema);
    sim_assert_eq!(have: schema, want: original);
}

#[test]
fn value_is_unchanged_when_no_pattern_present() {
    let mut schema = serde_json::json!({ "type": "string", "minLength": 1 });
    let expected = schema.clone();
    normalize_schema_pattern_dialects(&mut schema);
    sim_assert_eq!(have: schema, want: expected);
}

/// The zalando postgres-operator CRD's maintenance-window pattern escapes its
/// padding spaces, which ECMA-262 rejects under the `u` flag.
#[test]
fn escaped_spaces_become_bare_literals() {
    sim_assert_eq!(
        have: unicode_mode_escaped_pattern(r"^\ *((Mon|Tue):(2[0-3]|[01]?\d))\ *$"),
        want: Some(r"^ *((Mon|Tue):(2[0-3]|[01]?\d)) *$".to_string()),
    );
}

#[test]
fn unicode_mode_rejected_escapes_are_dropped_in_every_position() {
    // Outside a class
    sim_assert_eq!(
        have: unicode_mode_escaped_pattern(r"^a\%b\-c\#d\&e\~f\:g\,h$"),
        want: Some("^a%b-c#d&e~f:g,h$".to_string()),
    );
    // Class members and range endpoints
    sim_assert_eq!(
        have: unicode_mode_escaped_pattern(r"^[\ \%]+[\!-\/]$"),
        want: Some("^[ %]+[!-\\/]$".to_string()),
    );
    // An escaped backslash is itself a syntax-character escape, so the space after it stays bare.
    sim_assert_eq!(
        have: unicode_mode_escaped_pattern(r"a\\ b\ c"),
        want: Some(r"a\\ b c".to_string()),
    );
}

#[test]
fn unicode_mode_escapes_are_kept() {
    for pattern in [
        // Syntax characters, `/`, and a class-member `-`
        r"^\^\$\\\.\*\+\?\(\)\[\]\{\}\|\/$",
        r"^[a\-z]$",
        // Letter escapes keep their meaning
        r"^\d+\s\w$",
        "^plain text$",
    ] {
        sim_assert_eq!(have: unicode_mode_escaped_pattern(pattern), want: None, "{pattern:?}");
    }
}

#[test]
fn escapes_abstain_where_rust_and_re2_disagree() {
    for pattern in [
        // Under `x`, an escaped space is the only significant space.
        r"(?x)a\ b",
        r"(?x:a\ b)",
        // RE2 reads `[` inside a class and `&&` literally; Rust nests and intersects.
        r"[a[b]\ ]",
        r"[a&&b\ ]",
        // RE2 reads `{` literally when it does not form a repetition; Rust rejects it.
        r"a{2\,3}\ ",
    ] {
        sim_assert_eq!(have: unicode_mode_escaped_pattern(pattern), want: None, "{pattern:?}");
    }
}

/// Each dialect reads the respelled pattern exactly as it read the original.
///
/// The `regex` crate stands in for RE2 on both sides, and `regress` compares
/// ECMA-262's legacy reading of the original with its `u`-mode reading of the
/// rewrite.
/// The comparison stays within each dialect because the rewrite only claims
/// to preserve each dialect's own reading; RE2 and ECMA-262 already differ on
/// constructs such as `\s`.
#[test]
fn respelled_escapes_accept_exactly_the_original_language() -> eyre::Result<()> {
    let cases: &[(&str, &[&str])] = &[
        (
            r"^\ *((Mon|Tue):(2[0-3]|[01]?\d))\ *$",
            &[
                "Mon:23",
                "  Tue:7  ",
                "Wed:1",
                "Mon:24",
                " Mon:1\t",
                "Mon:\u{661}",
            ],
        ),
        (r"^a\%b\-c\#d$", &["a%b-c#d", "a%b\\-c#d", "a%bc#d"]),
        (r"^[\ \%\!-\~]+$", &["% !~", "a", "\t", ""]),
        (r"^[a\&\&b\~\~]$", &["a", "&", "b", "~", "c", ""]),
        (r"^\ \s$", &["  ", " \t", " \u{a0}", "x"]),
    ];
    for (pattern, inputs) in cases {
        let rewritten = unicode_mode_escaped_pattern(pattern)
            .ok_or_else(|| eyre::eyre!("{pattern:?} must be rewritten"))?;
        sim_assert_eq!(
            have: jsonschema_regex::is_valid_ecma_regex(&rewritten),
            want: true,
            "{rewritten:?} must pass the strict u-mode parser",
        );
        let re2_original = regex::Regex::new(pattern)?;
        let re2_rewritten = regex::Regex::new(&rewritten)?;
        let ecma_original = regress::Regex::new(pattern)?;
        let ecma_rewritten = regress::Regex::with_flags(&rewritten, "u")
            .wrap_err_with(|| format!("{rewritten:?} must compile under the u flag"))?;
        for input in *inputs {
            sim_assert_eq!(
                have: re2_rewritten.is_match(input),
                want: re2_original.is_match(input),
                "RE2 {pattern:?} -> {rewritten:?} on {input:?}",
            );
            sim_assert_eq!(
                have: ecma_rewritten.find(input).is_some(),
                want: ecma_original.find(input).is_some(),
                "ECMA {pattern:?} -> {rewritten:?} on {input:?}",
            );
        }
    }
    Ok(())
}

/// Hex escapes keep a doubled `&` or `~` from spelling a Rust class set operator.
#[test]
fn class_ampersands_and_tildes_become_hex_escapes() {
    sim_assert_eq!(
        have: unicode_mode_escaped_pattern(r"^[a\&\&b\~\~]$"),
        want: Some(r"^[a\x26\x26b\x7E\x7E]$".to_string()),
    );
}

/// RE2 reads `\<` and `\>` as literal angle brackets, which Rust parses as
/// word-boundary assertions.
///
/// The `regex` crate cannot stand in for RE2 here, so this case sits outside
/// the differential test.
#[test]
fn escaped_angle_brackets_become_bare_literals() {
    sim_assert_eq!(
        have: unicode_mode_escaped_pattern(r"^\<a\ \>$"),
        want: Some("^<a >$".to_string()),
    );
}

#[test]
fn provider_patterns_fold_then_drop_escapes() {
    let mut schema = serde_json::json!({
        "pattern": r"^(?i)a\ b$",
        "patternProperties": { r"^x\%$": {} },
    });
    normalize_schema_pattern_dialects(&mut schema);
    sim_assert_eq!(
        have: schema,
        want: serde_json::json!({
            "pattern": "^[aA] [bB]$",
            "patternProperties": { "^x%$": {} },
        }),
    );
}
