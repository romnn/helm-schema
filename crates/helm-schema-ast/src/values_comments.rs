use std::collections::BTreeMap;

use tree_sitter::Node;

use crate::ParseError;

/// Extract chart-authored descriptions from comments in a values YAML document.
///
/// The returned map is keyed by dotted `.Values` path without the leading
/// `.Values`. This is documentation metadata only: commented-out examples stay
/// comments and never become paths in this map.
pub fn extract_values_yaml_descriptions(
    src: &str,
) -> std::result::Result<BTreeMap<String, String>, ParseError> {
    let language = tree_sitter::Language::new(helm_schema_template_grammar::yaml::language());
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .map_err(|_| ParseError::TreeSitterParseFailed)?;

    let tree = parser
        .parse(src, None)
        .ok_or(ParseError::TreeSitterParseFailed)?;

    let mut descriptions = BTreeMap::new();
    collect_explicit_comment_descriptions(tree.root_node(), src, &mut descriptions);
    collect_node(tree.root_node(), src, &[], Vec::new(), &mut descriptions);
    Ok(descriptions)
}

fn collect_explicit_comment_descriptions(
    node: Node<'_>,
    src: &str,
    descriptions: &mut BTreeMap<String, String>,
) {
    if node.kind() == "comment"
        && let Some((path, description)) = explicit_comment_description(node, src)
    {
        insert_description(descriptions, &[path], description);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        collect_explicit_comment_descriptions(child, src, descriptions);
    }
}

fn collect_node<'tree>(
    node: Node<'tree>,
    src: &str,
    path: &[String],
    pending_comments: Vec<Node<'tree>>,
    descriptions: &mut BTreeMap<String, String>,
) {
    match node.kind() {
        "block_mapping" | "flow_mapping" => {
            collect_mapping(node, src, path, pending_comments, descriptions);
        }
        "block_sequence" | "flow_sequence" => {
            collect_sequence(node, src, path, pending_comments, descriptions);
        }
        "block_mapping_pair" | "flow_pair" => {
            collect_pair(node, src, path, pending_comments, descriptions);
        }
        _ => collect_wrapper(node, src, path, pending_comments, descriptions),
    }
}

fn collect_wrapper<'tree>(
    node: Node<'tree>,
    src: &str,
    path: &[String],
    pending_comments: Vec<Node<'tree>>,
    descriptions: &mut BTreeMap<String, String>,
) {
    let mut comments = pending_comments;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        if child.kind() == "comment" {
            comments.push(child);
            continue;
        }

        collect_node(
            child,
            src,
            path,
            std::mem::take(&mut comments),
            descriptions,
        );
    }
}

fn collect_mapping<'tree>(
    node: Node<'tree>,
    src: &str,
    path: &[String],
    pending_comments: Vec<Node<'tree>>,
    descriptions: &mut BTreeMap<String, String>,
) {
    let mut comments = pending_comments;
    let mut previous_pair_path: Option<Vec<String>> = None;
    let mut previous_pair_end_row: Option<usize> = None;

    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        match child.kind() {
            "comment" => {
                if previous_pair_end_row == Some(child.start_position().row) {
                    if let Some(path) = previous_pair_path.as_ref()
                        && let Some(description) = normalize_comment_block(&[child], src)
                    {
                        insert_description(descriptions, path, description);
                    }
                } else {
                    comments.push(child);
                }
            }
            "block_mapping_pair" | "flow_pair" => {
                let (trailing_comments, leading_comments) =
                    split_comments_before_next_pair(std::mem::take(&mut comments), src);
                if let Some(path) = previous_pair_path.as_ref()
                    && let Some(end_row) = previous_pair_end_row
                    && let Some(description) = normalize_comment_block(
                        contiguous_comments_after_row(&trailing_comments, end_row),
                        src,
                    )
                {
                    insert_description(descriptions, path, description);
                }
                previous_pair_path = collect_pair(child, src, path, leading_comments, descriptions);
                previous_pair_end_row = Some(child.end_position().row);
            }
            _ => {
                previous_pair_path = None;
                previous_pair_end_row = None;
                comments.clear();
                collect_node(child, src, path, Vec::new(), descriptions);
            }
        }
    }

    if let Some(path) = previous_pair_path.as_ref()
        && let Some(end_row) = previous_pair_end_row
        && let Some(description) = normalize_comment_block(
            contiguous_comments_after_row(
                trailing_comments_before_description_marker(&comments, src),
                end_row,
            ),
            src,
        )
    {
        insert_description(descriptions, path, description);
    }
}

