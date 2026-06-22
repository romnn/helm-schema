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
#[path = "tests/helper_discovery.rs"]
mod tests;
