use helm_schema_ast::{
    DefineIndex, HelmAst, HelmParser, Literal, TemplateExpr, TreeSitterParser,
    parse_action_expressions,
};

use crate::helper_eval::{
    CapabilityGuard, HelperBranch, HelperBranchBody, HelperOutput, decode_guard, helper_evaluate,
};
use crate::{ResourceRef, YamlPath};

/// AST-driven detector for Kubernetes resource identity.
///
/// The detector only reads manifest structure: top-level `apiVersion` / `kind`
/// mapping pairs, structural Helm control-flow nodes that wrap those pairs, and
/// helper calls in `apiVersion` values that statically evaluate to literal
/// outputs. It preserves typed capability branches so the K8s lookup layer can
/// choose the runtime-live branch instead of flattening mutually-exclusive
/// alternatives.
pub(crate) struct AstResourceDetector<'a> {
    defines: &'a DefineIndex,
}

impl<'a> AstResourceDetector<'a> {
    #[must_use]
    pub(crate) fn new(defines: &'a DefineIndex) -> Self {
        Self { defines }
    }

    /// Detect the resource identity for one manifest document subtree.
    ///
    /// Multi-document template sources are split by [`ResourceCursor`] before
    /// this method is called. Keeping that boundary outside the detector avoids
    /// mixing `apiVersion` candidates from unrelated YAML documents.
    #[must_use]
    pub(crate) fn detect(&self, ast: &HelmAst) -> Option<ResourceRef> {
        let mut state = ResourceState::default();
        self.scan_node(ast, &mut state, true);
        state.resource()
    }

    fn scan_items(&self, items: &[HelmAst], state: &mut ResourceState, capture_branches: bool) {
        for item in items {
            self.scan_node(item, state, capture_branches);
        }
    }