fn split_comments_before_next_pair<'tree>(
    comments: Vec<Node<'tree>>,
    src: &str,
) -> (Vec<Node<'tree>>, Vec<Node<'tree>>) {
    let Some(description_start) = comments
        .iter()
        .position(|comment| comment_starts_helm_docs_description(*comment, src))
    else {
        return (Vec::new(), comments);
    };

    if description_start == 0 {
        return (Vec::new(), comments);
    }

    let trailing = comments
        .get(..description_start)
        .map_or_else(Vec::new, <[Node<'tree>]>::to_vec);
    let leading = comments
        .get(description_start..)
        .map_or_else(Vec::new, <[Node<'tree>]>::to_vec);
    (trailing, leading)
}

fn comment_starts_helm_docs_description(comment: Node<'_>, src: &str) -> bool {
    let Some(text) = node_text(comment, src) else {
        return false;
    };
    comment_body(text)
        .is_some_and(|line| is_helm_docs_description_marker(line) || line.starts_with("@param "))
}

fn trailing_comments_before_description_marker<'slice, 'tree>(
    comments: &'slice [Node<'tree>],
    src: &str,
) -> &'slice [Node<'tree>] {
    match comments
        .iter()
        .position(|comment| comment_starts_helm_docs_description(*comment, src))
    {
        Some(0) => &[],
        Some(index) => comments.get(..index).unwrap_or_default(),
        None => comments,
    }
}

fn collect_sequence<'tree>(
    node: Node<'tree>,
    src: &str,
    path: &[String],
    pending_comments: Vec<Node<'tree>>,
    descriptions: &mut BTreeMap<String, String>,
) {
    let mut comments = pending_comments;
    let mut item_path = path.to_vec();
    item_path.push("*".to_string());

    let mut cursor = node.walk();
    for child in node.children(&mut cursor).filter(|child| child.is_named()) {
        match child.kind() {
            "comment" => comments.push(child),
            "block_sequence_item" => {
                if let Some(description) = leading_description_for(&comments, child, src) {
                    insert_description(descriptions, &item_path, description);
                }
                collect_node(
                    child,
                    src,
                    &item_path,
                    std::mem::take(&mut comments),
                    descriptions,
                );
            }
            _ => {
                comments.clear();
                collect_node(child, src, path, Vec::new(), descriptions);
            }
        }
    }
}

fn collect_pair<'tree>(
    node: Node<'tree>,
    src: &str,
    parent_path: &[String],
    pending_comments: Vec<Node<'tree>>,
    descriptions: &mut BTreeMap<String, String>,
) -> Option<Vec<String>> {
    let key = pair_key(node, src)?;
    let mut path = parent_path.to_vec();
    path.push(key);

    if let Some(description) = leading_description_for(&pending_comments, node, src) {
        insert_description(descriptions, &path, description);
    }

    let value_comments = comments_before_pair_value(node);
    if let Some(value) = node.child_by_field_name("value") {
        collect_node(value, src, &path, value_comments, descriptions);
    }

    Some(path)
}

fn pair_key(node: Node<'_>, src: &str) -> Option<String> {
    let key = node.child_by_field_name("key")?;
    scalar_key_text(key, src)
}

fn scalar_key_text(node: Node<'_>, src: &str) -> Option<String> {
    match node.kind() {
        "string_scalar" | "boolean_scalar" | "integer_scalar" | "float_scalar" | "null_scalar"
        | "timestamp_scalar" => node_text(node, src).map(std::borrow::ToOwned::to_owned),
        "double_quote_scalar" | "single_quote_scalar" => node_text(node, src).map(unquote_scalar),
        _ => {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .filter(|child| child.is_named())
                .find_map(|child| scalar_key_text(child, src))
        }
    }
}

fn unquote_scalar(text: &str) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let Some(last) = trimmed.chars().next_back() else {
        return String::new();
    };

    if matches!((first, last), ('"', '"') | ('\'', '\'')) && trimmed.len() >= 2 {
        let start = first.len_utf8();
        let end = trimmed.len().saturating_sub(last.len_utf8());
        trimmed.get(start..end).unwrap_or("").to_string()
    } else {
        trimmed.to_string()
    }
}

