use crate::{TemplateExpr, TemplateHeader};

/// Returns the variable bound by a single-variable range expression.
#[must_use]
pub fn range_variable_name_expr(expr: &TemplateExpr) -> Option<String> {
    let (TemplateExpr::VariableDefinition { name, .. } | TemplateExpr::Assignment { name, .. }) =
        expr.deparen()
    else {
        return None;
    };
    Some(name.trim_start_matches('$').to_string())
}

/// The typed header of a range node's iterated clause.
#[must_use]
pub fn range_header_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<TemplateHeader> {
    let range = node.child_by_field_name("range").or_else(|| {
        let mut walker = node.walk();
        node.named_children(&mut walker)
            .filter(|child| child.kind() == "range_variable_definition")
            .find_map(|child| child.child_by_field_name("range"))
    })?;
    Some(TemplateHeader::from_node(range, source))
}

/// Reports whether a range header binds separate key and value variables.
#[must_use]
pub fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
    destructured_range_variables(node).len() >= 2
}

/// Reports whether a range header assigns existing variables with `=`.
#[must_use]
pub fn range_uses_assignment(node: tree_sitter::Node<'_>) -> bool {
    let mut walker = node.walk();
    let mut children = node.named_children(&mut walker);
    let Some(definition) =
        children.find(|child| matches!(child.kind(), "assignment" | "range_variable_definition"))
    else {
        return false;
    };
    if definition.kind() == "assignment" {
        return true;
    }
    let mut definition_walker = definition.walk();
    definition
        .children(&mut definition_walker)
        .any(|child| child.kind() == "=")
}

/// The KEY variable of a destructured range header (`$k` in
/// `range $k, $v := …`).
#[must_use]
pub fn range_destructured_key_variable(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<String> {
    let variables = destructured_range_variables(node);
    if variables.len() < 2 {
        return None;
    }
    variables
        .first()
        .and_then(|variable| variable.utf8_text(source.as_bytes()).ok())
        .map(|name| name.trim().trim_start_matches('$').to_string())
}

/// Returns the value variable of a destructured range header.
///
/// `range $k, $v := …` binds each member's value to `$v`.
#[must_use]
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
        .find(|child| matches!(child.kind(), "assignment" | "range_variable_definition"))
    else {
        return Vec::new();
    };
    let mut definition_walker = definition.walk();
    definition
        .children(&mut definition_walker)
        .take_while(|child| !matches!(child.kind(), ":=" | "="))
        .filter(|child| child.kind() == "variable")
        .collect()
}
