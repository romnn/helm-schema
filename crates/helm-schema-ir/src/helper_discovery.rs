/// A `{{ define "name" }} ... {{ end }}` block extracted from template
/// source, with the body text and its byte span in the original source.
/// `body` excludes the surrounding `{{ define }}` / `{{ end }}` actions
/// themselves; it's only the rendered content between them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefineBlock {
    pub name: String,
    pub body: String,
    pub byte_range: std::ops::Range<usize>,
    pub body_range: std::ops::Range<usize>,
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
    let Some(tree) = parse_go_template(src) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    collect_define_blocks(tree.root_node(), src, &mut out);
    out.sort_by_key(|block| block.byte_range.start);
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

#[must_use]
pub fn extract_helper_calls_from_ast(ast: &helm_schema_ast::HelmAst) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    collect_helper_calls_in_ast(ast, true, &mut seen, &mut out);
    out
}

#[must_use]
pub fn extract_helper_calls_from_ast_body(nodes: &[helm_schema_ast::HelmAst]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for node in nodes {
        collect_helper_calls_in_ast(node, true, &mut seen, &mut out);
    }
    out
}

#[must_use]
pub fn extract_helper_calls_from_ast_excluding_defines(
    ast: &helm_schema_ast::HelmAst,
) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<String> = Vec::new();
    collect_helper_calls_in_ast(ast, false, &mut seen, &mut out);
    out
}

fn collect_helper_calls_in_ast(
    node: &helm_schema_ast::HelmAst,
    descend_into_defines: bool,
    seen: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    use helm_schema_ast::{HelmAst, TemplateExpr};

    let mut collect_from_expr = |top: &TemplateExpr| {
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
    };

    if !descend_into_defines && matches!(node, HelmAst::Define { .. }) {
        return;
    }

    match node {
        HelmAst::Document { items } | HelmAst::Mapping { items } | HelmAst::Sequence { items } => {
            for item in items {
                collect_helper_calls_in_ast(item, descend_into_defines, seen, out);
            }
        }
        HelmAst::Pair { key, value } => {
            collect_helper_calls_in_ast(key, descend_into_defines, seen, out);
            if let Some(value) = value.as_deref() {
                collect_helper_calls_in_ast(value, descend_into_defines, seen, out);
            }
        }
        HelmAst::If {
            condition,
            then_branch,
            else_branch,
        } => {
            collect_from_expr(condition.expr());
            for item in then_branch {
                collect_helper_calls_in_ast(item, descend_into_defines, seen, out);
            }
            for item in else_branch {
                collect_helper_calls_in_ast(item, descend_into_defines, seen, out);
            }
        }
        HelmAst::Range {
            header,
            body,
            else_branch,
            ..
        }
        | HelmAst::With {
            header,
            body,
            else_branch,
            ..
        } => {
            collect_from_expr(header.expr());
            for item in body {
                collect_helper_calls_in_ast(item, descend_into_defines, seen, out);
            }
            for item in else_branch {
                collect_helper_calls_in_ast(item, descend_into_defines, seen, out);
            }
        }
        HelmAst::Define { body, .. } | HelmAst::Block { body, .. } => {
            for item in body {
                collect_helper_calls_in_ast(item, descend_into_defines, seen, out);
            }
        }
        HelmAst::HelmExpr { action } => {
            for expr in action.exprs() {
                collect_from_expr(expr);
            }
        }
        HelmAst::Scalar { text } => {
            for expr in helm_schema_ast::parse_action_expressions(text) {
                collect_from_expr(&expr);
            }
        }
        HelmAst::HelmComment { .. } => {}
    }
}

fn collect_define_blocks(node: tree_sitter::Node<'_>, src: &str, out: &mut Vec<DefineBlock>) {
    if node.kind() == "define_action"
        && let Some(block) = define_block_from_node(node, src)
    {
        out.push(block);
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_define_blocks(child, src, out);
    }
}

fn define_block_from_node(node: tree_sitter::Node<'_>, src: &str) -> Option<DefineBlock> {
    let name = define_name(node, src)?;
    let body_children = children_with_field(node, "body");
    let end_action_start = find_end_action_start(node);

    let body_end = end_action_start.unwrap_or_else(|| {
        body_children
            .last()
            .map(tree_sitter::Node::end_byte)
            .unwrap_or_else(|| node.end_byte())
    });
    let body_start = body_children
        .first()
        .map(tree_sitter::Node::start_byte)
        .unwrap_or(body_end);
    let body_range = body_start..body_end;
    let body = src.get(body_range.clone())?.to_string();

    Some(DefineBlock {
        name,
        body,
        byte_range: node.start_byte()..node.end_byte(),
        body_range,
    })
}

