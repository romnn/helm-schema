use serde_json::{Map, Value};

use crate::Guard;
pub use crate::helper_discovery::{DefineBlock, extract_define_blocks, extract_helper_calls};

/// A type hint extracted from a `default <literal> .Values.X` template
/// pattern: the literal's JSON-mappable type tells us what the contractual
/// type of `X` is, even when values.yaml only has a null placeholder.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DefaultLiteralType {
    String,
    Integer,
    Number,
    Boolean,
}

impl DefaultLiteralType {
    fn schema(&self) -> Value {
        let ty = match self {
            DefaultLiteralType::String => "string",
            DefaultLiteralType::Integer => "integer",
            DefaultLiteralType::Number => "number",
            DefaultLiteralType::Boolean => "boolean",
        };
        let mut m = Map::new();
        m.insert("type".to_string(), Value::String(ty.to_string()));
        Value::Object(m)
    }
}

/// Extract type hints implied by `default <literal> .Values.X` and
/// `.Values.X | default <literal>` patterns in template text.
///
/// Returns pairs of `(values_path, schema_fragment)` where
/// `schema_fragment` reflects the literal's JSON type
/// (`string`/`integer`/`number`/`boolean`). Patterns where the first
/// argument to `default` is a non-literal (function call, computed
/// expression) are skipped — the broader "has *some* fallback"
/// classification lives in
/// [`crate::required_inference::extract_default_fallback_paths`].
///
/// Parses each `{{ ... }}` action via the
/// [`helm_schema_ast::parse_action_expressions`] typed AST, then walks
/// for two shapes:
///   - `Call { function: "default", args: [Literal(lit), <.Values.X>] }`
///   - `Pipeline([<.Values.X>, Call { function: "default", args: [Literal(lit)] }])`
///
/// Because the AST distinguishes string literals from raw expression
/// text, a `default 5 .Values.x` substring living inside a quoted
/// payload (e.g. `{{ "default 5 .Values.x" | quote }}`) is parsed as
/// `Literal::String` and never produces a phantom hint.
///
/// Lines that are YAML comments (start with `#`, possibly indented)
/// are stripped from `text` *before* parsing. Helm WILL execute any
/// `{{ ... }}` action embedded in such a line, but the surrounding
/// `# example: ...` convention strongly signals "this is documentation,
/// not a real binding" — we preserve that intent.
#[must_use]
pub fn extract_default_type_hints(text: &str) -> Vec<(String, Value)> {
    use helm_schema_ast::{TemplateExpr, parse_action_expressions};

    let cleaned = strip_yaml_comment_lines(text);
    let mut out: Vec<(String, Value)> = Vec::new();
    for top in parse_action_expressions(&cleaned) {
        // `walk` already recurses through `Parenthesized`, so the
        // visitor's `expr` here is never the parens wrapper — by the
        // time control reaches a match arm, the wrapping parens have
        // been peeled by the walk itself. Don't `deparen` again: the
        // walk would emit the same wrapped subtree twice (once at the
        // Parenthesized parent, once at the inner), duplicating hints
        // for nested forms like `default 5 (default "x" .Values.X)`.
        // Arg-level `deparen` IS safe (those values aren't visited
        // independently for `default`-match purposes — they ride along
        // with the matched Call).
        top.walk(|expr| match expr {
            // Prefix form: `default LIT .Values.X`. Parens around the
            // literal (`default (LIT) (.Values.X)`) are syntactic
            // grouping; peel them before classifying.
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let TemplateExpr::Literal(lit) = args[0].deparen() else {
                    return;
                };
                let Some(ty) = classify_literal_type(lit) else {
                    return;
                };
                let Some(path) = values_path_from_expr(&args[1]) else {
                    return;
                };
                out.push((path, ty.schema()));
            }
            // Pipeline form: `.Values.X | default LIT`. Stages may have
            // been written in parens (`(.Values.X) | (default LIT)`) —
            // parens are syntactic grouping, so `deparen` peels them
            // before the path / call patterns fire.
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let Some(path) = values_path_from_expr(&window[0]) else {
                        continue;
                    };
                    let TemplateExpr::Call { function, args } = window[1].deparen() else {
                        continue;
                    };
                    if function != "default" || args.len() != 1 {
                        continue;
                    }
                    let TemplateExpr::Literal(lit) = args[0].deparen() else {
                        continue;
                    };
                    let Some(ty) = classify_literal_type(lit) else {
                        continue;
                    };
                    out.push((path, ty.schema()));
                }
            }
            _ => {}
        });
    }
    out
}

