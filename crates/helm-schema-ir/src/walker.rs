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
pub fn extract_values_paths(text: &str) -> Vec<String> {
    let re = Regex::new(r"\.Values\.([\w]+(?:\.(?:[\w]+|\*))*)").unwrap();
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