fn comments_before_pair_value<'tree>(pair: Node<'tree>) -> Vec<Node<'tree>> {
    let Some(value) = pair.child_by_field_name("value") else {
        return Vec::new();
    };

    let mut comments = Vec::new();
    let mut cursor = pair.walk();
    for child in pair.children(&mut cursor).filter(|child| child.is_named()) {
        if child.kind() == "comment" && child.end_byte() <= value.start_byte() {
            comments.push(child);
        }
    }
    comments
}

fn leading_description_for(comments: &[Node<'_>], node: Node<'_>, src: &str) -> Option<String> {
    let adjacent = adjacent_comment_tail(comments, node.start_position().row);
    normalize_comment_block(adjacent, src)
}

fn adjacent_comment_tail<'slice, 'tree>(
    comments: &'slice [Node<'tree>],
    target_row: usize,
) -> &'slice [Node<'tree>] {
    let mut first = comments.len();
    let mut next_row = target_row;

    for (index, comment) in comments.iter().enumerate().rev() {
        if comment.end_position().row.saturating_add(1) != next_row {
            break;
        }
        first = index;
        next_row = comment.start_position().row;
    }

    comments.get(first..).unwrap_or_default()
}

fn contiguous_comments_after_row<'slice, 'tree>(
    comments: &'slice [Node<'tree>],
    previous_row: usize,
) -> &'slice [Node<'tree>] {
    let mut next_row = previous_row.saturating_add(1);
    let mut end = 0;

    for (index, comment) in comments.iter().enumerate() {
        if comment.start_position().row != next_row {
            break;
        }
        end = index + 1;
        next_row = comment.end_position().row.saturating_add(1);
    }

    comments.get(..end).unwrap_or_default()
}