/// Map an AST [`helm_schema_ast::Literal`] to the corresponding
/// JSON Schema scalar type. Returns `None` for `Nil`.
fn classify_literal_type(lit: &helm_schema_ast::Literal) -> Option<DefaultLiteralType> {
    match lit {
        helm_schema_ast::Literal::String(_) | helm_schema_ast::Literal::RawString(_) => {
            Some(DefaultLiteralType::String)
        }
        helm_schema_ast::Literal::Int(_) => Some(DefaultLiteralType::Integer),
        helm_schema_ast::Literal::Float(_) => Some(DefaultLiteralType::Number),
        helm_schema_ast::Literal::Bool(_) => Some(DefaultLiteralType::Boolean),
        helm_schema_ast::Literal::Nil => None,
    }
}

/// Strip YAML-comment lines (those whose first non-whitespace char is
/// `#`) from `src`. Pure-comment lines never produce real YAML keys at
/// render time; any `{{ ... }}` action embedded in them is documentation
/// by convention. Called before [`parse_action_expressions`] so the
/// downstream extractors don't pick up phantom signals from example
/// snippets in docstring-style comments.
pub(crate) fn strip_yaml_comment_lines(src: &str) -> String {
    src.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// If `expr` is a `.Values.X.Y…` reference (root context or via a
/// `$` / `$root` variable), return the dotted path with the leading
/// `Values.` stripped (`"X.Y..."`). Otherwise returns `None`. Used by
/// both `extract_default_type_hints` and
/// `crate::required_inference::extract_default_fallback_paths`.
pub(crate) fn values_path_from_expr(expr: &helm_schema_ast::TemplateExpr) -> Option<String> {
    use helm_schema_ast::TemplateExpr as E;
    let expr = expr.deparen();
    let segments: &[String] = match expr {
        E::Field(path) => path,
        E::Selector { operand, path } => {
            // Accept `$.Values.X` and `$name.Values.X` — variable
            // operands stand in for a re-rooted context, matching the
            // chart-helper idiom `{{- $root := . -}}`.
            if !matches!(operand.as_ref(), E::Variable(_)) {
                return None;
            }
            path
        }
        _ => return None,
    };
    let mut iter = segments.iter();
    let head = iter.next()?;
    if head != "Values" {
        return None;
    }
    let tail: Vec<String> = iter.cloned().collect();
    if tail.is_empty() {
        return None;
    }
    Some(tail.join("."))
}

/// Extract `.Values.foo.bar` references → `["foo.bar"]`.
///
/// Walks the typed expression AST (via
/// [`helm_schema_ast::parse_action_expressions`]) so bytes inside a Go
/// string literal — e.g. `eq .Values.X ".Values.fake"` — can no longer
/// masquerade as additional `.Values.*` references. The result is
/// sorted and deduplicated.
///
/// Wildcards (`*` segments) are accepted at non-leading positions to
/// match what [`crate::symbolic::SymbolicIrContext`]'s dot-binding
/// rewrite produces (e.g. `.Values.someList.*.name`). The first segment
/// after `Values` must be a real identifier, matching the old regex's
/// `[\w]+(?:\.(?:[\w]+|\*))*` shape.
#[must_use]
#[cfg(test)]
pub fn extract_values_paths(text: &str) -> Vec<String> {
    let mut paths: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for top in parse_bare_expression_text(text) {
        collect_loose_values_paths(&top, &mut paths);
    }
    paths.into_iter().collect()
}

/// Walk `expr` preorder and insert every loose-form `.Values.X`
/// reference into `out`. The set-based output ensures sort+dedup
/// without an extra pass and lets callers union results from multiple
/// expressions cheaply.
fn collect_loose_values_paths(
    expr: &helm_schema_ast::TemplateExpr,
    out: &mut std::collections::BTreeSet<String>,
) {
    expr.walk(|node| {
        if let Some(path) = values_path_from_expr_loose(node) {
            out.insert(path);
        }
    });
}

/// Parse `text` as a bare Go template expression (no `{{ }}` braces)
/// and return every top-level expression the wrapped action produces.
///
/// Wraps the input in `{{ ... }}` so tree-sitter recognises it as an
/// action body. Wildcard markers (`*` segments produced by the
/// dot-binding rewrite in [`crate::symbolic::SymbolicIrContext`])
/// are first substituted with a placeholder identifier (tree-sitter
/// rejects bare `*` in selector chains), then restored to literal
/// `*` in every node of the returned tree — so callers see a normal
/// `Field(["someList", "*", "name"])` and never have to know the
/// placeholder exists. Returns an empty `Vec` for blank input or
/// unparseable text; never panics.
fn parse_bare_expression_text(text: &str) -> Vec<helm_schema_ast::TemplateExpr> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let has_wildcards = trimmed.contains(".*");
    let normalised = if has_wildcards {
        trimmed.replace(".*", &format!(".{WILDCARD_PLACEHOLDER}"))
    } else {
        trimmed.to_string()
    };
    let wrapped = if normalised.trim_start().starts_with("{{") {
        normalised
    } else {
        format!("{{{{ {normalised} }}}}")
    };
    let mut exprs = helm_schema_ast::parse_action_expressions(&wrapped);
    if has_wildcards {
        for e in &mut exprs {
            restore_wildcards_in_expr(e);
        }
    }
    exprs
}

