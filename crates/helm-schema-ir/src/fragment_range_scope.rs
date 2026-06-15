use std::collections::{HashMap, HashSet};

use helm_schema_ast::{HelmAst, HelmParser as _, TemplateExpr, TreeSitterParser};

use crate::fragment_binding::FragmentBinding;
use crate::fragment_classification::is_fragment_expr;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::template_expr_cache::parse_expr_text;
use crate::tree_sitter_utils::children_with_field;
use crate::yaml_shape::parse_yaml_key;

pub(crate) fn range_variable_item_binding(
    header: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<(String, FragmentBinding)> {
    let header = header
        .trim()
        .strip_prefix("range ")
        .unwrap_or_else(|| header.trim());
    let exprs = parse_expr_text(header);
    let [TemplateExpr::VariableDefinition { name, value }] = exprs.as_slice() else {
        return None;
    };
    let binding =
        fragment_binding_from_range_value_expr(value, local_bindings, current_dot, context, seen)?;
    let item = FragmentBinding::item_binding(&binding)?;
    Some((name.trim_start_matches('$').to_string(), item))
}

pub(crate) fn range_variable_name(header: &str) -> Option<String> {
    let header = header
        .trim()
        .strip_prefix("range ")
        .unwrap_or_else(|| header.trim());
    let exprs = parse_expr_text(header);
    let [TemplateExpr::VariableDefinition { name, .. }] = exprs.as_slice() else {
        return None;
    };
    Some(name.trim_start_matches('$').to_string())
}

pub(crate) fn range_iterable_binding(
    header: &str,
    local_bindings: &HashMap<String, FragmentBinding>,
    current_dot: Option<&FragmentBinding>,
    context: FragmentEvalContext<'_>,
    seen: &mut HashSet<String>,
) -> Option<FragmentBinding> {
    let header = header
        .trim()
        .strip_prefix("range ")
        .unwrap_or_else(|| header.trim());
    let exprs = parse_expr_text(header);
    let value = match exprs.as_slice() {
        [TemplateExpr::VariableDefinition { value, .. }]
        | [TemplateExpr::Assignment { value, .. }] => value.as_ref(),
        [expr] => expr,
        _ => return None,
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
    let text = match key {
        HelmAst::HelmExpr { text } | HelmAst::Scalar { text } => text,
        _ => return false,
    };
    parse_expr_text(text).iter().any(|expr| {
        let mut matches_key_variable = false;
        expr.walk(|node| {
            if matches!(node, TemplateExpr::Variable(name) if name == key_variable) {
                matches_key_variable = true;
            }
        });
        matches_key_variable
    })
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

        if rest.is_empty() || parse_yaml_key(rest).is_some() || is_fragment_expr(rest) {
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
