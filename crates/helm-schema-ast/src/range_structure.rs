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

pub fn range_header_text_from_source(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
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

fn range_body_min_content_indent(node: tree_sitter::Node<'_>, source: &str) -> Option<usize> {
    let mut min_indent = None;
    let mut body_text = String::new();
    for body_node in children_with_field(node, "body") {
        let Ok(text) = body_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        body_text.push_str(text);
    }

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
