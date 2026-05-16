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

fn classify_default_literal(lit: &str) -> Option<DefaultLiteralType> {
    let lit = lit.trim();
    if lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"') {
        return Some(DefaultLiteralType::String);
    }
    if lit == "true" || lit == "false" {
        return Some(DefaultLiteralType::Boolean);
    }
    if lit.parse::<i64>().is_ok() || lit.parse::<u64>().is_ok() {
        return Some(DefaultLiteralType::Integer);
    }
    if lit.parse::<f64>().is_ok() {
        return Some(DefaultLiteralType::Number);
    }
    None
}

/// Extract type hints implied by `default <literal> .Values.X` and
/// `.Values.X | default <literal>` patterns in template text.
///
/// Returns pairs of `(values_path, schema_fragment)` where `schema_fragment`
/// reflects the literal's JSON type (string/integer/number/boolean). Patterns
/// where the first argument to `default` is a non-literal (function call,
/// computed expression) are skipped.
///
/// Pre-filtering before regex match:
/// 1. Lines that are YAML comments (start with `#`) are dropped — the
///    template engine never executes them, so any `default ...` syntax
///    they contain is descriptive text.
/// 2. Helm comments `{{/* ... */}}` are dropped — same reason.
/// 3. Go string literals inside template actions are replaced with `""`,
///    so a `default 5 .Values.x` substring living inside a Go string
///    doesn't produce a hint, while `default "fallback" .Values.x` (where
///    `"fallback"` is the literal arg) still classifies correctly as a
///    string literal.
#[must_use]
pub fn extract_default_type_hints(text: &str) -> Vec<(String, Value)> {
    let cleaned = preprocess_for_hint_extraction(text);
    static_re().captures_iter_for(&cleaned)
}

pub(crate) fn preprocess_for_hint_extraction(src: &str) -> String {
    let no_yaml_comments: String = src
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");

    let helm_comment_re = helm_comment_regex();
    let no_helm_comments = helm_comment_re.replace_all(&no_yaml_comments, " ");

    let string_lit_re = go_string_literal_regex();
    string_lit_re
        .replace_all(&no_helm_comments, "\"\"")
        .into_owned()
}

fn helm_comment_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"(?s)\{\{-?\s*/\*.*?\*/\s*-?\}\}").expect("helm comment regex"))
}

fn go_string_literal_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static R: OnceLock<Regex> = OnceLock::new();
    // Match string literals containing `default` or `.Values.` — these are
    // the ones that would mislead the regex. Leave other strings (including
    // legitimate `default "fallback"` literal args) alone so they continue
    // to classify as String.
    R.get_or_init(|| {
        Regex::new(r#""[^"\n]*(?:default|\.Values\.)[^"\n]*""#).expect("go string literal regex")
    })
}

/// Holds compiled regexes; one set is sufficient for the whole process.
struct DefaultHintRegexes {
    prefix: Regex,
    pipeline: Regex,
}

fn static_re() -> &'static DefaultHintRegexes {
    use std::sync::OnceLock;
    static R: OnceLock<DefaultHintRegexes> = OnceLock::new();
    R.get_or_init(|| {
        // Literal: quoted string, signed integer/float, or `true`/`false`.
        let lit = r#"(?:"[^"]*"|-?\d+(?:\.\d+)?|true|false)"#;
        let path = r#"[\w]+(?:\.[\w]+)*"#;
        // Optional `$` or `$varname` prefix on `.Values.`. Covers the
        // common rooted forms used inside `range`/`with` bodies where `.`
        // has been rebound:
        //   `.Values.X`            (root context)
        //   `$.Values.X`           (`$` is the root scope)
        //   `$root.Values.X`       (chart-defined `$root := .` alias)
        let values_prefix = r"(?:\$\w*)?\.Values\.";
        DefaultHintRegexes {
            // `default LIT .Values.PATH` (or rooted variants)
            prefix: Regex::new(&format!(
                r"\bdefault\s+(?P<lit>{lit})\s+{values_prefix}(?P<path>{path})"
            ))
            .expect("default-prefix regex"),
            // `.Values.PATH | default LIT` (or rooted variants)
            pipeline: Regex::new(&format!(
                r"{values_prefix}(?P<path>{path})\s*\|\s*default\s+(?P<lit>{lit})"
            ))
            .expect("default-pipeline regex"),
        }
    })
}

impl DefaultHintRegexes {
    fn captures_iter_for(&self, text: &str) -> Vec<(String, Value)> {
        let mut out: Vec<(String, Value)> = Vec::new();
        for caps in self.prefix.captures_iter(text) {
            if let Some(ty) = classify_default_literal(&caps["lit"]) {
                out.push((caps["path"].to_string(), ty.schema()));
            }
        }
        for caps in self.pipeline.captures_iter(text) {
            if let Some(ty) = classify_default_literal(&caps["lit"]) {
                out.push((caps["path"].to_string(), ty.schema()));
            }
        }
        out
    }
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
/// Comment text is skipped so `{{/* include "X" */}}` doesn't produce
/// a false edge.
///
/// Go string literals inside template actions are also skipped via
/// regex alternation, so a quoted payload like
/// `{{ "include \"X\"" | quote }}` doesn't produce a false edge. The
/// alternation engine consumes the string span before the helper-call
/// alternative gets a chance to match the bytes inside it. Action
/// interiors are raw text in the parser AST (`HelmAst::HelmExpr`), so
/// without this we'd let phantom calls flow into the cross-chart
/// helper call graph and contaminate downstream type-hint extraction.
#[must_use]
pub fn extract_helper_calls(src: &str) -> Vec<String> {
    let preserved = mask_helm_comments(src);
    let action_re = template_action_regex();
    let call_re = helper_call_regex();

    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for cap in action_re.captures_iter(&preserved) {
        let inner = cap.get(1).expect("re capture group 1").as_str();
        for call in call_re.captures_iter(inner) {
            // The combined regex has three alternations: double-quoted
            // string, backtick raw string, and the actual helper call.
            // Only the helper-call branch has a `name` capture group.
            // The other branches match (so the engine consumes those
            // spans and skips past them) but produce no name.
            let Some(name) = call.name("name") else {
                continue;
            };
            let name = name.as_str().to_string();
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
    }
    out
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

fn helper_call_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static R: OnceLock<Regex> = OnceLock::new();
    // Three alternations, tried left-to-right at each position:
    //
    //   1. A double-quoted Go string literal (with `\"`/`\\` escapes).
    //   2. A backtick-quoted Go raw string literal.
    //   3. The actual helper-call pattern `include "name"` /
    //      `template "name"`, captured into the `name` group.
    //
    // The regex engine's leftmost-first matching means a string-literal
    // span at position p is consumed before alternation 3 ever gets a
    // chance to scan the bytes inside it. So `"include \"X\""` inside
    // a `{{ ... }}` action no longer produces a phantom call to `X` —
    // the outer quoted string is matched and skipped first.
    //
    // Helper-define names allow `.`, `_`, `-`, alphanumerics.
    R.get_or_init(|| {
        Regex::new(concat!(
            r#""(?:[^"\\]|\\.)*""#,
            r"|`[^`]*`",
            r#"|\b(?:include|template)\s+"(?P<name>[A-Za-z0-9._-]+)""#,
        ))
        .expect("helper call regex")
    })
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
}