/// Placeholder identifier substituted for `*` path segments so
/// tree-sitter can parse [`crate::symbolic::SymbolicIrContext`]'s
/// dot-binding rewrites. Stripped from the AST in
/// [`restore_wildcards_in_expr`] before any caller observes a parse
/// result — the constant is purely an internal token, never exposed.
const WILDCARD_PLACEHOLDER: &str = "__hsast_wildcard_marker__";

/// Walk `expr` and replace every [`WILDCARD_PLACEHOLDER`] occurrence
/// — in `Field`/`Selector` path segments and in `Literal::String` /
/// `Literal::RawString` content — with a literal `*`. Centralises the
/// "undo substitution" step so no extractor can accidentally surface
/// the internal placeholder to a caller.
fn restore_wildcards_in_expr(expr: &mut helm_schema_ast::TemplateExpr) {
    use helm_schema_ast::{Literal, TemplateExpr};
    match expr {
        TemplateExpr::Field(path) => restore_segments(path),
        TemplateExpr::Selector { operand, path } => {
            restore_segments(path);
            restore_wildcards_in_expr(operand);
        }
        TemplateExpr::Literal(Literal::String(s) | Literal::RawString(s)) => {
            if s.contains(WILDCARD_PLACEHOLDER) {
                *s = s.replace(WILDCARD_PLACEHOLDER, "*");
            }
        }
        TemplateExpr::Call { args, .. } => {
            for a in args {
                restore_wildcards_in_expr(a);
            }
        }
        TemplateExpr::Pipeline(stages) => {
            for s in stages {
                restore_wildcards_in_expr(s);
            }
        }
        TemplateExpr::Parenthesized(inner) => restore_wildcards_in_expr(inner),
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            restore_wildcards_in_expr(value)
        }
        TemplateExpr::Literal(_) | TemplateExpr::Variable(_) | TemplateExpr::Unknown(_) => {}
    }
}

fn restore_segments(segments: &mut [String]) {
    for seg in segments {
        if seg == WILDCARD_PLACEHOLDER {
            "*".clone_into(seg);
        }
    }
}

