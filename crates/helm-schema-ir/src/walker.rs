use regex::Regex;
use serde_json::{Map, Value};

use crate::Guard;

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
        top.walk(|expr| match expr {
            // Prefix form: `default LIT .Values.X`.
            TemplateExpr::Call { function, args } if function == "default" && args.len() == 2 => {
                let TemplateExpr::Literal(lit) = &args[0] else {
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
            // Pipeline form: `.Values.X | default LIT`.
            TemplateExpr::Pipeline(stages) if stages.len() >= 2 => {
                for window in stages.windows(2) {
                    let Some(path) = values_path_from_expr(&window[0]) else {
                        continue;
                    };
                    let TemplateExpr::Call { function, args } = &window[1] else {
                        continue;
                    };
                    if function != "default" || args.len() != 1 {
                        continue;
                    }
                    let TemplateExpr::Literal(lit) = &args[0] else {
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
/// match what [`crate::symbolic::SymbolicIrGenerator`]'s dot-binding
/// rewrite produces (e.g. `.Values.someList.*.name`). The first segment
/// after `Values` must be a real identifier, matching the old regex's
/// `[\w]+(?:\.(?:[\w]+|\*))*` shape.
#[must_use]
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
/// dot-binding rewrite in [`crate::symbolic::SymbolicIrGenerator`])
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
/// tree-sitter can parse [`crate::symbolic::SymbolicIrGenerator`]'s
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

/// "Loose" path extractor for [`extract_values_paths`] and
/// [`parse_condition`]: locates `Values` *anywhere* in a selector
/// chain (`.context.Values.X` → `"X"`), matching the old regex which
/// matched any `.Values.X` substring in the input text. Unlike
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

/// A `{{ define "name" }} ... {{ end }}` block extracted from template
/// source, with the body text and its byte span in the original source.
/// `body` excludes the surrounding `{{ define }}` / `{{ end }}` actions
/// themselves — it's only the rendered content between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineBlock {
    pub name: String,
    pub body: String,
    pub byte_range: std::ops::Range<usize>,
}

/// Extract all `{{ define "name" }} ... {{ end }}` blocks from template
/// source text. Handles nested control flow (`if`/`with`/`range`/inner
/// `define`) via bracket-depth tracking. Whitespace markers `{{-` /
/// `-}}` are tolerated.
///
/// Helm-comment blocks `{{/* ... */}}` are excluded from depth counting
/// so a `{{ end }}` mentioned inside a comment doesn't unbalance the
/// stack.
///
/// Defines that never close (truncated source, regex misparse) are
/// silently dropped — preferring incomplete coverage over panics.
#[must_use]
pub fn extract_define_blocks(src: &str) -> Vec<DefineBlock> {
    // Strip Helm comments so their content can't masquerade as actions.
    // Replace each comment with whitespace of the same length to keep
    // byte offsets aligned with the original `src`.
    let preserved = mask_helm_comments(src);

    let action_re = template_action_regex();

    #[derive(Debug)]
    struct OpenDefine {
        name: String,
        body_start: usize,
        action_start: usize,
        depth_when_opened: usize,
    }

    let mut depth: usize = 0;
    let mut stack: Vec<OpenDefine> = Vec::new();
    let mut out: Vec<DefineBlock> = Vec::new();

    for cap in action_re.captures_iter(&preserved) {
        let action_match = cap.get(0).expect("re match");
        let inner = cap.get(1).expect("re capture group 1").as_str().trim();

        if let Some(name) = parse_define_directive(inner) {
            stack.push(OpenDefine {
                name,
                body_start: action_match.end(),
                action_start: action_match.start(),
                depth_when_opened: depth,
            });
            depth = depth.saturating_add(1);
            continue;
        }

        if is_block_open_directive(inner) {
            depth = depth.saturating_add(1);
            continue;
        }

        if inner == "end" || inner.starts_with("end ") || inner.starts_with("end\t") {
            depth = depth.saturating_sub(1);
            if let Some(open) = stack.last()
                && open.depth_when_opened == depth
            {
                let opened = stack.pop().expect("stack non-empty");
                let body = src
                    .get(opened.body_start..action_match.start())
                    .unwrap_or("")
                    .to_string();
                out.push(DefineBlock {
                    name: opened.name,
                    body,
                    byte_range: opened.action_start..action_match.end(),
                });
            }
            // `else`/`else if` deliberately don't change depth — they're
            // matched by `is_block_open_directive` returning false.
        }
    }

    out
}

/// Extract every `{{ include "X" ... }}` or `{{ template "X" ... }}`
/// helper-name reference from template source text. Helper names are
/// returned in source order (with duplicates collapsed) so the caller
/// can build a per-source call graph without further dedup.
///
/// Parses each `{{ ... }}` action via the
/// [`helm_schema_ast::parse_action_expressions`] typed AST, then walks
/// the tree for `Call { function: "include" | "template", … }` nodes
/// whose first argument is a string literal. Because the AST tokenizes
/// string literals as `Literal::String` rather than raw bytes, a
/// quoted payload like `{{ "include \"X\"" | quote }}` produces a
/// `Literal::String("include \"X\"")` — never a phantom call to `X`.
/// `template_action` (`{{ template "name" . }}`) is normalized into
/// the same `Call` shape so both keyword forms surface identically.
#[must_use]
pub fn extract_helper_calls(src: &str) -> Vec<String> {
    use helm_schema_ast::{TemplateExpr, parse_action_expressions};

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for top in parse_action_expressions(src) {
        top.walk(|expr| {
            let TemplateExpr::Call { function, args } = expr else {
                return;
            };
            if function != "include" && function != "template" {
                return;
            }
            let Some(TemplateExpr::Literal(lit)) = args.first() else {
                return;
            };
            let Some(name) = lit.as_string() else {
                return;
            };
            if seen.insert(name.to_string()) {
                out.push(name.to_string());
            }
        });
    }
    out
}

fn helm_comment_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?s)\{\{-?\s*/\*.*?\*/\s*-?\}\}").expect("helm comment regex"))
}

/// Replace each `{{/* ... */}}` Helm comment in `src` with spaces of
/// the same byte length. Keeps every other byte position unchanged so
/// callers can safely use offsets/spans derived from the masked string
/// against the original `src`.
fn mask_helm_comments(src: &str) -> String {
    let re = helm_comment_regex();
    let mut out: Vec<u8> = src.as_bytes().to_vec();
    for m in re.find_iter(src) {
        for b in &mut out[m.start()..m.end()] {
            // ASCII space is one byte; comment matches are always
            // composed of bytes (no multibyte boundary issues) since
            // `{` / `}` / `/` / `*` are all single-byte.
            *b = b' ';
        }
    }
    // Safe: we only replaced ASCII bytes with ASCII space, no UTF-8
    // boundary was crossed.
    String::from_utf8(out).expect("ASCII-safe replacement")
}

fn template_action_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static R: OnceLock<Regex> = OnceLock::new();
    // `(?s)` so `.` matches newlines (multi-line actions are valid).
    // Non-greedy body so adjacent actions don't merge.
    R.get_or_init(|| Regex::new(r"(?s)\{\{-?(.*?)-?\}\}").expect("template action regex"))
}

fn parse_define_directive(inner: &str) -> Option<String> {
    let rest = inner
        .strip_prefix("define ")
        .or_else(|| inner.strip_prefix("define\t"))?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    let name = &rest[..end];
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn is_block_open_directive(inner: &str) -> bool {
    for prefix in [
        "if ", "if\t", "with ", "with\t", "range ", "range\t", "block ", "block\t",
    ] {
        if inner.starts_with(prefix) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod helper_call_tests {
    use super::extract_helper_calls;

    #[test]
    fn extracts_real_include_call() {
        let src = r#"{{ include "common.labels" . }}"#;
        assert_eq!(extract_helper_calls(src), vec!["common.labels".to_string()]);
    }

    #[test]
    fn extracts_real_template_call() {
        let src = r#"{{ template "common.labels" . }}"#;
        assert_eq!(extract_helper_calls(src), vec!["common.labels".to_string()]);
    }

    #[test]
    fn skips_helm_comment_call() {
        let src = r#"{{/* include "common.fake" */}}{{ include "common.real" . }}"#;
        assert_eq!(extract_helper_calls(src), vec!["common.real".to_string()]);
    }

    #[test]
    fn skips_call_inside_double_quoted_string() {
        // `{{ "include \"common.fake\"" | quote }}` — the OUTER quoted
        // string is a payload, not a call. Without string-literal
        // awareness the regex would produce a phantom edge to
        // `common.fake`.
        let src = r#"{{ "include \"common.fake\"" | quote }}{{ include "common.real" . }}"#;
        assert_eq!(extract_helper_calls(src), vec!["common.real".to_string()]);
    }

    #[test]
    fn skips_call_inside_backtick_raw_string() {
        let src = "{{ `include \"common.fake\"` | quote }}{{ include \"common.real\" . }}";
        assert_eq!(extract_helper_calls(src), vec!["common.real".to_string()]);
    }

    #[test]
    fn multiple_real_calls_in_one_action() {
        // Surfaced both: include followed by literal arg whose contents
        // happen to look like nothing dangerous.
        let src = r#"{{ include "a" . }}{{ include "b" . }}"#;
        assert_eq!(
            extract_helper_calls(src),
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn dedup_preserves_first_occurrence_order() {
        // Two calls to `a`, one to `b`. Output should have each name
        // once, in first-occurrence order.
        let src = r#"{{ include "a" . }}{{ include "b" . }}{{ include "a" . }}"#;
        assert_eq!(
            extract_helper_calls(src),
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn extracts_helper_inside_control_flow_body() {
        // Helper call buried inside `{{ if ... }}` body must still
        // surface — extractors are about reachable references, not
        // just top-level ones.
        let src = r#"{{ if .X }}{{ include "deep" . }}{{ end }}"#;
        assert_eq!(extract_helper_calls(src), vec!["deep".to_string()]);
    }

    #[test]
    fn extracts_helper_inside_range_destructure_header() {
        // `{{ range $i, $v := include "src" . }}` — the include call
        // lives in the range header's destructuring assignment. It
        // must be discovered because the helper IS executed at
        // render time.
        let src = r#"{{ range $i, $v := include "src" . }}{{ end }}"#;
        assert_eq!(extract_helper_calls(src), vec!["src".to_string()]);
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
