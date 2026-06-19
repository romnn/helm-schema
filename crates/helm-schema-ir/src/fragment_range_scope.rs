use std::collections::{HashMap, HashSet};

use helm_schema_ast::{HelmAst, HelmParser as _, TemplateExpr, TemplateHeader, TreeSitterParser};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_classification::is_fragment_exprs;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::template_expr_cache::parse_expr_text;
use crate::tree_sitter_utils::children_with_field;
use crate::yaml_syntax::{first_mapping_colon_offset, parse_yaml_key};

pub(crate) fn range_variable_name_expr(expr: &TemplateExpr) -> Option<String> {
    let TemplateExpr::VariableDefinition { name, .. } = expr.deparen() else {
        return None;
    };
    Some(name.trim_start_matches('$').to_string())
}

pub(crate) fn range_iterable_binding_expr(
    expr: &TemplateExpr,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let value = match expr.deparen() {
        TemplateExpr::VariableDefinition { value, .. } | TemplateExpr::Assignment { value, .. } => {
            value.as_ref()
        }
        expr => expr,
    };
    fragment_binding_from_range_value_expr(value, local_bindings, current_dot, context, seen)
}

fn fragment_binding_from_range_value_expr(
    value: &TemplateExpr,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    context.fragment_binding_from_expr(value, local_bindings, current_dot, seen)
}

pub(crate) fn range_header_text_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<String> {
    if let Some(range) = node.child_by_field_name("range") {
        return range
            .utf8_text(source.as_bytes())
            .ok()
            .map(|text| text.trim().to_string());
    }
    let mut walker = node.walk();
    for child in node.named_children(&mut walker) {
        if child.kind() == "range_variable_definition"
            && let Some(range) = child.child_by_field_name("range")
        {
            return range
                .utf8_text(source.as_bytes())
                .ok()
                .map(|text| text.trim().to_string());
        }
    }
    None
}

pub(crate) fn range_header_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<TemplateHeader> {
    range_header_text_from_source(node, source).map(TemplateHeader::parse_range)
}

pub(crate) fn range_body_emits_sequence_item_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("- ") || trimmed == "-" {
                return true;
            }
        }
    }
    false
}

pub(crate) fn range_body_renders_mapping_entries_from_ast(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    let Some(key_variable) = range_destructured_key_variable(node, source) else {
        return false;
    };
    let mut body_text = String::new();
    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        body_text.push_str(text);
    }

    let Ok(ast) = TreeSitterParser.parse(&body_text) else {
        return false;
    };
    ast_directly_renders_templated_mapping_key(&ast, &key_variable)
}

pub(crate) fn range_body_mapping_entry_indent_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<usize> {
    let key_variable = range_destructured_key_variable(node, source)?;
    let mut body_text = String::new();
    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        body_text.push_str(text);
    }

    for line in body_text.lines() {
        let indent = line.chars().take_while(|&ch| ch == ' ').count();
        let after = &line[indent..];
        if mapping_key_text_refs_range_key_variable(after, &key_variable) {
            return Some(indent);
        }
    }
    None
}

fn mapping_key_text_refs_range_key_variable(text: &str, key_variable: &str) -> bool {
    let Some(colon_offset) = first_mapping_colon_offset(text) else {
        return false;
    };
    let key_text = &text[..colon_offset];
    parse_expr_text(key_text)
        .iter()
        .any(|expr| expr_refs_range_key_variable(expr, key_variable))
}

fn expr_refs_range_key_variable(expr: &TemplateExpr, key_variable: &str) -> bool {
    let mut refs_variable = false;
    expr.walk(|node| {
        if matches!(
            node,
            TemplateExpr::Variable(name)
                if name == key_variable || name.trim_start_matches('$') == key_variable
        ) {
            refs_variable = true;
        }
    });
    refs_variable
}

