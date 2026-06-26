use crate::{ParseError, parse_go_template};

/// Return whether the source contains any Helm/Go-template action.
///
/// This is a syntax-level check over the template grammar. Callers that only
/// accept literal YAML can use it to abstain before handing source text to a
/// YAML parser.
#[tracing::instrument(skip_all, fields(bytes = src.len()))]
pub fn contains_template_action(src: &str) -> Result<bool, ParseError> {
    let tree = parse_go_template(src).ok_or(ParseError::TreeSitterParseFailed)?;
    Ok(node_contains_template_action(tree.root_node()))
}

fn node_contains_template_action(node: tree_sitter::Node<'_>) -> bool {
    if is_template_action_node(node.kind()) {
        return true;
    }

    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(node_contains_template_action)
}

fn is_template_action_node(kind: &str) -> bool {
    is_template_delim_start(kind)
        || is_template_delim_end(kind)
        || matches!(
            kind,
            "template_action"
                | "if_action"
                | "else_action"
                | "range_action"
                | "with_action"
                | "define_action"
                | "block_action"
                | "end_action"
        )
}

fn is_template_delim_start(kind: &str) -> bool {
    kind == "{{" || kind == "{{-"
}

fn is_template_delim_end(kind: &str) -> bool {
    kind == "}}" || kind == "-}}"
}
