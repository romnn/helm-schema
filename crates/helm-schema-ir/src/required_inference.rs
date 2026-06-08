//! Static-analysis primitives consumed solely by the heuristic
//! `--infer-required` feature in `helm-schema-gen` /
//! `helm-schema-cli`.
//!
//! Lives in its own module so the entire required-inference feature can
//! be removed by deleting this file plus the matching modules in the
//! two downstream crates. Nothing in `helm_schema_ir`'s core
//! API consumes anything here.

use helm_schema_ast::{TemplateExpr, parse_action_expressions};

use crate::walker::{strip_yaml_comment_lines, values_path_from_expr};

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
/// Walks the typed AST from
/// [`helm_schema_ast::parse_action_expressions`], so string-literal
/// payloads like `{{ "default 5 .Values.x" | quote }}` never produce
/// phantom paths.
#[must_use]
pub fn extract_default_fallback_paths(text: &str) -> Vec<String> {
    // Skip YAML-comment lines (same convention as
    // [`crate::extract_default_type_hints`]): `# example: …` style
    // documentation must not contribute fallback signals.
    let cleaned = strip_yaml_comment_lines(text);
    let mut out: Vec<String> = Vec::new();
    for top in parse_action_expressions(&cleaned) {
        // `walk` recurses through `Parenthesized` for us, so the
        // visitor's `expr` is never the parens wrapper at match time —
        // adding `deparen` here would double-count nested forms like
        // `default 5 (default "x" .Values.X)`. Arg-level `deparen` IS
        // safe (the args don't visit independently for this pattern).
        top.walk(|expr| match expr {
            // Prefix form: `default <any-expr> .Values.X`. The first
            // arg can be anything (literal, identifier, parenthesised
            // pipeline). The second arg must resolve to a `.Values.X`
            // path. `default` with arg-count other than 2 is treated
            // as the pipeline form (one arg, takes its value from the
            // pipe input) — handled in the `Pipeline` branch below.
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                if let Some(path) = values_path_from_expr(&args[1]) {
                    out.push(path);
                }
            }
            // Pipeline form: `.Values.X | default <any>`. The presence
            // of `default` after the pipe is the signal — its argument
            // (if any) is irrelevant for the "has fallback" classification.
            // Parens around the consumer call (`.Values.X | (default …)`)
            // are syntactic grouping; peel them on the consumer slot only.
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let Some(path) = values_path_from_expr(&window[0]) else {
                        continue;
                    };
                    let TemplateExpr::Call { function, .. } = window[1].deparen() else {
                        continue;
                    };
                    if function == "default" {
                        out.push(path);
                    }
                }
            }
            _ => {}
        });
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::extract_default_fallback_paths;

    fn paths(src: &str) -> Vec<String> {
        extract_default_fallback_paths(src)
    }

    #[test]
    fn literal_first_arg_emits_path() {
        assert_eq!(
            paths(r#"{{ default "x" .Values.X }}"#),
            vec!["X".to_string()]
        );
    }

    #[test]
    fn non_literal_first_arg_still_emits_path() {
        // Unlike `extract_default_type_hints`, the fallback extractor
        // accepts any first argument shape — the existence of a
        // default IS the signal, regardless of value type.
        assert_eq!(
            paths(r#"{{ default .Chart.Name .Values.X }}"#),
            vec!["X".to_string()],
        );
        assert_eq!(
            paths(r#"{{ default (printf "%s" .Y) .Values.X }}"#),
            vec!["X".to_string()],
        );
        assert_eq!(
            paths(r#"{{ default $root.Foo .Values.X }}"#),
            vec!["X".to_string()],
        );
    }

    #[test]
    fn pipeline_form_emits_path() {
        assert_eq!(
            paths(r#"{{ .Values.X | default "x" }}"#),
            vec!["X".to_string()],
        );
    }

    #[test]
    fn yaml_comment_line_is_ignored() {
        // Documentation by convention — must not contribute a path.
        let src = "# example: {{ default \"x\" .Values.X }}\nname: real\n";
        assert!(paths(src).is_empty());
    }

    #[test]
    fn quoted_payload_does_not_emit_phantom_path() {
        // The `default ... .Values.X` substring lives inside a Go
        // string literal — the typed AST sees it as a Literal::String,
        // never a Call.
        let src = r#"{{ "default 5 .Values.X" | quote }}"#;
        assert!(paths(src).is_empty());
    }

    #[test]
    fn sorted_and_deduped() {
        let src = r#"
            {{ default 1 .Values.b }}
            {{ default 2 .Values.a }}
            {{ default 3 .Values.b }}
        "#;
        assert_eq!(paths(src), vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn rooted_dollar_dotvalues_path() {
        assert_eq!(
            paths(r#"{{ default 1 $.Values.X }}"#),
            vec!["X".to_string()],
        );
    }
}
