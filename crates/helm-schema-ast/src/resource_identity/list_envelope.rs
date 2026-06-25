use crate::parse_helm_template;

pub(super) struct ListItemSource<'source> {
    pub(super) source: &'source str,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) path_prefix: Vec<String>,
}

pub(super) fn list_item_sources<'source>(
    source: &'source str,
    base_offset: usize,
    path_prefix: Vec<String>,
) -> Vec<ListItemSource<'source>> {
    let Some(tree) = parse_helm_template(source) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let Some(document) = first_document_node(root) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    if let Some(items_sequence) = top_level_items_sequence(document, source) {
        let mut cursor = items_sequence.walk();
        for item in items_sequence.children(&mut cursor) {
            if !item.is_named() || !matches!(item.kind(), "block_sequence_item" | "flow_node") {
                continue;
            }
            let content = sequence_item_content_node(item);
            let Some(item_source) = source.get(content.start_byte()..content.end_byte()) else {
                continue;
            };
            let mut item_prefix = path_prefix.clone();
            item_prefix.push("items[*]".to_string());
            items.push(ListItemSource {
                source: item_source,
                start: base_offset + content.start_byte(),
                end: base_offset + content.end_byte(),
                path_prefix: item_prefix,
            });
        }
    }
    items
}

fn first_document_node(root: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut cursor = root.walk();
    root.children(&mut cursor)
        .find(|child| child.is_named() && child.kind() == "document")
}

fn top_level_items_sequence<'tree>(
    document: tree_sitter::Node<'tree>,
    source: &str,
) -> Option<tree_sitter::Node<'tree>> {
    let mapping = top_level_mapping_node(document)?;
    let pair_kind = match mapping.kind() {
        "block_mapping" => "block_mapping_pair",
        "flow_mapping" => "flow_pair",
        _ => return None,
    };
    let mut cursor = mapping.walk();
    for pair in mapping.children(&mut cursor) {
        if !pair.is_named() || pair.kind() != pair_kind {
            continue;
        }
        let Some(key) = pair.child_by_field_name("key") else {
            continue;
        };
        if yaml_scalar_text(key, source) != Some("items") {
            continue;
        }
        return pair.child_by_field_name("value").and_then(sequence_node);
    }
    None
}

fn top_level_mapping_node(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        "block_mapping" | "flow_mapping" => Some(node),
        "document" => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .filter(|child| child.is_named())
                .find_map(top_level_mapping_node)
        }
        "block_node" | "flow_node" | "block_sequence_item" => {
            node.named_child(0).and_then(top_level_mapping_node)
        }
        _ => None,
    }
}

fn sequence_node(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        "block_sequence" | "flow_sequence" => Some(node),
        "block_node" | "flow_node" => node.named_child(0).and_then(sequence_node),
        _ => None,
    }
}

fn sequence_item_content_node(item: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let content = if item.kind() == "block_sequence_item" {
        item.named_child(0).unwrap_or(item)
    } else {
        item
    };
    unwrap_yaml_value_node(content)
}

fn unwrap_yaml_value_node(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    if matches!(node.kind(), "block_node" | "flow_node")
        && let Some(child) = node.named_child(0)
    {
        return unwrap_yaml_value_node(child);
    }
    node
}

fn yaml_scalar_text<'source>(
    node: tree_sitter::Node<'_>,
    source: &'source str,
) -> Option<&'source str> {
    let text = node.utf8_text(source.as_bytes()).ok()?.trim();
    if let Some(unquoted) = text
        .strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|text| text.strip_suffix('\''))
        })
    {
        Some(unquoted)
    } else {
        Some(text)
    }
}
