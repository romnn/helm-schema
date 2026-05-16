//! Static-analysis primitives consumed solely by the heuristic
//! `--infer-required` feature in `helm-schema-gen` /
//! `helm-schema-cli`.
//!
//! Lives in its own module so the entire required-inference feature can
//! be removed by deleting this file plus the matching modules in the
//! two downstream crates. Nothing in `helm_schema_ir`'s core
//! API consumes anything here.

use std::sync::OnceLock;

use regex::Regex;

use crate::walker::preprocess_for_hint_extraction;

/// Extract paths that have a `default <ANY-EXPR> .Values.X` (prefix) or
/// `.Values.X | default <ANY-EXPR>` (pipeline) fallback. Broader than
/// [`crate::extract_default_type_hints`]: the first argument to
/// `default` can be any expression, not just a literal. The literal
/// version is consumed by core type inference; this broader variant is
/// only meaningful for "this path has *some* fallback, exclude from
/// required" — a heuristic, hence its placement here.
///
/// Examples that qualify:
///   `default "x" .Values.X`            ← also a literal hint
///   `default .Chart.Name .Values.X`    ← identifier expression
///   `default $root.Foo .Values.X`      ← variable reference
///   `default (printf "%s" .X) .Values.Y`  ← parenthesized expression
///   `.Values.X | default (...)`        ← pipeline form
///
/// Same comment / string-literal pre-filtering as
/// [`crate::extract_default_type_hints`] — re-uses the
/// `walker::preprocess_for_hint_extraction` utility.
#[must_use]
pub fn extract_default_fallback_paths(text: &str) -> Vec<String> {
    let cleaned = preprocess_for_hint_extraction(text);
    let regexes = fallback_re();
    let mut out: Vec<String> = Vec::new();
    for caps in regexes.prefix.captures_iter(&cleaned) {
        out.push(caps["path"].to_string());
    }
    for caps in regexes.pipeline.captures_iter(&cleaned) {
        out.push(caps["path"].to_string());
    }
    out.sort();
    out.dedup();
    out
}

struct FallbackPathRegexes {
    prefix: Regex,
    pipeline: Regex,
}

fn fallback_re() -> &'static FallbackPathRegexes {
    static R: OnceLock<FallbackPathRegexes> = OnceLock::new();
    R.get_or_init(|| {
        let path = r"[\w]+(?:\.[\w]+)*";
        let values_prefix = r"(?:\$\w*)?\.Values\.";
        // First argument to `default`: any of:
        //   - a quoted string literal (which may contain spaces — `\S+`
        //     would split on the first space and miss the rest)
        //   - a parenthesized expression (single-level; nested parens are
        //     unusual in chart helpers and a regex can't match balanced
        //     groups precisely)
        //   - a single whitespace-free token (numeric literal, bool,
        //     identifier, dot-path, $var)
        //
        // Order matters: try the quoted form first so we consume the full
        // `"two words"` rather than letting `\S+` settle for `"two`.
        let arg = r#"(?:"[^"]*"|\([^)]*\)|\S+)"#;
        FallbackPathRegexes {
            prefix: Regex::new(&format!(
                r"\bdefault\s+{arg}\s+{values_prefix}(?P<path>{path})"
            ))
            .expect("fallback-prefix regex"),
            // Pipeline form: we only need to know `default` follows the
            // pipe — the fallback expression itself is irrelevant for the
            // "has fallback" classification.
            pipeline: Regex::new(&format!(
                r"{values_prefix}(?P<path>{path})\s*\|\s*default\b"
            ))
            .expect("fallback-pipeline regex"),
        }
    })
}