fn normalize_comment_block(comments: &[Node<'_>], src: &str) -> Option<String> {
    let mut lines = Vec::new();
    for comment in comments {
        if let Some(line) = normalize_comment_line(node_text(*comment, src)?) {
            lines.push(line);
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn normalize_comment_line(text: &str) -> Option<String> {
    let line = comment_body(text)?;
    let line = strip_helm_docs_description_marker(line).unwrap_or(line);
    let line = line.trim_end();

    if line.is_empty() || line.trim_start().starts_with('@') || is_decorative_heading(line) {
        None
    } else {
        Some(line.to_string())
    }
}

fn explicit_comment_description(comment: Node<'_>, src: &str) -> Option<(String, String)> {
    let line = comment_body(node_text(comment, src)?)?;
    let rest = line.strip_prefix("@param ")?.trim_start();
    let split_at = rest.find(char::is_whitespace)?;
    let path = rest.get(..split_at)?.trim();
    let description = rest.get(split_at..)?.trim();

    if path.is_empty() || description.is_empty() {
        None
    } else {
        Some((path.to_string(), description.to_string()))
    }
}

fn comment_body(text: &str) -> Option<&str> {
    let mut line = text.trim_start();
    while let Some(rest) = line.strip_prefix('#') {
        line = rest.trim_start();
    }

    let line = line.trim_end();
    if line.len() == text.trim_start().len() {
        None
    } else {
        Some(line)
    }
}

fn is_helm_docs_description_marker(line: &str) -> bool {
    line == "--" || line.starts_with("-- ") || line.starts_with("--\t")
}

fn strip_helm_docs_description_marker(line: &str) -> Option<&str> {
    if line == "--" {
        Some("")
    } else if let Some(rest) = line.strip_prefix("-- ") {
        Some(rest.trim_start())
    } else {
        line.strip_prefix("--\t").map(str::trim_start)
    }
}

fn is_decorative_heading(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("----") && trimmed.ends_with("----")
}

fn insert_description(
    descriptions: &mut BTreeMap<String, String>,
    path: &[String],
    description: String,
) {
    if path.is_empty() || description.trim().is_empty() {
        return;
    }

    descriptions
        .entry(path.join("."))
        .and_modify(|existing| {
            if !existing.is_empty() {
                existing.push('\n');
            }
            existing.push_str(&description);
        })
        .or_insert(description);
}

fn node_text<'src>(node: Node<'_>, src: &'src str) -> Option<&'src str> {
    node.utf8_text(src.as_bytes()).ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use test_util::prelude::sim_assert_eq;

    use indoc::indoc;

    use super::extract_values_yaml_descriptions;

    #[test]
    fn extracts_leading_inline_and_nested_values_comments() {
        let yaml = indoc! {"
            # Root flag docs
            enabled: true # inline flag docs

            # -- Parent docs
            parent:
              # -- Child docs line 1
              # Child docs line 2
              child: value
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        let expected = BTreeMap::from([
            (
                "enabled".to_string(),
                "Root flag docs\ninline flag docs".to_string(),
            ),
            ("parent".to_string(), "Parent docs".to_string()),
            (
                "parent.child".to_string(),
                "Child docs line 1\nChild docs line 2".to_string(),
            ),
        ]);
        sim_assert_eq!(descriptions, expected);
    }

    #[test]
    fn comments_inside_parent_pair_document_first_nested_key() {
        let yaml = indoc! {"
            global:
              # -- Image registry docs
              imageRegistry: null
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        sim_assert_eq!(
            descriptions.get("global.imageRegistry").map(String::as_str),
            Some("Image registry docs")
        );
        assert!(
            !descriptions.contains_key("global"),
            "child comment must not attach to the parent object"
        );
    }

    #[test]
    fn commented_out_values_do_not_become_description_paths() {
        let yaml = indoc! {"
            config:
              # -- Disabled value docs
              # disabled: true
              enabled: true
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        assert!(!descriptions.contains_key("config.disabled"));
        sim_assert_eq!(
            descriptions.get("config.enabled").map(String::as_str),
            Some("Disabled value docs\ndisabled: true")
        );
    }

    #[test]
    fn helm_docs_marker_splits_previous_examples_from_next_description() {
        let yaml = indoc! {"
            ingress:
              # -- Ingress annotations
              annotations: {}
              # nginx.ingress.kubernetes.io/rewrite-target: /
              # cert-manager.io/cluster-issuer: letsencrypt-prod
              # -- Ingress hosts
              hosts: []
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        sim_assert_eq!(
            descriptions.get("ingress.annotations").map(String::as_str),
            Some(
                "Ingress annotations\nnginx.ingress.kubernetes.io/rewrite-target: /\ncert-manager.io/cluster-issuer: letsencrypt-prod"
            )
        );
        sim_assert_eq!(
            descriptions.get("ingress.hosts").map(String::as_str),
            Some("Ingress hosts")
        );
    }

    #[test]
    fn trailing_examples_at_end_of_mapping_document_previous_key() {
        let yaml = indoc! {"
            ingress:
              # -- Ingress annotations
              annotations: {}
              # nginx.ingress.kubernetes.io/rewrite-target: /

            detached: true

            # This detached comment is separated from the key above.
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        sim_assert_eq!(
            descriptions.get("ingress.annotations").map(String::as_str),
            Some("Ingress annotations\nnginx.ingress.kubernetes.io/rewrite-target: /")
        );
        sim_assert_eq!(descriptions.get("detached").map(String::as_str), None);
    }

    #[test]
    fn decorative_comment_headings_do_not_become_descriptions() {
        let yaml = indoc! {"
            section:
              ###
              ### ---- OPERATOR ----
              ###
              operator:
                # -- Operator image docs
                image: example
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        assert!(!descriptions.contains_key("section.operator"));
        sim_assert_eq!(
            descriptions
                .get("section.operator.image")
                .map(String::as_str),
            Some("Operator image docs")
        );
    }

    #[test]
    fn helm_docs_param_comments_attach_to_explicit_paths() {
        let yaml = indoc! {"
            ## @section Global parameters
            ## Global Docker image parameters
            ##
            ## @param global.imageRegistry Global Docker image registry
            ## @param global.imagePullSecrets Global Docker registry secret names as an array
            global:
              imageRegistry: \"\"
              imagePullSecrets: []

            auth:
              ## @param auth.enabled Enable password authentication
              ##
              enabled: true
        "};

        let descriptions = extract_values_yaml_descriptions(yaml).expect("parse yaml comments");

        sim_assert_eq!(
            descriptions.get("global.imageRegistry").map(String::as_str),
            Some("Global Docker image registry")
        );
        sim_assert_eq!(
            descriptions
                .get("global.imagePullSecrets")
                .map(String::as_str),
            Some("Global Docker registry secret names as an array")
        );
        sim_assert_eq!(
            descriptions.get("auth.enabled").map(String::as_str),
            Some("Enable password authentication")
        );
        assert!(
            !descriptions.contains_key("global"),
            "helm-docs section comments must not attach to the parent object"
        );
    }
}
