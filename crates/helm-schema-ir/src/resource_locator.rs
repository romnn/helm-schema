use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};

use crate::resource_detector::AstResourceDetector;
use crate::{ResourceRef, YamlPath};

/// Provides source-position-aware Kubernetes resource context for a manifest
/// template.
pub(crate) trait ResourceLocator {
    fn advance_to(&mut self, byte: usize);
    fn current_resource(&self) -> Option<&ResourceRef>;
    fn rebase_path(&self, path: YamlPath) -> YamlPath;
}

/// AST-backed resource locator over rendered-manifest source bytes.
///
/// Resource identity detection is delegated to [`AstResourceDetector`]. This
/// type owns only source span selection and transparent Kubernetes `kind: List`
/// descent, keeping byte-position concerns out of the symbolic IR walker.
#[derive(Default, Clone, Debug)]
pub(crate) struct AstResourceLocator {
    spans: Vec<ResourceSpan>,
    current_span: Option<usize>,
}

#[derive(Clone, Debug)]
struct ResourceSpan {
    start: usize,
    end: usize,
    resource: ResourceRef,
    path_prefix: Vec<String>,
}

impl AstResourceLocator {
    #[must_use]
    pub(crate) fn from_source(source: &str, defines: &DefineIndex) -> Self {
        let mut spans = Vec::new();
        for (start, end) in document_spans(source) {
            let Some(doc_src) = source.get(start..end) else {
                continue;
            };
            spans.extend(resource_spans_for_manifest_source(
                doc_src,
                start,
                start,
                end,
                Vec::new(),
                defines,
            ));
        }
        spans.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| left.end.cmp(&right.end))
        });
        Self {
            spans,
            current_span: None,
        }
    }

    #[cfg(test)]
    fn span_count(&self) -> usize {
        self.spans.len()
    }
}

impl ResourceLocator for AstResourceLocator {
    fn advance_to(&mut self, byte: usize) {
        self.current_span = self
            .spans
            .iter()
            .enumerate()
            .filter(|(_, span)| span.start <= byte && byte < span.end)
            .min_by(|(_, left), (_, right)| {
                let left_len = left.end.saturating_sub(left.start);
                let right_len = right.end.saturating_sub(right.start);
                left_len
                    .cmp(&right_len)
                    .then_with(|| right.start.cmp(&left.start))
            })
            .map(|(index, _)| index);
    }

    fn current_resource(&self) -> Option<&ResourceRef> {
        self.current_span
            .and_then(|index| self.spans.get(index))
            .map(|span| &span.resource)
    }

    fn rebase_path(&self, path: YamlPath) -> YamlPath {
        let Some(span) = self.current_span.and_then(|index| self.spans.get(index)) else {
            return path;
        };
        if span.path_prefix.is_empty() || !path.0.starts_with(&span.path_prefix) {
            return path;
        }
        YamlPath(path.0[span.path_prefix.len()..].to_vec())
    }
}

fn resource_spans_for_manifest_source(
    source: &str,
    base_offset: usize,
    span_start: usize,
    span_end: usize,
    path_prefix: Vec<String>,
    defines: &DefineIndex,
) -> Vec<ResourceSpan> {
    let Some(resource) = detect_manifest_resource(source, defines) else {
        return Vec::new();
    };

    if is_kubernetes_list_envelope(&resource) {
        return list_item_resource_spans(source, base_offset, path_prefix, defines);
    }

    vec![ResourceSpan {
        start: span_start,
        end: span_end,
        resource,
        path_prefix,
    }]
}

fn is_kubernetes_list_envelope(resource: &ResourceRef) -> bool {
    resource.kind == "List"
        && resource.api_version == "v1"
        && resource.api_version_candidates.is_empty()
        && resource.api_version_branches.is_empty()
}

fn detect_manifest_resource(source: &str, defines: &DefineIndex) -> Option<ResourceRef> {
    if let Some(resource) = TreeSitterParser
        .parse(source)
        .ok()
        .and_then(|ast| AstResourceDetector::new(defines).detect(&ast))
    {
        return Some(resource);
    }

    let normalized = normalize_sequence_item_source(source);
    if normalized == source {
        return None;
    }
    TreeSitterParser
        .parse(&normalized)
        .ok()
        .and_then(|ast| AstResourceDetector::new(defines).detect(&ast))
}