    fn scan_node(&self, node: &HelmAst, state: &mut ResourceState, capture_branches: bool) {
        match node {
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                self.scan_items(items, state, capture_branches);
            }
            HelmAst::Pair { key, value } => {
                let Some(key_text) = scalar_text(key) else {
                    return;
                };
                match key_text {
                    "apiVersion" => {
                        if let Some(output) = self.api_version_output(value.as_deref()) {
                            state.record_api_version_output(output);
                        }
                    }
                    "kind" => {
                        if state.kind.is_none()
                            && let Some(value) = value.as_deref().and_then(scalar_text)
                            && !value.is_empty()
                        {
                            state.kind = Some(value.to_string());
                        }
                    }
                    _ => {}
                }
            }
            HelmAst::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if capture_branches
                    && is_capability_guard(cond)
                    && let Some(branches) = self.inline_api_version_branches(node)
                {
                    state.record_api_version_branches(branches);
                    self.scan_items(then_branch, state, false);
                    self.scan_items(else_branch, state, false);
                    return;
                }
                self.scan_items(then_branch, state, capture_branches);
                self.scan_items(else_branch, state, capture_branches);
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                self.scan_items(body, state, capture_branches);
                self.scan_items(else_branch, state, capture_branches);
            }
            HelmAst::Block { body, .. } => {
                self.scan_items(body, state, capture_branches);
            }
            HelmAst::Define { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    fn inline_api_version_branches(&self, node: &HelmAst) -> Option<Vec<HelperBranch>> {
        let branches = self.inline_api_version_branches_inner(node)?;
        if branches.is_empty() {
            None
        } else {
            Some(branches)
        }
    }

    fn inline_api_version_branches_inner(&self, node: &HelmAst) -> Option<Vec<HelperBranch>> {
        let HelmAst::If {
            cond,
            then_branch,
            else_branch,
        } = node
        else {
            return None;
        };
        let guard = decode_guard(cond);
        if !matches!(
            guard,
            CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
        ) {
            return None;
        }

        let mut branches = Vec::new();
        branches.push(HelperBranch {
            guard: Some(guard),
            body: self.api_version_branch_body(then_branch),
        });

        if let [nested @ HelmAst::If { .. }] = else_branch.as_slice()
            && let Some(nested_branches) = self.inline_api_version_branches_inner(nested)
        {
            branches.extend(nested_branches);
        } else if !else_branch.is_empty() {
            branches.push(HelperBranch {
                guard: None,
                body: self.api_version_branch_body(else_branch),
            });
        }

        branches.retain(|branch| !branch.body.is_empty());
        if branches.is_empty() {
            None
        } else {
            Some(branches)
        }
    }

    fn api_version_branch_body(&self, items: &[HelmAst]) -> HelperBranchBody {
        let mut literals = Vec::new();
        let mut nested = Vec::new();
        for item in items {
            self.collect_api_version_outputs(item, &mut literals, &mut nested);
        }
        if nested.is_empty() {
            return HelperBranchBody::literals(dedup_preserve_order(literals));
        }

        let literals = dedup_preserve_order(literals);
        if !literals.is_empty() {
            nested.insert(
                0,
                HelperBranch {
                    guard: None,
                    body: HelperBranchBody::literals(literals),
                },
            );
        }
        HelperBranchBody::Nested { branches: nested }
    }

    fn collect_api_version_outputs(
        &self,
        node: &HelmAst,
        literals: &mut Vec<String>,
        nested: &mut Vec<HelperBranch>,
    ) {
        match node {
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    self.collect_api_version_outputs(item, literals, nested);
                }
            }
            HelmAst::Pair { key, value } => {
                if scalar_text(key) == Some("apiVersion")
                    && let Some(output) = self.api_version_output(value.as_deref())
                {
                    match output {
                        HelperOutput::Literals(values) => literals.extend(values),
                        HelperOutput::Branched { branches } => nested.extend(branches),
                    }
                }
            }
            HelmAst::If { .. } => {
                if let Some(branches) = self.inline_api_version_branches_inner(node) {
                    nested.extend(branches);
                }
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                for item in body.iter().chain(else_branch) {
                    self.collect_api_version_outputs(item, literals, nested);
                }
            }
            HelmAst::Block { body, .. } => {
                for item in body {
                    self.collect_api_version_outputs(item, literals, nested);
                }
            }
            HelmAst::Define { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    fn api_version_output(&self, value: Option<&HelmAst>) -> Option<HelperOutput> {
        match value? {
            HelmAst::Scalar { text } => {
                let value = text.trim();
                if value.is_empty() {
                    None
                } else {
                    Some(HelperOutput::Literals(vec![value.to_string()]))
                }
            }
            HelmAst::HelmExpr { text } => self.helper_api_version_output(text),
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                for item in items {
                    if let Some(output) = self.api_version_output(Some(item)) {
                        return Some(output);
                    }
                }
                None
            }
            HelmAst::Pair { value, .. } => self.api_version_output(value.as_deref()),
            node @ HelmAst::If { .. } => self
                .inline_api_version_branches(node)
                .map(|branches| HelperOutput::Branched { branches }),
            HelmAst::Sequence { .. }
            | HelmAst::Range { .. }
            | HelmAst::With { .. }
            | HelmAst::Define { .. }
            | HelmAst::Block { .. }
            | HelmAst::HelmComment { .. } => None,
        }
    }

    fn helper_api_version_output(&self, text: &str) -> Option<HelperOutput> {
        let mut combined = ResourceState::default();
        for name in helper_call_names(text) {
            combined.record_api_version_output(helper_evaluate(&name, self.defines));
        }
        if combined.api_versions.is_empty() && combined.api_version_branches.is_empty() {
            None
        } else if combined.api_version_branches.is_empty() {
            Some(HelperOutput::Literals(combined.api_versions))
        } else {
            Some(HelperOutput::Branched {
                branches: combined.api_version_branches,
            })
        }
    }
}

/// Source-position cursor over AST-detected document resources.
#[derive(Default, Clone, Debug)]
pub(crate) struct ResourceCursor {
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

impl ResourceCursor {
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

