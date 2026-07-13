use crate::{TemplateExpr, TemplateHeader};

pub fn range_variable_name_expr(expr: &TemplateExpr) -> Option<String> {
    let TemplateExpr::VariableDefinition { name, .. } = expr.deparen() else {
        return None;
    };
    Some(name.trim_start_matches('$').to_string())
}

fn range_header_text_from_source(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let range = node.child_by_field_name("range").or_else(|| {
        let mut walker = node.walk();
        node.named_children(&mut walker)
            .filter(|child| child.kind() == "range_variable_definition")
            .find_map(|child| child.child_by_field_name("range"))
    })?;
    range
        .utf8_text(source.as_bytes())
        .ok()
        .map(|text| text.trim().to_string())
}

pub fn range_header_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<TemplateHeader> {
    range_header_text_from_source(node, source).map(TemplateHeader::parse_range)
}

pub fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
    destructured_range_variables(node).len() >= 2
}

/// The VALUE variable of a destructured range header
/// (`range $k, $v := …` binds each member's value to `$v`).
pub fn range_destructured_value_variable(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<String> {
    let variables = destructured_range_variables(node);
    if variables.len() < 2 {
        return None;
    }
    variables
        .last()
        .and_then(|variable| variable.utf8_text(source.as_bytes()).ok())
        .map(|name| name.trim().trim_start_matches('$').to_string())
}

fn destructured_range_variables(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut walker = node.walk();
    let Some(definition) = node
        .named_children(&mut walker)
        .find(|child| child.kind() == "range_variable_definition")
    else {
        return Vec::new();
    };
    let mut definition_walker = definition.walk();
    definition
        .named_children(&mut definition_walker)
        .filter(|child| child.kind() == "variable")
        .collect()
}
