pub(crate) fn children_with_field<'node>(
    node: tree_sitter::Node<'node>,
    field: &str,
) -> Vec<tree_sitter::Node<'node>> {
    let mut cursor = node.walk();
    node.children_by_field_name(field, &mut cursor)
        .filter(tree_sitter::Node::is_named)
        .collect()
}
