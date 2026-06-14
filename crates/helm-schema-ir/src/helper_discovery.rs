use regex::Regex;

/// A `{{ define "name" }} ... {{ end }}` block extracted from template
/// source, with the body text and its byte span in the original source.
/// `body` excludes the surrounding `{{ define }}` / `{{ end }}` actions
/// themselves; it's only the rendered content between them.
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
/// Defines that never close are silently dropped, preferring incomplete
/// coverage over panics on malformed templates.
#[must_use]
pub fn extract_define_blocks(src: &str) -> Vec<DefineBlock> {
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
            if let Some(opened) = stack.pop_if(|open| open.depth_when_opened == depth) {
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
        }
    }

    out
}

/// Extract every `{{ include "X" ... }}` or `{{ template "X" ... }}`
/// helper-name reference from template source text. Helper names are returned
/// in source order with duplicates collapsed.
///
/// Parses each action via [`helm_schema_ast::parse_action_expressions`] and
/// walks the typed tree for `Call { function: "include" | "template", ... }`
/// nodes whose first argument is a string literal. Because string literals are
/// typed AST nodes, quoted payloads that contain bytes like `include "X"` do
/// not create phantom helper edges.
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
        for byte in &mut out[m.start()..m.end()] {
            *byte = b' ';
        }
    }
    // Replacing bytes with ASCII spaces preserves the original UTF-8 validity.
    String::from_utf8(out).expect("ASCII-safe replacement")
}

fn template_action_regex() -> &'static Regex {
    use std::sync::OnceLock;
    static R: OnceLock<Regex> = OnceLock::new();
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
mod tests {
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
        let src = r#"{{ include "a" . }}{{ include "b" . }}"#;
        assert_eq!(
            extract_helper_calls(src),
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn dedup_preserves_first_occurrence_order() {
        let src = r#"{{ include "a" . }}{{ include "b" . }}{{ include "a" . }}"#;
        assert_eq!(
            extract_helper_calls(src),
            vec!["a".to_string(), "b".to_string()],
        );
    }

    #[test]
    fn extracts_helper_inside_control_flow_body() {
        let src = r#"{{ if .X }}{{ include "deep" . }}{{ end }}"#;
        assert_eq!(extract_helper_calls(src), vec!["deep".to_string()]);
    }

    #[test]
    fn extracts_helper_inside_range_destructure_header() {
        let src = r#"{{ range $i, $v := include "src" . }}{{ end }}"#;
        assert_eq!(extract_helper_calls(src), vec!["src".to_string()]);
    }
}