fn define_name(node: tree_sitter::Node<'_>, src: &str) -> Option<String> {
    let raw = node
        .child_by_field_name("name")?
        .utf8_text(src.as_bytes())
        .ok()?
        .trim();
    let quoted = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('`')
                .and_then(|rest| rest.strip_suffix('`'))
        })
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(raw)
        .trim();
    if quoted.is_empty() {
        return None;
    }
    Some(quoted.to_string())
}

fn children_with_field<'a>(node: tree_sitter::Node<'a>, field: &str) -> Vec<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor).collect()
}

fn find_end_action_start(node: tree_sitter::Node<'_>) -> Option<usize> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == "end_action")
        .map(|child| child.start_byte())
}

fn parse_go_template(src: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return None;
    }
    parser.parse(src, None)
}

#[cfg(test)]
mod tests {
    use super::{
        extract_define_blocks, extract_helper_calls, extract_helper_calls_from_ast_body,
        extract_helper_calls_from_ast_excluding_defines,
    };
    use helm_schema_ast::{HelmParser, TreeSitterParser};

    #[test]
    fn extracts_define_blocks_with_exact_body_spans() {
        let src = indoc::indoc! {r#"
            {{- define "outer" -}}
            before
            {{- define "inner" -}}
            inside
            {{- end -}}
            after
            {{- end -}}
        "#};

        let blocks = extract_define_blocks(src);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].name, "outer");
        assert_eq!(blocks[1].name, "inner");
        assert_eq!(&src[blocks[0].body_range.clone()], blocks[0].body);
        assert_eq!(&src[blocks[1].body_range.clone()], blocks[1].body);
        assert!(blocks[0].body.contains("before"));
        assert!(blocks[0].body.contains("after"));
        assert!(blocks[0].body.contains(r#"{{- define "inner" -}}"#));
        assert_eq!(blocks[1].body.trim(), "inside");
    }

    #[test]
    fn extracts_define_blocks_without_comment_masking_heuristics() {
        let src = indoc::indoc! {r#"
            {{- define "x" -}}
            {{/* {{ end }} should not terminate the define */}}
            value
            {{- end -}}
        "#};

        let blocks = extract_define_blocks(src);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].name, "x");
        assert!(blocks[0].body.contains("should not terminate"));
        assert_eq!(
            &src[blocks[0].byte_range.clone()],
            src.get(blocks[0].byte_range.clone()).unwrap_or("")
        );
    }

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

    #[test]
    fn ast_extraction_can_skip_define_bodies_for_chart_direct_calls() {
        let src = indoc::indoc! {r#"
            {{ include "direct" . }}
            {{- define "helper" -}}
            {{ include "nested" . }}
            {{- end -}}
        "#};
        let ast = TreeSitterParser.parse(src).expect("parse");

        assert_eq!(
            extract_helper_calls_from_ast_excluding_defines(&ast),
            vec!["direct".to_string()]
        );
    }

    #[test]
    fn ast_body_extraction_visits_helper_body_headers_and_actions() {
        let src = indoc::indoc! {r#"
            {{- define "helper" -}}
            {{- if include "guard" . -}}
            {{ include "body" . }}
            {{- end -}}
            {{- end -}}
        "#};
        let ast = TreeSitterParser.parse(src).expect("parse");
        let helm_schema_ast::HelmAst::Document { items } = ast else {
            panic!("expected document root");
        };
        let [helm_schema_ast::HelmAst::Define { body, .. }] = items.as_slice() else {
            panic!("expected one define");
        };

        assert_eq!(
            extract_helper_calls_from_ast_body(body),
            vec!["guard".to_string(), "body".to_string()]
        );
    }

    #[test]
    fn ast_extraction_finds_helper_calls_embedded_inside_scalar_text() {
        let src = indoc::indoc! {r#"
            apiVersion: v1
            kind: ServiceAccount
            metadata:
              name: {{ include "helper.name" . }}-suffix
        "#};
        let ast = TreeSitterParser.parse(src).expect("parse");

        assert_eq!(
            extract_helper_calls_from_ast_excluding_defines(&ast),
            vec!["helper.name".to_string()]
        );
    }
}
