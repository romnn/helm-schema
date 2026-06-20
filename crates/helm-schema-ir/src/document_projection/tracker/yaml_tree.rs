pub(super) fn parse_yaml_tree(source: &str) -> Option<tree_sitter::Tree> {
    let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
}

pub(super) fn is_scalar_like(kind: &str) -> bool {
    matches!(
        kind,
        "plain_scalar"
            | "string_scalar"
            | "double_quote_scalar"
            | "single_quote_scalar"
            | "integer_scalar"
            | "float_scalar"
            | "boolean_scalar"
            | "null_scalar"
    )
}

pub(super) fn scalar_text(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    match node.kind() {
        "block_node" | "flow_node" => node
            .named_child(0)
            .and_then(|child| scalar_text(child, source)),
        kind if is_scalar_like(kind) => {
            let text = node.utf8_text(source.as_bytes()).ok()?.trim();
            Some(strip_scalar_quotes(text).to_string())
        }
        _ => None,
    }
}

pub(super) fn strip_scalar_quotes(text: &str) -> &str {
    if text.len() >= 2
        && ((text.starts_with('"') && text.ends_with('"'))
            || (text.starts_with('\'') && text.ends_with('\'')))
    {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

pub(super) fn unwrap_yaml_node(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut current = node;
    loop {
        match current.kind() {
            "block_node" | "flow_node" => {
                let Some(child) = current.named_child(0) else {
                    return current;
                };
                current = child;
            }
            _ => return current,
        }
    }
}