fn normalize_sequence_item_source(source: &str) -> String {
    let mut lines = source.lines();
    let Some(first) = lines.next() else {
        return source.to_string();
    };
    let rest = lines.collect::<Vec<_>>();
    // Tree-sitter gives us the exact mapping node inside a YAML sequence item.
    // The first key starts immediately after `- `, while continuation lines
    // still carry their original document indentation. Dedent only that shared
    // continuation indentation so the item parses as a standalone resource.
    let Some(indent) = rest
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .filter(|indent| *indent > 0)
        .min()
    else {
        return source.to_string();
    };

    let mut normalized = String::with_capacity(source.len());
    normalized.push_str(first);
    for line in rest {
        normalized.push('\n');
        let line_indent = line.len() - line.trim_start_matches(' ').len();
        if line_indent >= indent {
            normalized.push_str(&line[indent..]);
        } else {
            normalized.push_str(line);
        }
    }
    if source.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn list_item_resource_spans(
    source: &str,
    base_offset: usize,
    path_prefix: Vec<String>,
    defines: &DefineIndex,
) -> Vec<ResourceSpan> {
    let Some(tree) = parse_template_tree(source) else {
        return Vec::new();
    };
    let root = tree.root_node();
    let Some(document) = first_document_node(root) else {
        return Vec::new();
    };
    let mut spans = Vec::new();
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
            spans.extend(resource_spans_for_manifest_source(
                item_source,
                base_offset + content.start_byte(),
                base_offset + content.start_byte(),
                base_offset + content.end_byte(),
                item_prefix,
                defines,
            ));
        }
    }
    spans
}

fn parse_template_tree(source: &str) -> Option<tree_sitter::Tree> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(source, None)
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

fn document_spans(source: &str) -> Vec<(usize, usize)> {
    let language =
        tree_sitter::Language::new(helm_schema_template_grammar::helm_template::language());
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return whole_source_span(source);
    }
    let Some(tree) = parser.parse(source, None) else {
        return whole_source_span(source);
    };

    let root = tree.root_node();
    let mut docs = Vec::new();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.is_named() && child.kind() == "document" {
            docs.push((child.start_byte(), child.end_byte()));
        }
    }
    if docs.is_empty() {
        return whole_source_span(source);
    }
    docs.sort_by_key(|(start, _)| *start);
    for index in 0..docs.len() {
        let end = docs
            .get(index + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(source.len());
        docs[index].1 = end;
    }
    docs
}

fn whole_source_span(source: &str) -> Vec<(usize, usize)> {
    if source.is_empty() {
        Vec::new()
    } else {
        vec![(0, source.len())]
    }
}

#[cfg(test)]
mod tests {
    use helm_schema_ast::DefineIndex;
    use indoc::indoc;

    use super::{AstResourceLocator, ResourceLocator};