    pub(crate) fn advance_to(&mut self, byte: usize) {
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

    #[must_use]
    pub(crate) fn current(&self) -> Option<ResourceRef> {
        self.current_span
            .and_then(|index| self.spans.get(index))
            .map(|span| span.resource.clone())
    }

    #[must_use]
    pub(crate) fn rebase_path(&self, path: YamlPath) -> YamlPath {
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
    let Some(mapping) = top_level_mapping_node(document) else {
        return None;
    };
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

#[derive(Default)]
struct ResourceState {
    kind: Option<String>,
    api_versions: Vec<String>,
    multi_branch: bool,
    api_version_branches: Vec<HelperBranch>,
}

impl ResourceState {
    fn record_api_version_output(&mut self, output: HelperOutput) {
        match output {
            HelperOutput::Literals(literals) => {
                if literals.len() > 1 {
                    self.multi_branch = true;
                }
                for literal in literals {
                    self.insert_api_version(literal);
                }
            }
            HelperOutput::Branched { branches } => self.record_api_version_branches(branches),
        }
    }

    fn record_api_version_branches(&mut self, branches: Vec<HelperBranch>) {
        if branches.is_empty() {
            return;
        }
        self.multi_branch = true;
        for branch in &branches {
            for literal in branch.body.all_literals() {
                self.insert_api_version(literal);
            }
        }
        self.api_version_branches.extend(branches);
    }

    fn insert_api_version(&mut self, value: String) {
        if !value.is_empty() && !self.api_versions.contains(&value) {
            self.api_versions.push(value);
        }
    }

    fn resource(self) -> Option<ResourceRef> {
        let kind = self.kind?;
        let (api_version, api_version_candidates) = if self.multi_branch {
            (String::new(), self.api_versions)
        } else {
            let mut versions = self.api_versions;
            let primary = versions.first().cloned().unwrap_or_default();
            versions.retain(|version| version != &primary);
            (primary, versions)
        };
        Some(ResourceRef {
            api_version,
            kind,
            api_version_candidates,
            api_version_branches: self.api_version_branches,
        })
    }
}

fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
}

fn is_capability_guard(cond: &str) -> bool {
    matches!(
        decode_guard(cond),
        CapabilityGuard::Has { .. } | CapabilityGuard::NotHas { .. }
    )
}

fn helper_call_names(text: &str) -> Vec<String> {
    let action_text = format!("{{{{ {text} }}}}");
    let mut out = Vec::new();
    for expr in parse_action_expressions(&action_text) {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            if !matches!(function.as_str(), "include" | "template") {
                return;
            }
            let Some(TemplateExpr::Literal(Literal::String(name) | Literal::RawString(name))) =
                args.first()
            else {
                return;
            };
            if !name.is_empty() && !out.contains(name) {
                out.push(name.clone());
            }
        });
    }
    out
}

fn dedup_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        if !value.is_empty() && !out.contains(&value) {
            out.push(value);
        }
    }
    out
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
    use helm_schema_ast::{DefineIndex, HelmParser, TreeSitterParser};
    use indoc::indoc;

    use super::{AstResourceDetector, ResourceCursor};
    use crate::helper_eval::{CapabilityGuard, HelperBranchBody};

    fn detect(src: &str, defines: &DefineIndex) -> Option<crate::ResourceRef> {
        let ast = TreeSitterParser.parse(src).expect("parse template");
        AstResourceDetector::new(defines).detect(&ast)
    }