/// "Loose" path extractor for [`parse_condition`] and related tests:
/// locates `Values` *anywhere* in a selector chain
/// (`.context.Values.X` → `"X"`), matching the old regex which matched
/// any `.Values.X` substring in the input text. Unlike
/// [`values_path_from_expr`] (used by literal-default extractors,
/// which require the chain to be rooted at `.Values`), this accepts
/// embedded chains commonly produced by chart helpers — e.g.
/// `dict "context" .` followed by `.context.Values.X` in the
/// downstream helper body.
///
/// Rejects paths whose first segment after `Values` is `*`, matching
/// the old regex's `[\w]+(?:\.(?:[\w]+|\*))*` requirement that the
/// segment immediately after `Values` be a real identifier.
fn values_path_from_expr_loose(expr: &helm_schema_ast::TemplateExpr) -> Option<String> {
    use helm_schema_ast::TemplateExpr as E;
    let expr = expr.deparen();
    let segments: &[String] = match expr {
        E::Field(path) | E::Selector { path, .. } => path,
        _ => return None,
    };
    let values_idx = segments.iter().position(|s| s == "Values")?;
    let tail = &segments[values_idx + 1..];
    if tail.first()?.as_str() == "*" {
        return None;
    }
    Some(tail.join("."))
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
/// Parses via [`helm_schema_ast::parse_action_expressions`] and
/// pattern-matches on the typed AST, so payloads like
/// `eq .Values.X ".Values.fake"` correctly classify as `Eq` instead of
/// silently falling through to `[Truthy("X"), Truthy("fake")]` due to
/// string-literal contamination.
///
/// Returns an empty vec if no `.Values.*` references are found.
#[must_use]
pub fn parse_condition(text: &str) -> Vec<Guard> {
    use helm_schema_ast::TemplateExpr;
    use std::collections::BTreeSet;

    let Some(top) = parse_bare_expression_text(text).into_iter().next() else {
        return Vec::new();
    };

    // Each `not`/`or`/`eq`/`ne` branch collects every `.Values.*`
    // reference found anywhere inside the builtin's argument tree,
    // not just at the immediate operand position. That matches how
    // chart helpers nest references inside `has`, `quote`, `list`,
    // etc. Walking the typed AST is contamination-free because
    // string literals never expose `Field`/`Selector` nodes — the
    // old regex would happily match `.Values.X` bytes inside
    // `"some .Values.X string"`.
    let mut paths: BTreeSet<String> = BTreeSet::new();
    collect_loose_values_paths(&top, &mut paths);

    if let TemplateExpr::Call { function, args } = &top {
        match function.as_str() {
            "not" => {
                if let Some(path) = single(&paths) {
                    return vec![Guard::Not { path }];
                }
            }
            "or" if paths.len() >= 2 => {
                return vec![Guard::Or {
                    paths: paths.into_iter().collect(),
                }];
            }
            "eq" => {
                if let Some(path) = single(&paths)
                    && let Some(value) = first_string_literal(args)
                {
                    return vec![Guard::Eq { path, value }];
                }
            }
            "ne" => {
                if let Some(path) = single(&paths) {
                    return vec![Guard::Truthy { path }];
                }
            }
            "typeIs" => {
                if let Some(path) = single(&paths)
                    && let Some(schema_type) = type_is_schema_type(args.first())
                {
                    return vec![Guard::TypeIs { path, schema_type }];
                }
            }
            // Other keywords (`and`, plus unrecognised builtins) fall
            // through to the default truthy-collection branch below —
            // every embedded `.Values.X` reference becomes its own
            // `Truthy` guard, matching the old regex pipeline.
            _ => {}
        }
    }

    paths
        .into_iter()
        .map(|path| Guard::Truthy { path })
        .collect()
}

fn type_is_schema_type(expr: Option<&helm_schema_ast::TemplateExpr>) -> Option<String> {
    let helm_schema_ast::TemplateExpr::Literal(
        helm_schema_ast::Literal::String(type_name)
        | helm_schema_ast::Literal::RawString(type_name),
    ) = expr?.deparen()
    else {
        return None;
    };
    let schema_type = match type_name.as_str() {
        "bool" | "boolean" => "boolean",
        "float64" | "number" => "number",
        "int" | "int64" | "integer" => "integer",
        "list" | "slice" | "array" => "array",
        "map" | "dict" | "object" => "object",
        "string" => "string",
        _ => return None,
    };
    Some(schema_type.to_string())
}

/// Return the single element of `paths` (cloned) if there's exactly
/// one, else `None`. Lets the `not`/`eq`/`ne` arms read the singleton
/// without an `.expect()` on an iterator.
fn single(paths: &std::collections::BTreeSet<String>) -> Option<String> {
    if paths.len() == 1 {
        paths.iter().next().cloned()
    } else {
        None
    }
}

/// Walk `exprs` preorder and return the decoded content of the first
/// `Literal::String` (or `RawString`) encountered. Used by `eq`'s
/// guard classification to find the comparison value, mirroring the
/// old regex's `"([^"]*)"` capture which also returned the first
/// quoted span.
fn first_string_literal(exprs: &[helm_schema_ast::TemplateExpr]) -> Option<String> {
    let mut found: Option<String> = None;
    for e in exprs {
        if found.is_some() {
            break;
        }
        e.walk(|node| {
            if found.is_some() {
                return;
            }
            if let helm_schema_ast::TemplateExpr::Literal(lit) = node
                && let Some(s) = lit.as_string()
            {
                found = Some(s.to_string());
            }
        });
    }
    found
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

#[cfg(test)]
mod default_type_hint_tests {
    use super::extract_default_type_hints;
    use serde_json::json;

    fn hints(src: &str) -> Vec<(String, serde_json::Value)> {
        extract_default_type_hints(src)
    }

    #[test]
    fn prefix_literal_emits_typed_hint() {
        assert_eq!(
            hints(r#"{{ default 5 .Values.replicas }}"#),
            vec![("replicas".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn pipeline_literal_emits_typed_hint() {
        assert_eq!(
            hints(r#"{{ .Values.replicas | default 5 }}"#),
            vec![("replicas".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn nested_default_inner_emits_hint_outer_does_not() {
        // `default 5 (default "x" .Values.X)` — the OUTER call has
        // args [Int, Parenthesized(...)], where args[1] is not a
        // `.Values.X` reference (it's a wrapped expression). So no
        // hint for the outer. The INNER call IS a direct `default LIT
        // .Values.X` pattern → hint emitted with String type.
        assert_eq!(
            hints(r#"{{ default 5 (default "x" .Values.X) }}"#),
            vec![("X".to_string(), json!({"type": "string"}))],
        );
    }

    #[test]
    fn chained_defaults_emit_one_hint_for_innermost_path() {
        // `.Values.X | default 5 | default 10` — only the first pipe
        // pair `(.Values.X, default 5)` matches the pattern. The
        // second pair `(default 5, default 10)` has a Call as its
        // first stage, not a `.Values.X` path, so it doesn't match.
        assert_eq!(
            hints(r#"{{ .Values.X | default 5 | default 10 }}"#),
            vec![("X".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn intervening_call_breaks_pipeline_pattern() {
        // `.Values.X | required "msg" | default 5` — `required` sits
        // between the path and `default`. The pattern matcher only
        // pairs adjacent stages, and `(required, default)` has a
        // non-Field first half, so no hint is emitted. Helm semantics
        // agree: `default 5` fires on `required(...)`'s return value,
        // not on `.Values.X` directly.
        assert!(hints(r#"{{ .Values.X | required "msg" | default 5 }}"#).is_empty(),);
    }

    #[test]
    fn rooted_dollar_dotvalues_path_is_recognised() {
        // `$.Values.X` should resolve to path "X" — the `$` is a bare
        // variable rebinding the root scope.
        assert_eq!(
            hints(r#"{{ default 5 $.Values.X }}"#),
            vec![("X".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn rooted_named_variable_dotvalues_path_is_recognised() {
        // `$root.Values.X` — `$root := .` is a common chart-helper
        // idiom inside range/with bodies where `.` has been rebound.
        assert_eq!(
            hints(r#"{{ default 5 $root.Values.X }}"#),
            vec![("X".to_string(), json!({"type": "integer"}))],
        );
    }

    #[test]
    fn default_with_non_values_target_no_hint() {
        // `default 5 .NotValues.X` — second arg's head is not "Values",
        // so no hint.
        assert!(hints(r#"{{ default 5 .NotValues.X }}"#).is_empty());
    }

    #[test]
    fn default_with_dot_only_no_hint() {
        // `default 5 .` — second arg is the bare context, not a
        // Values path.
        assert!(hints(r#"{{ default 5 . }}"#).is_empty());
    }

    #[test]
    fn default_with_parenthesised_first_arg_no_hint() {
        // First arg is not a literal — it's a Parenthesized call. The
        // type cannot be inferred from a non-literal default value.
        // (The path X is still a real *use*, but the hint feature is
        // literal-only by design.)
        assert!(hints(r#"{{ default (printf "%s" .Y) .Values.X }}"#).is_empty());
    }

    #[test]
    fn bool_literal_classified_as_boolean() {
        assert_eq!(
            hints(r#"{{ default true .Values.enabled }}"#),
            vec![("enabled".to_string(), json!({"type": "boolean"}))],
        );
    }

    #[test]
    fn nil_literal_emits_no_hint() {
        // `default nil` doesn't constrain the type at all.
        assert!(hints(r#"{{ default nil .Values.X }}"#).is_empty());
    }
}

#[cfg(test)]
mod extract_values_paths_tests {
    use super::extract_values_paths;

    #[test]
    fn root_chain_extracted() {
        assert_eq!(
            extract_values_paths(".Values.foo.bar"),
            vec!["foo.bar".to_string()]
        );
    }

    #[test]
    fn quoted_payload_does_not_create_phantom_path() {
        // The .Values.fake substring lives inside a string literal —
        // typed AST sees Literal::String, not Field. Old regex would
        // have matched it and produced a phantom path.
        let text = r#"eq .Values.X ".Values.fake""#;
        assert_eq!(extract_values_paths(text), vec!["X".to_string()]);
    }

    #[test]
    fn rooted_dollar_values_path() {
        assert_eq!(extract_values_paths("$.Values.X"), vec!["X".to_string()]);
    }

    #[test]
    fn rooted_named_variable_values_path() {
        assert_eq!(
            extract_values_paths("$root.Values.Y"),
            vec!["Y".to_string()]
        );
    }

    #[test]
    fn embedded_values_in_helper_context_chain() {
        // `.context.Values.X` — Values is the second segment, not the
        // root. Old regex matched anywhere; loose extractor preserves
        // that behaviour so chart helpers receiving a context dict
        // (e.g. `{{ template "x" (dict "context" .) }}`) still produce
        // signals.
        assert_eq!(
            extract_values_paths(".context.Values.X"),
            vec!["X".to_string()],
        );
    }

    #[test]
    fn multiple_refs_are_sorted_and_deduped() {
        let text = ".Values.b .Values.a .Values.b";
        assert_eq!(
            extract_values_paths(text),
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn wildcard_segment_in_rewritten_path() {
        // Rewritten by symbolic::rewrite_dot_expr_to_values when a
        // range body references `.foo` against a `.Values.someList`
        // header. The trailing wildcard must round-trip.
        assert_eq!(
            extract_values_paths(".Values.someList.*.name"),
            vec!["someList.*.name".to_string()],
        );
    }

    #[test]
    fn empty_text_returns_empty() {
        assert!(extract_values_paths("").is_empty());
        assert!(extract_values_paths("   \n  ").is_empty());
    }

    #[test]
    fn no_values_reference_returns_empty() {
        assert!(extract_values_paths(".Chart.Name").is_empty());
        assert!(extract_values_paths("printf \"%s\" $var").is_empty());
    }

    #[test]
    fn dot_star_inside_string_literal_does_not_emit_phantom_wildcard_path() {
        // The wildcard substitution is applied to the whole text, but
        // restoration runs over EVERY node of the parsed AST — so a
        // string literal containing `.*` doesn't masquerade as a
        // Values reference, AND the path extraction for `.Values.X`
        // still works alongside it.
        assert_eq!(
            extract_values_paths(r#"eq .Values.X "pattern.*foo""#),
            vec!["X".to_string()],
        );
    }

    #[test]
    fn dot_values_substring_inside_string_does_not_emit_phantom() {
        // Contamination guard: `.Values.fake` inside a quoted payload
        // is just a string, not a path.
        assert_eq!(
            extract_values_paths(r#"eq .Values.X ".Values.fake""#),
            vec!["X".to_string()],
        );
    }
}

#[cfg(test)]
mod parse_condition_tests {
    use super::parse_condition;
    use crate::Guard;

    #[test]
    fn truthy_simple_path() {
        assert_eq!(
            parse_condition(".Values.X"),
            vec![Guard::Truthy { path: "X".into() }],
        );
    }

    #[test]
    fn not_simple_path() {
        assert_eq!(
            parse_condition("not .Values.X"),
            vec![Guard::Not { path: "X".into() }],
        );
    }

    #[test]
    fn not_with_nested_helper_call() {
        // Old code matched `.Values.X` inside `not (has .Values.X ...)`
        // and emitted `Not`. Preserved by the new typed walker.
        assert_eq!(
            parse_condition(r#"not (has (quote .Values.global.logLevel) (list "" (quote "")))"#),
            vec![Guard::Not {
                path: "global.logLevel".into(),
            }],
        );
    }

    #[test]
    fn or_with_two_paths_emits_or_guard() {
        assert_eq!(
            parse_condition("or .Values.A .Values.B"),
            vec![Guard::Or {
                paths: vec!["A".into(), "B".into()],
            }],
        );
    }

    #[test]
    fn or_paths_are_sorted() {
        assert_eq!(
            parse_condition("or .Values.z .Values.a"),
            vec![Guard::Or {
                paths: vec!["a".into(), "z".into()],
            }],
        );
    }

    #[test]
    fn or_with_nested_helper_calls() {
        // Old code accepted nested paths inside `or (has .Values.A) (has .Values.B)`.
        assert_eq!(
            parse_condition("or (has .Values.A 1) (has .Values.B 2)"),
            vec![Guard::Or {
                paths: vec!["A".into(), "B".into()],
            }],
        );
    }

    #[test]
    fn eq_with_string_literal() {
        assert_eq!(
            parse_condition(r#"eq .Values.X "value""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: "value".into(),
            }],
        );
    }

    #[test]
    fn eq_with_string_literal_containing_phantom_path() {
        // The contamination bug: `eq .Values.X ".Values.fake"`. Old
        // regex extracted both "X" and "fake" from the rest text,
        // failed the `paths.len() == 1` check, and silently fell
        // through to `[Truthy("X"), Truthy("fake")]`. New typed
        // walker sees Literal::String for the second arg — no
        // phantom path — and correctly emits `Eq`.
        assert_eq!(
            parse_condition(r#"eq .Values.X ".Values.fake""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: ".Values.fake".into(),
            }],
        );
    }

    #[test]
    fn eq_compare_two_values_falls_through_to_truthy() {
        // `eq .Values.X .Values.Y` — two Values refs, no string
        // literal → not a classic `eq` guard. Old code fell through;
        // new code does the same.
        assert_eq!(
            parse_condition("eq .Values.X .Values.Y"),
            vec![
                Guard::Truthy { path: "X".into() },
                Guard::Truthy { path: "Y".into() },
            ],
        );
    }

    #[test]
    fn ne_with_string_literal_emits_truthy() {
        // `ne` doesn't get its own guard kind — it's treated as a
        // truthy check on the referenced path.
        assert_eq!(
            parse_condition(r#"ne .Values.X "value""#),
            vec![Guard::Truthy { path: "X".into() }],
        );
    }

    #[test]
    fn and_falls_through_to_per_path_truthy() {
        // `and` isn't a recognised builtin keyword — it falls through
        // to the default "collect every .Values.X" branch.
        assert_eq!(
            parse_condition("and .Values.A .Values.B"),
            vec![
                Guard::Truthy { path: "A".into() },
                Guard::Truthy { path: "B".into() },
            ],
        );
    }

    #[test]
    fn and_with_parens_falls_through_to_per_path_truthy() {
        assert_eq!(
            parse_condition("and (.Values.A) (.Values.B)"),
            vec![
                Guard::Truthy { path: "A".into() },
                Guard::Truthy { path: "B".into() },
            ],
        );
    }

    #[test]
    fn empty_condition_returns_empty() {
        assert!(parse_condition("").is_empty());
        assert!(parse_condition("   ").is_empty());
    }

    #[test]
    fn condition_without_values_reference_returns_empty() {
        assert!(parse_condition(".Chart.Name").is_empty());
        assert!(parse_condition("not (empty $var)").is_empty());
    }

    #[test]
    fn eq_value_preserves_literal_dot_star_substring() {
        // Regression: the wildcard substitution `_HelmStarWildcard_` is
        // applied to the WHOLE input text — including the bytes inside
        // string literals. If we don't restore the placeholder when
        // extracting the literal's content, the returned `Eq` value
        // gets corrupted.
        assert_eq!(
            parse_condition(r#"eq .Values.X "match.*foo""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: "match.*foo".into(),
            }],
        );
    }

    #[test]
    fn eq_value_preserves_dot_values_substring_inside_string() {
        // Another preservation case: the string content itself contains
        // a `.Values.X` substring. Must NOT be extracted as a path
        // (contamination) AND the returned `Eq` value must be the
        // verbatim string content.
        assert_eq!(
            parse_condition(r#"eq .Values.X ".Values.fake""#),
            vec![Guard::Eq {
                path: "X".into(),
                value: ".Values.fake".into(),
            }],
        );
    }
}