    #[test]
    fn resource_locator_keeps_multi_document_resources_separate() {
        let source = indoc! {r#"
            apiVersion: v1
            kind: ConfigMap
            data:
              first: "{{ .Values.first }}"
            ---
            apiVersion: apps/v1
            kind: Deployment
            spec:
              replicas: {{ .Values.replicas }}
        "#};
        let mut locator = AstResourceLocator::from_source(source, &DefineIndex::new());

        locator.advance_to(source.find("first").expect("first marker"));
        let first = locator.current_resource().expect("first resource");
        assert_eq!(first.kind, "ConfigMap");
        assert_eq!(first.api_version, "v1");
        assert!(first.api_version_candidates.is_empty());

        locator.advance_to(source.find("replicas").expect("replicas marker"));
        let second = locator.current_resource().expect("second resource");
        assert_eq!(second.kind, "Deployment");
        assert_eq!(second.api_version, "apps/v1");
        assert!(second.api_version_candidates.is_empty());
    }

    #[test]
    fn resource_locator_descends_into_list_items_and_rebases_paths() {
        let source = indoc! {r#"
            apiVersion: v1
            kind: List
            items:
              - apiVersion: networking.k8s.io/v1
                kind: Ingress
                spec:
                  rules:
                    - host: {{ .Values.host | quote }}
              - apiVersion: v1
                kind: Service
                spec:
                  ports:
                    - port: {{ .Values.port }}
        "#};
        let mut locator = AstResourceLocator::from_source(source, &DefineIndex::new());
        assert_eq!(
            locator.span_count(),
            2,
            "List envelope should produce one span per inner resource"
        );

        locator.advance_to(source.find("host").expect("host marker"));
        let ingress = locator.current_resource().expect("ingress resource");
        assert_eq!(ingress.kind, "Ingress");
        assert_eq!(ingress.api_version, "networking.k8s.io/v1");
        assert_eq!(
            locator.rebase_path(crate::YamlPath(vec![
                "items[*]".to_string(),
                "spec".to_string(),
                "rules[*]".to_string(),
                "host".to_string(),
            ])),
            crate::YamlPath(vec![
                "spec".to_string(),
                "rules[*]".to_string(),
                "host".to_string(),
            ])
        );

        locator.advance_to(source.find("port").expect("port marker"));
        let service = locator.current_resource().expect("service resource");
        assert_eq!(service.kind, "Service");
        assert_eq!(service.api_version, "v1");
        assert_eq!(
            locator.rebase_path(crate::YamlPath(vec![
                "items[*]".to_string(),
                "spec".to_string(),
                "ports[*]".to_string(),
                "port".to_string(),
            ])),
            crate::YamlPath(vec![
                "spec".to_string(),
                "ports[*]".to_string(),
                "port".to_string(),
            ])
        );

        locator.advance_to(source.find("kind: List").expect("list marker"));
        assert!(
            locator.current_resource().is_none(),
            "the transparent List wrapper must not become the current resource"
        );
    }

    #[test]
    fn resource_locator_descends_into_ranged_list_items() {
        let source = indoc! {r#"
            apiVersion: v1
            kind: List
            items:
            {{- range $index, $replica := until (.Values.replicas | int) }}
              - apiVersion: networking.k8s.io/v1
                kind: Ingress
                spec:
                  rules:
                    - host: {{ $.Values.host | quote }}
            {{- end }}
        "#};
        let mut locator = AstResourceLocator::from_source(source, &DefineIndex::new());
        assert_eq!(
            locator.span_count(),
            1,
            "ranged List envelope should produce the inner resource span"
        );

        locator.advance_to(source.find("host").expect("host marker"));
        let ingress = locator.current_resource().expect("ingress resource");
        assert_eq!(ingress.kind, "Ingress");
        assert_eq!(ingress.api_version, "networking.k8s.io/v1");
        assert_eq!(
            locator.rebase_path(crate::YamlPath(vec![
                "items[*]".to_string(),
                "spec".to_string(),
                "rules[*]".to_string(),
                "host".to_string(),
            ])),
            crate::YamlPath(vec![
                "spec".to_string(),
                "rules[*]".to_string(),
                "host".to_string(),
            ])
        );
    }

    #[test]
    fn resource_locator_keeps_non_kubernetes_list_kind_as_resource() {
        let source = indoc! {r#"
            apiVersion: example.com/v1
            kind: List
            items:
              - apiVersion: v1
                kind: Service
                spec:
                  ports:
                    - port: {{ .Values.port }}
        "#};
        let mut locator = AstResourceLocator::from_source(source, &DefineIndex::new());
        assert_eq!(
            locator.span_count(),
            1,
            "only the exact Kubernetes v1/List envelope should be transparent"
        );

        locator.advance_to(source.find("port").expect("port marker"));
        let resource = locator.current_resource().expect("outer resource");
        assert_eq!(resource.kind, "List");
        assert_eq!(resource.api_version, "example.com/v1");
        assert_eq!(
            locator.rebase_path(crate::YamlPath(vec![
                "items[*]".to_string(),
                "spec".to_string(),
                "ports[*]".to_string(),
                "port".to_string(),
            ])),
            crate::YamlPath(vec![
                "items[*]".to_string(),
                "spec".to_string(),
                "ports[*]".to_string(),
                "port".to_string(),
            ])
        );
    }
}
