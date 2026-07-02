use crate::{
    TemplateExpr, TemplateHeader, children_with_field, first_mapping_colon_offset, parse_expr_text,
    parse_yaml_key,
};

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

pub fn range_body_emits_sequence_item_from_source(
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

pub fn range_body_renders_mapping_entries_from_ast(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    let Some(entry_indent) = range_body_mapping_entry_indent_from_source(node, source) else {
        return false;
    };
    range_body_min_content_indent(node, source) == Some(entry_indent)
}

pub fn range_body_mapping_entry_indent_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> Option<usize> {
    let key_variable = range_destructured_key_variable(node, source)?;
    let body_text = range_body_text(node, source);

    for line in body_text.lines() {
        let indent = line.chars().take_while(|&ch| ch == ' ').count();
        let after = &line[indent..];
        if mapping_key_text_refs_range_key_variable(after, &key_variable) {
            return Some(indent);
        }
    }
    None
}

pub fn mapping_key_text_refs_range_key_variable(text: &str, key_variable: &str) -> bool {
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

pub fn range_body_renders_scalar_sequence_items_from_source(
    node: tree_sitter::Node<'_>,
    source: &str,
) -> bool {
    let mut saw_sequence_item = false;
    let body_text = range_body_text(node, source);

    for line in body_text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix('-') else {
            continue;
        };
        let rest = rest.trim_start();
        saw_sequence_item = true;

        let renders_fragment = rest.starts_with("{{")
            && parse_expr_text(rest)
                .iter()
                .any(TemplateExpr::renders_yaml_fragment);
        if rest.is_empty() || parse_yaml_key(rest).is_some() || renders_fragment {
            return false;
        }
    }

    saw_sequence_item
}

pub fn range_has_destructured_variable_definition(node: tree_sitter::Node<'_>) -> bool {
    destructured_range_variables(node).len() >= 2
}

fn range_destructured_key_variable(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let variables = destructured_range_variables(node);
    if variables.len() < 2 {
        return None;
    }
    variables
        .first()?
        .utf8_text(source.as_bytes())
        .ok()
        .map(|text| text.trim_start_matches('$').to_string())
}

/// The `$key, $value` variable nodes of a `range $key, $value := …`
/// destructuring header, in source order. Empty when the range has no
/// `range_variable_definition` child.
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

/// Concatenated source text of every `body` child of a range/control
/// node. Body children are contiguous source spans, so this reproduces
/// the body's exact source text for line-based scanning.
fn range_body_text(node: tree_sitter::Node<'_>, source: &str) -> String {
    let mut body_text = String::new();
    for body_node in children_with_field(node, "body") {
        if let Ok(text) = body_node.utf8_text(source.as_bytes()) {
            body_text.push_str(text);
        }
    }
    body_text
}

fn range_body_min_content_indent(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    let mut min_indent = None;
    let body_text = range_body_text(node, source);

    for line in body_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("{{") && first_mapping_colon_offset(trimmed).is_none() {
            continue;
        }
        let indent = line.chars().take_while(|&ch| ch == ' ').count();
        min_indent = Some(min_indent.map_or(indent, |current: usize| current.min(indent)));
    }
    min_indent
}