    #[test]
    fn detects_kind_before_api_version() {
        let resource = detect(
            indoc! {r#"
                kind: NetworkPolicy
                apiVersion: networking.k8s.io/v1
                metadata:
                  name: example
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        assert_eq!(resource.kind, "NetworkPolicy");
        assert_eq!(resource.api_version, "networking.k8s.io/v1");
    }

    #[test]
    fn resolves_helper_returned_api_version() {
        let helpers = indoc! {r#"
            {{- define "x.apiVersion" -}}
            {{- print "apps/v1" -}}
            {{- end -}}
        "#};
        let mut defines = DefineIndex::new();
        defines
            .add_source(&TreeSitterParser, helpers)
            .expect("helpers");
        let resource = detect(
            indoc! {r#"
                apiVersion: {{ template "x.apiVersion" . }}
                kind: Deployment
                metadata:
                  name: example
            "#},
            &defines,
        )
        .expect("resource");

        assert_eq!(resource.kind, "Deployment");
        assert_eq!(resource.api_version, "apps/v1");
        assert!(resource.api_version_candidates.is_empty());
    }

    #[test]
    fn preserves_inline_capability_branches() {
        let resource = detect(
            indoc! {r#"
                {{- if .Capabilities.APIVersions.Has "policy/v1" }}
                apiVersion: policy/v1
                {{- else }}
                apiVersion: policy/v1beta1
                {{- end }}
                kind: PodDisruptionBudget
                metadata:
                  name: example
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        assert_eq!(resource.kind, "PodDisruptionBudget");
        assert_eq!(resource.api_version, "");
        assert_eq!(
            resource.api_version_candidates,
            vec!["policy/v1".to_string(), "policy/v1beta1".to_string()]
        );
        assert_eq!(resource.api_version_branches.len(), 2);
        assert_eq!(
            resource.api_version_branches[0].guard,
            Some(CapabilityGuard::Has {
                api: "policy/v1".to_string()
            })
        );
        assert_eq!(
            resource.api_version_branches[1].body,
            HelperBranchBody::literals(vec!["policy/v1beta1".to_string()])
        );
    }

    #[test]
    fn mixed_literal_and_nested_branch_preserves_nested_guards() {
        let resource = detect(
            indoc! {r#"
                {{- if .Capabilities.APIVersions.Has "policy/v1" }}
                apiVersion: policy/v1
                {{- if .Capabilities.APIVersions.Has "policy/v1/PodDisruptionBudget" }}
                apiVersion: policy/v1
                {{- else }}
                apiVersion: policy/v1beta1
                {{- end }}
                {{- else }}
                apiVersion: policy/v1beta1
                {{- end }}
                kind: PodDisruptionBudget
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        let HelperBranchBody::Nested { branches } = &resource.api_version_branches[0].body else {
            panic!("expected nested branch body");
        };
        assert_eq!(branches.len(), 3);
        assert_eq!(
            branches[0].body,
            HelperBranchBody::literals(vec!["policy/v1".to_string()])
        );
        assert_eq!(
            branches[1].guard,
            Some(CapabilityGuard::Has {
                api: "policy/v1/PodDisruptionBudget".to_string()
            })
        );
        assert_eq!(
            branches[2].body,
            HelperBranchBody::literals(vec!["policy/v1beta1".to_string()])
        );
    }

    #[test]
    fn capability_guard_without_api_version_does_not_create_empty_branch_resource() {
        let resource = detect(
            indoc! {r#"
                {{- if .Capabilities.APIVersions.Has "v1/ConfigMap" }}
                metadata:
                  labels:
                    enabled: "true"
                {{- end }}
                apiVersion: v1
                kind: ConfigMap
            "#},
            &DefineIndex::new(),
        )
        .expect("resource");

        assert_eq!(resource.kind, "ConfigMap");
        assert_eq!(resource.api_version, "v1");
        assert!(resource.api_version_candidates.is_empty());
        assert!(resource.api_version_branches.is_empty());
    }

    #[test]
    fn resource_cursor_keeps_multi_document_resources_separate() {
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
        let mut cursor = ResourceCursor::from_source(source, &DefineIndex::new());

        cursor.advance_to(source.find("first").expect("first marker"));
        let first = cursor.current().expect("first resource");
        assert_eq!(first.kind, "ConfigMap");
        assert_eq!(first.api_version, "v1");
        assert!(first.api_version_candidates.is_empty());

        cursor.advance_to(source.find("replicas").expect("replicas marker"));
        let second = cursor.current().expect("second resource");
        assert_eq!(second.kind, "Deployment");
        assert_eq!(second.api_version, "apps/v1");
        assert!(second.api_version_candidates.is_empty());
    }

    #[test]
    fn resource_cursor_descends_into_list_items_and_rebases_paths() {
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
        let mut cursor = ResourceCursor::from_source(source, &DefineIndex::new());
        assert_eq!(
            cursor.spans.len(),
            2,
            "List envelope should produce one span per inner resource: {:?}",
            cursor.spans
        );

        cursor.advance_to(source.find("host").expect("host marker"));
        let ingress = cursor.current().expect("ingress resource");
        assert_eq!(ingress.kind, "Ingress");
        assert_eq!(ingress.api_version, "networking.k8s.io/v1");
        assert_eq!(
            cursor.rebase_path(crate::YamlPath(vec![
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

        cursor.advance_to(source.find("port").expect("port marker"));
        let service = cursor.current().expect("service resource");
        assert_eq!(service.kind, "Service");
        assert_eq!(service.api_version, "v1");
        assert_eq!(
            cursor.rebase_path(crate::YamlPath(vec![
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

        cursor.advance_to(source.find("kind: List").expect("list marker"));
        assert!(
            cursor.current().is_none(),
            "the transparent List wrapper must not become the current resource"
        );
    }

    #[test]
    fn resource_cursor_descends_into_ranged_list_items() {
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
        let mut cursor = ResourceCursor::from_source(source, &DefineIndex::new());
        assert_eq!(
            cursor.spans.len(),
            1,
            "ranged List envelope should produce the inner resource span: {:?}",
            cursor.spans
        );

        cursor.advance_to(source.find("host").expect("host marker"));
        let ingress = cursor.current().expect("ingress resource");
        assert_eq!(ingress.kind, "Ingress");
        assert_eq!(ingress.api_version, "networking.k8s.io/v1");
        assert_eq!(
            cursor.rebase_path(crate::YamlPath(vec![
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
    fn resource_cursor_keeps_non_kubernetes_list_kind_as_resource() {
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
        let mut cursor = ResourceCursor::from_source(source, &DefineIndex::new());
        assert_eq!(
            cursor.spans.len(),
            1,
            "only the exact Kubernetes v1/List envelope should be transparent: {:?}",
            cursor.spans
        );

        cursor.advance_to(source.find("port").expect("port marker"));
        let resource = cursor.current().expect("outer resource");
        assert_eq!(resource.kind, "List");
        assert_eq!(resource.api_version, "example.com/v1");
        assert_eq!(
            cursor.rebase_path(crate::YamlPath(vec![
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