fn ast_directly_renders_templated_mapping_key(ast: &HelmAst, key_variable: &str) -> bool {
    match ast {
        HelmAst::Pair { key, .. } => ast_key_refs_range_key_variable(key, key_variable),
        HelmAst::Document { items } | HelmAst::Mapping { items } => items
            .iter()
            .any(|item| ast_directly_renders_templated_mapping_key(item, key_variable)),
        HelmAst::Sequence { .. } => false,
        HelmAst::If {
            then_branch,
            else_branch,
            ..
        } => then_branch
            .iter()
            .chain(else_branch)
            .any(|item| ast_directly_renders_templated_mapping_key(item, key_variable)),
        HelmAst::With {
            body, else_branch, ..
        } => body
            .iter()
            .chain(else_branch)
            .any(|item| ast_directly_renders_templated_mapping_key(item, key_variable)),
        HelmAst::Range { .. } => false,
        HelmAst::Block { body, .. } | HelmAst::Define { body, .. } => body
            .iter()
            .any(|item| ast_directly_renders_templated_mapping_key(item, key_variable)),
        HelmAst::Scalar { .. } | HelmAst::HelmExpr { .. } | HelmAst::HelmComment { .. } => false,
    }
}

fn ast_key_refs_range_key_variable(key: &HelmAst, key_variable: &str) -> bool {
    match key {
        HelmAst::HelmExpr { action } => action
            .exprs()
            .iter()
            .any(|expr| expr_refs_range_key_variable(expr, key_variable)),
        HelmAst::Scalar { text } => parse_expr_text(text)
            .iter()
            .any(|expr| expr_refs_range_key_variable(expr, key_variable)),
        _ => false,
    }
}

pub(crate) fn range_body_renders_scalar_sequence_items_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    let mut saw_sequence_item = false;
    let mut body_text = String::new();

    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        body_text.push_str(text);
    }

    for line in body_text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('-') else {
            continue;
        };
        let rest = rest.trim_start();
        saw_sequence_item = true;

        let renders_fragment = rest.starts_with("{{") && is_fragment_exprs(&parse_expr_text(rest));
        if rest.is_empty() || parse_yaml_key(rest).is_some() || renders_fragment {
            return false;
        }
    }

    saw_sequence_item
}

pub(crate) fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
    let mut walker = node.walk();
    node.named_children(&mut walker)
        .find(|child| child.kind() == "range_variable_definition")
        .is_some_and(|definition| {
            let mut definition_walker = definition.walk();
            definition
                .named_children(&mut definition_walker)
                .filter(|child| child.kind() == "variable")
                .count()
                >= 2
        })
}

fn range_destructured_key_variable(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut walker = node.walk();
    let definition = node
        .named_children(&mut walker)
        .find(|child| child.kind() == "range_variable_definition")?;
    let mut definition_walker = definition.walk();
    let mut variables = definition
        .named_children(&mut definition_walker)
        .filter(|child| child.kind() == "variable");
    let first = variables.next()?;
    variables.next()?;
    first
        .utf8_text(source.as_bytes())
        .ok()
        .map(|text| text.trim_start_matches('$').to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        mapping_key_text_refs_range_key_variable, range_body_mapping_entry_indent_from_source,
    };

    #[test]
    fn templated_mapping_key_text_refs_range_key_variable() {
        assert!(mapping_key_text_refs_range_key_variable(
            "{{- $key | nindent 2 }}: {{ tpl (toString $value) $ | quote }}",
            "key",
        ));
    }

    #[test]
    fn destructured_range_mapping_entry_indent_uses_body_key_indent() {
        let source = r#"
data:
{{- range $key, $value := .Values.controller.config }}
  {{- $key | nindent 2 }}: {{ tpl (toString $value) $ | quote }}
{{- end }}
        "#;
        let tree = parse_go_template(source);
        let range = find_kind(tree.root_node(), "range_action").expect("range action");

        assert_eq!(
            range_body_mapping_entry_indent_from_source(range, source),
            Some(2)
        );
    }

    fn parse_go_template(source: &str) -> tree_sitter::Tree {
        let language =
            tree_sitter::Language::new(helm_schema_template_grammar::go_template::language());
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .expect("set go template language");
        parser.parse(source, None).expect("parse go template")
    }

    fn find_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = find_kind(child, kind) {
                return Some(found);
            }
        }
        None
    }
}
