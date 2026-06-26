use std::collections::HashSet;

use helm_schema_ast::{
    ResourceSpan, TemplateExpr, TemplateHeader, decode_guard, decode_guard_expr, parse_expr_text,
    parse_helm_template, parse_yaml_key,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, ResourceRef};

use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::node_eval::{NodeAction, node_action};

const MAX_RECURSION_DEPTH: usize = 12;

pub(crate) fn collect_resource_spans(
    source: &str,
    analysis_db: &IrAnalysisDb,
) -> Vec<ResourceSpan> {
    let mut spans = Vec::new();
    for (start, end) in document_spans(source) {
        let Some(document_source) = source.get(start..end) else {
            continue;
        };
        spans.extend(resource_spans_for_manifest_source(
            document_source,
            start,
            start,
            end,
            Vec::new(),
            analysis_db,
        ));
    }
    if spans.is_empty() {
        spans.extend(resource_spans_from_source_keys(source));
    }
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
    });
    spans
}

pub(crate) fn helper_body_defines_resource(name: &str, analysis_db: &IrAnalysisDb) -> bool {
    let Some(body) = analysis_db.parsed_helper_body(name) else {
        return false;
    };
    detect_manifest_resource(body.source, analysis_db).is_some()
}

#[derive(Default)]
struct ResourceState {
    kind: Option<String>,
    api_versions: Vec<String>,
    suppress_primary_api_version: bool,
    api_version_branches: Vec<HelperBranch>,
}

impl ResourceState {
    fn set_kind_if_empty(&mut self, kind: &str) {
        if self.kind.is_none() && !kind.is_empty() {
            self.kind = Some(kind.to_string());
        }
    }

    fn record_api_version_output(&mut self, output: HelperBranchBody) {
        match output {
            HelperBranchBody::Literals { values } => {
                for value in values {
                    self.insert_api_version(value);
                }
            }
            HelperBranchBody::Nested { branches } => {
                self.suppress_primary_api_version = true;
                self.record_api_version_branch_literals(&branches);
            }
        }
    }

    fn record_api_version_branches(&mut self, branches: Vec<HelperBranch>) {
        if branches.is_empty() {
            return;
        }
        self.record_api_version_branch_literals(&branches);
        self.api_version_branches.extend(branches);
    }

    fn record_api_version_branch_literals(&mut self, branches: &[HelperBranch]) {
        for branch in branches {
            for value in branch.body.all_literals() {
                self.insert_api_version(value);
            }
        }
    }

    fn insert_api_version(&mut self, value: String) {
        if !value.is_empty() && !self.api_versions.contains(&value) {
            self.api_versions.push(value);
        }
    }

    fn into_resource(self) -> Option<ResourceRef> {
        let kind = self.kind?;
        let mut versions = self.api_versions;
        let api_version = if self.suppress_primary_api_version {
            String::new()
        } else {
            versions.first().cloned().unwrap_or_default()
        };
        if !api_version.is_empty() {
            versions.retain(|version| version != &api_version);
        }
        Some(ResourceRef {
            api_version,
            kind,
            api_version_candidates: versions,
            api_version_branches: self.api_version_branches,
        })
    }
}

#[derive(Default)]
pub(crate) struct OutputEvaluator {
    seen: HashSet<String>,
}

impl OutputEvaluator {
    pub(crate) fn evaluate_body(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> HelperBranchBody {
        if depth >= MAX_RECURSION_DEPTH {
            return HelperBranchBody::literals(Vec::new());
        }
        let mut literals = Vec::new();
        let mut branches = Vec::new();
        self.collect_node(
            source,
            node,
            analysis_db,
            depth + 1,
            &mut literals,
            &mut branches,
        );
        body_from_helper_parts(literals, branches)
    }

    fn evaluate_value_node(
        &mut self,
        node: tree_sitter::Node<'_>,
        source: &str,
        analysis_db: &IrAnalysisDb,
    ) -> HelperBranchBody {
        let node = unwrap_yaml_value_node(node);
        if let Some(literal) = literal_yaml_scalar(node, source) {
            return HelperBranchBody::literals(vec![literal.to_string()]);
        }
        let Ok(text) = node.utf8_text(source.as_bytes()) else {
            return HelperBranchBody::literals(Vec::new());
        };
        let exprs = parse_expr_text(text.trim());
        self.action_body(&exprs, analysis_db, 0)
            .unwrap_or_else(|| HelperBranchBody::literals(Vec::new()))
    }

    fn collect_node(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
        depth: usize,
        literals: &mut Vec<String>,
        branches: &mut Vec<HelperBranch>,
    ) {
        match node_action(source, node) {
            NodeAction::Text => {
                if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    push_nonempty(text, literals);
                }
            }
            NodeAction::Output(Some(exprs)) => {
                if let Some(body) = self.action_body(&exprs, analysis_db, depth) {
                    append_body(body, literals, branches);
                }
            }
            NodeAction::If(Some(header)) => {
                branches.extend(self.branches_from_if(source, node, &header, analysis_db, depth));
            }
            NodeAction::Range(_) | NodeAction::With(_) => {
                for field in ["body", "alternative"] {
                    for child in helm_schema_ast::children_with_field(node, field) {
                        self.collect_node(
                            source,
                            child,
                            analysis_db,
                            depth + 1,
                            literals,
                            branches,
                        );
                    }
                }
            }
            NodeAction::Suppressed | NodeAction::Assignment(_) => {}
            NodeAction::Descend | NodeAction::Output(None) | NodeAction::If(None) => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.collect_node(source, child, analysis_db, depth + 1, literals, branches);
                }
            }
        }
    }

    fn branches_from_if(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        header: &TemplateHeader,
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> Vec<HelperBranch> {
        if depth >= MAX_RECURSION_DEPTH {
            return Vec::new();
        };
        let guard = guard_from_header(header);
        let mut branches = vec![HelperBranch {
            guard: Some(guard),
            body: self.evaluate_children_with_field(
                source,
                node,
                "consequence",
                analysis_db,
                depth + 1,
            ),
        }];

        for (header, children) in else_if_pairs(node, source) {
            let body = self.evaluate_nodes(source, &children, analysis_db, depth + 1);
            if !body.is_empty() {
                branches.push(HelperBranch {
                    guard: Some(guard_from_header(&header)),
                    body,
                });
            }
        }

        let body =
            self.evaluate_children_with_field(source, node, "alternative", analysis_db, depth + 1);
        if !body.is_empty() {
            branches.push(HelperBranch { guard: None, body });
        }
        branches.retain(|branch| !branch.body.is_empty());
        branches
    }

    fn action_body(
        &mut self,
        exprs: &[TemplateExpr],
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> Option<HelperBranchBody> {
        let helper_names = helper_call_names(exprs);
        if !helper_names.is_empty() {
            let mut literals = Vec::new();
            let mut branches = Vec::new();
            for name in helper_names {
                if let Some(body) = self.with_helper_body(&name, analysis_db, |this, body| {
                    this.evaluate_body(body.source, body.tree.root_node(), analysis_db, depth + 1)
                }) {
                    append_body(body, &mut literals, &mut branches);
                }
            }
            return nonempty_body(literals, branches);
        }

        let literals =
            dedup_preserve_order(exprs.iter().flat_map(static_literal_outputs).collect());
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
    }

    fn with_helper_body<T>(
        &mut self,
        name: &str,
        analysis_db: &IrAnalysisDb,
        f: impl FnOnce(&mut Self, crate::analysis_db::ParsedHelperBody<'_>) -> T,
    ) -> Option<T> {
        if !self.seen.insert(name.to_string()) {
            return None;
        }
        let result = analysis_db
            .parsed_helper_body(name)
            .map(|body| f(self, body));
        self.seen.remove(name);
        result
    }

    fn evaluate_children_with_field(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        field: &str,
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> HelperBranchBody {
        let children = helm_schema_ast::children_with_field(node, field);
        self.evaluate_nodes(source, &children, analysis_db, depth)
    }

    fn evaluate_nodes(
        &mut self,
        source: &str,
        nodes: &[tree_sitter::Node<'_>],
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> HelperBranchBody {
        let mut literals = Vec::new();
        let mut branches = Vec::new();
        for node in nodes {
            self.collect_node(
                source,
                *node,
                analysis_db,
                depth + 1,
                &mut literals,
                &mut branches,
            );
        }
        body_from_helper_parts(literals, branches)
    }
}

fn resource_spans_for_manifest_source(
    source: &str,
    base_offset: usize,
    span_start: usize,
    span_end: usize,
    path_prefix: Vec<String>,
    analysis_db: &IrAnalysisDb,
) -> Vec<ResourceSpan> {
    let Some(resource) = detect_manifest_resource(source, analysis_db) else {
        return Vec::new();
    };
    if is_kubernetes_list_envelope(&resource) {
        return list_item_sources(source, base_offset, path_prefix)
            .into_iter()
            .flat_map(|item| {
                resource_spans_for_manifest_source(
                    item.source,
                    item.start,
                    item.start,
                    item.end,
                    item.path_prefix,
                    analysis_db,
                )
            })
            .collect();
    }
    vec![ResourceSpan {
        start: span_start,
        end: span_end,
        resource,
        path_prefix,
    }]
}

fn detect_manifest_resource(source: &str, analysis_db: &IrAnalysisDb) -> Option<ResourceRef> {
    if let Some(resource) = detect_resource_in_source(source, analysis_db) {
        return Some(resource);
    }
    let normalized = normalize_sequence_item_source(source);
    if normalized == source {
        return None;
    }
    detect_resource_in_source(&normalized, analysis_db)
}

fn resource_spans_from_source_keys(source: &str) -> Vec<ResourceSpan> {
    let mut starts = Vec::new();
    let mut pending_start = None;
    let mut pending_api_versions = Vec::new();
    let mut pending_kind = None;
    let mut byte = 0usize;

    for line in source.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        let indent = line_without_newline
            .chars()
            .take_while(|ch| *ch == ' ')
            .count();
        let trimmed = &line_without_newline[indent..];
        if indent == 0 && trimmed == "---" {
            pending_start = None;
            pending_api_versions.clear();
            pending_kind = None;
            byte += line.len();
            continue;
        }
        if indent == 0
            && let Some(key) = parse_yaml_key(trimmed).map(helm_schema_ast::ParsedYamlKey::into_key)
        {
            match key.as_str() {
                "apiVersion" => {
                    pending_start.get_or_insert(byte);
                    if let Some(value) = literal_mapping_value(trimmed) {
                        pending_api_versions
                            .push((byte, HelperBranchBody::literals(vec![value.to_string()])));
                    }
                }
                "kind" => {
                    pending_start.get_or_insert(byte);
                    pending_kind = literal_mapping_value(trimmed).map(str::to_string);
                }
                _ => {}
            }
        }
        if pending_start.is_some() && !pending_api_versions.is_empty() && pending_kind.is_some() {
            let start = pending_start.expect("checked pending start");
            let kind = pending_kind.take().expect("checked kind");
            let mut state = ResourceState::default();
            state.set_kind_if_empty(&kind);
            record_api_version_events(
                &mut state,
                source,
                std::mem::take(&mut pending_api_versions),
            );
            if let Some(resource) = state.into_resource() {
                starts.push((start, resource));
            }
            pending_start = None;
        }
        byte += line.len();
    }

    starts.sort_by_key(|(start, _)| *start);
    starts.dedup_by(|left, right| left.0 == right.0);
    let mut spans = Vec::new();
    for index in 0..starts.len() {
        spans.push(ResourceSpan {
            start: if index == 0 { 0 } else { starts[index].0 },
            end: starts
                .get(index + 1)
                .map(|(next_start, _)| *next_start)
                .unwrap_or(source.len()),
            resource: starts[index].1.clone(),
            path_prefix: Vec::new(),
        });
    }
    spans
}

fn literal_mapping_value(line: &str) -> Option<&str> {
    let colon = helm_schema_ast::first_mapping_colon_offset(line)?;
    let value = line[colon + 1..].trim();
    if value.is_empty() || value.contains("{{") || value.contains("}}") {
        return None;
    }
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .or(Some(value))
}

fn detect_resource_in_source(source: &str, analysis_db: &IrAnalysisDb) -> Option<ResourceRef> {
    let tree = parse_helm_template(source)?;
    let document = first_document_node(tree.root_node()).unwrap_or_else(|| tree.root_node());
    let mapping = top_level_mapping_node(document)?;
    let pair_kind = match mapping.kind() {
        "block_mapping" => "block_mapping_pair",
        "flow_mapping" => "flow_pair",
        "ERROR" => "block_mapping_pair",
        _ => return None,
    };

    let mut state = ResourceState::default();
    let mut api_version_events = Vec::new();
    let mut cursor = mapping.walk();
    for pair in mapping.children(&mut cursor) {
        if !pair.is_named() || pair.kind() != pair_kind {
            continue;
        }
        let Some(key) = pair.child_by_field_name("key") else {
            continue;
        };
        let Some(key) = yaml_scalar_text(key, source) else {
            continue;
        };
        let value = pair
            .child_by_field_name("value")
            .map(unwrap_yaml_value_node);
        match key {
            "apiVersion" => {
                let Some(value) = value else {
                    continue;
                };
                let output =
                    OutputEvaluator::default().evaluate_value_node(value, source, analysis_db);
                api_version_events.push((pair.start_byte(), output));
            }
            "kind" => {
                let Some(value) = value else {
                    continue;
                };
                if let Some(kind) = literal_yaml_scalar(value, source) {
                    state.set_kind_if_empty(kind);
                }
            }
            _ => {}
        }
    }
    append_literal_api_version_source_events(source, &mut api_version_events);
    record_api_version_events(&mut state, source, api_version_events);
    state.into_resource()
}

fn append_literal_api_version_source_events(
    source: &str,
    events: &mut Vec<(usize, HelperBranchBody)>,
) {
    let mut byte = 0usize;
    for line in source.split_inclusive('\n') {
        let line_without_newline = line.trim_end_matches(['\r', '\n']);
        let indent = line_without_newline
            .chars()
            .take_while(|ch| *ch == ' ')
            .count();
        let trimmed = &line_without_newline[indent..];
        if indent == 0
            && parse_yaml_key(trimmed)
                .map(helm_schema_ast::ParsedYamlKey::into_key)
                .as_deref()
                == Some("apiVersion")
            && !events.iter().any(|(event_byte, _)| *event_byte == byte)
            && let Some(value) = literal_mapping_value(trimmed)
        {
            events.push((byte, HelperBranchBody::literals(vec![value.to_string()])));
        }
        byte += line.len();
    }
}

fn is_kubernetes_list_envelope(resource: &ResourceRef) -> bool {
    resource.kind == "List"
        && resource.api_version == "v1"
        && resource.api_version_candidates.is_empty()
        && resource.api_version_branches.is_empty()
}

struct ListItemSource<'source> {
    source: &'source str,
    start: usize,
    end: usize,
    path_prefix: Vec<String>,
}

fn list_item_sources<'source>(
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
    let Some(items_sequence) = top_level_items_sequence(document, source) else {
        return Vec::new();
    };

    let mut items = Vec::new();
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
    items
}

fn document_spans(source: &str) -> Vec<(usize, usize)> {
    let Some(tree) = parse_helm_template(source) else {
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
    if let Some(first) = docs.first_mut() {
        first.0 = 0;
    }
    for index in 0..docs.len() {
        docs[index].1 = docs
            .get(index + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(source.len());
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
        "ERROR" => "block_mapping_pair",
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
        if yaml_scalar_text(key, source) == Some("items") {
            return pair.child_by_field_name("value").and_then(sequence_node);
        }
    }
    None
}

fn top_level_mapping_node(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    match node.kind() {
        "block_mapping" | "flow_mapping" => Some(node),
        "ERROR" => {
            let mut cursor = node.walk();
            if node
                .named_children(&mut cursor)
                .any(|child| child.kind() == "block_mapping_pair")
            {
                return Some(node);
            }
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find_map(top_level_mapping_node)
        }
        "document" | "block_node" | "flow_node" | "block_sequence_item" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .find_map(top_level_mapping_node)
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
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|text| text.strip_suffix('\''))
        })
        .or(Some(text))
}

fn literal_yaml_scalar<'source>(
    node: tree_sitter::Node<'_>,
    source: &'source str,
) -> Option<&'source str> {
    if matches!(node.kind(), "helm_template") {
        return None;
    }
    let text = yaml_scalar_text(node, source)?;
    (!text.contains("{{") && !text.contains("}}")).then_some(text)
}

fn normalize_sequence_item_source(source: &str) -> String {
    let mut lines = source.lines();
    let Some(first) = lines.next() else {
        return source.to_string();
    };
    let rest = lines.collect::<Vec<_>>();
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

fn body_from_parts(literals: Vec<String>, mut branches: Vec<HelperBranch>) -> HelperBranchBody {
    let literals = dedup_preserve_order(literals);
    if branches.is_empty() {
        return HelperBranchBody::literals(literals);
    }
    if !literals.is_empty() {
        branches.insert(
            0,
            HelperBranch {
                guard: None,
                body: HelperBranchBody::Literals { values: literals },
            },
        );
    }
    HelperBranchBody::Nested { branches }
}

fn body_from_helper_parts(literals: Vec<String>, branches: Vec<HelperBranch>) -> HelperBranchBody {
    if literals.is_empty() || branches.is_empty() {
        return body_from_parts(literals, branches);
    }

    let mut out = dedup_preserve_order(literals);
    let mut seen = out.iter().cloned().collect();
    for branch in branches {
        branch.body.append_all_literals(&mut out, &mut seen);
    }
    HelperBranchBody::literals(out)
}

fn nonempty_body(literals: Vec<String>, branches: Vec<HelperBranch>) -> Option<HelperBranchBody> {
    if !branches.is_empty() {
        Some(HelperBranchBody::Nested { branches })
    } else {
        let literals = dedup_preserve_order(literals);
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
    }
}

fn append_body(
    body: HelperBranchBody,
    literals: &mut Vec<String>,
    branches: &mut Vec<HelperBranch>,
) {
    match body {
        HelperBranchBody::Literals { values } => literals.extend(values),
        HelperBranchBody::Nested { branches: nested } => branches.extend(nested),
    }
}

fn push_nonempty(text: &str, out: &mut Vec<String>) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
}

fn guard_from_header(header: &TemplateHeader) -> CapabilityGuard {
    decode_guard_expr(header.expr(), header.raw()).unwrap_or_else(|| decode_guard(header.raw()))
}

enum ApiVersionEvent {
    Control(ControlEvent),
    Output(HelperBranchBody),
}

enum ControlEvent {
    If(CapabilityGuard),
    ElseIf(CapabilityGuard),
    Else,
    End,
}

enum ControlKind {
    If,
    Other,
}

struct IfFrame {
    branches: Vec<HelperBranch>,
    current_guard: Option<CapabilityGuard>,
    current_literals: Vec<String>,
    current_branches: Vec<HelperBranch>,
}

impl IfFrame {
    fn new(guard: CapabilityGuard) -> Self {
        Self {
            branches: Vec::new(),
            current_guard: Some(guard),
            current_literals: Vec::new(),
            current_branches: Vec::new(),
        }
    }

    fn start_branch(&mut self, guard: Option<CapabilityGuard>) {
        self.finish_current_branch();
        self.current_guard = guard;
    }

    fn append(&mut self, body: HelperBranchBody) {
        append_body(body, &mut self.current_literals, &mut self.current_branches);
    }

    fn finish_current_branch(&mut self) {
        if !self.current_literals.is_empty() || !self.current_branches.is_empty() {
            self.branches.push(HelperBranch {
                guard: self.current_guard.clone(),
                body: body_from_parts(
                    std::mem::take(&mut self.current_literals),
                    std::mem::take(&mut self.current_branches),
                ),
            });
        }
    }

    fn finish(mut self) -> Option<HelperBranchBody> {
        self.finish_current_branch();
        match self.branches.len() {
            0 => None,
            1 => self.branches.into_iter().next().map(|branch| branch.body),
            _ => Some(HelperBranchBody::Nested {
                branches: self.branches,
            }),
        }
    }
}

fn record_api_version_events(
    state: &mut ResourceState,
    source: &str,
    api_versions: Vec<(usize, HelperBranchBody)>,
) {
    if api_versions.is_empty() {
        return;
    }

    let mut events = control_events(source);
    events.extend(
        api_versions
            .into_iter()
            .map(|(byte, body)| (byte, ApiVersionEvent::Output(body))),
    );
    events.sort_by_key(|(byte, event)| {
        let rank = match event {
            ApiVersionEvent::Control(_) => 0,
            ApiVersionEvent::Output(_) => 1,
        };
        (*byte, rank)
    });
    let mut stack: Vec<IfFrame> = Vec::new();
    for (_byte, event) in events {
        match event {
            ApiVersionEvent::Output(body) => append_api_version_body(state, &mut stack, body),
            ApiVersionEvent::Control(ControlEvent::If(guard)) => stack.push(IfFrame::new(guard)),
            ApiVersionEvent::Control(ControlEvent::ElseIf(guard)) => {
                if let Some(frame) = stack.last_mut() {
                    frame.start_branch(Some(guard));
                }
            }
            ApiVersionEvent::Control(ControlEvent::Else) => {
                if let Some(frame) = stack.last_mut() {
                    frame.start_branch(None);
                }
            }
            ApiVersionEvent::Control(ControlEvent::End) => {
                if let Some(frame) = stack.pop()
                    && let Some(body) = frame.finish()
                {
                    append_control_frame_body(state, &mut stack, body);
                }
            }
        }
    }

    while let Some(frame) = stack.pop() {
        if let Some(body) = frame.finish() {
            append_control_frame_body(state, &mut stack, body);
        }
    }
}

fn append_api_version_body(
    state: &mut ResourceState,
    stack: &mut [IfFrame],
    body: HelperBranchBody,
) {
    if let Some(frame) = stack.last_mut() {
        frame.append(body);
    } else {
        state.record_api_version_output(body);
    }
}

fn append_control_frame_body(
    state: &mut ResourceState,
    stack: &mut [IfFrame],
    body: HelperBranchBody,
) {
    if let Some(frame) = stack.last_mut() {
        frame.append(body);
        return;
    }

    match body {
        HelperBranchBody::Literals { values } => {
            state.record_api_version_output(HelperBranchBody::Literals { values });
        }
        HelperBranchBody::Nested { branches } => state.record_api_version_branches(branches),
    }
}

fn control_events(source: &str) -> Vec<(usize, ApiVersionEvent)> {
    let mut events = Vec::new();
    let mut controls = Vec::new();
    let mut cursor = 0usize;
    while let Some(open_rel) = source[cursor..].find("{{") {
        let open = cursor + open_rel;
        let Some(close_rel) = source[open + 2..].find("}}") else {
            break;
        };
        let close = open + 2 + close_rel;
        let text = normalize_action_text(&source[open + 2..close]);
        match text.as_str() {
            "else" if matches!(controls.last(), Some(ControlKind::If)) => {
                events.push((open, ApiVersionEvent::Control(ControlEvent::Else)));
            }
            "end" => {
                if let Some(ControlKind::If) = controls.pop() {
                    events.push((open, ApiVersionEvent::Control(ControlEvent::End)));
                }
            }
            _ if text.starts_with("else if ")
                && matches!(controls.last(), Some(ControlKind::If)) =>
            {
                let header = TemplateHeader::parse_control(text);
                events.push((
                    open,
                    ApiVersionEvent::Control(ControlEvent::ElseIf(guard_from_header(&header))),
                ));
            }
            _ if text.starts_with("if ") => {
                let header = TemplateHeader::parse_control(text);
                controls.push(ControlKind::If);
                events.push((
                    open,
                    ApiVersionEvent::Control(ControlEvent::If(guard_from_header(&header))),
                ));
            }
            _ if text.starts_with("with ") || text.starts_with("range ") => {
                controls.push(ControlKind::Other);
            }
            _ => {}
        }
        cursor = close + 2;
    }
    events
}

fn normalize_action_text(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('-')
        .trim()
        .trim_end_matches('-')
        .trim()
        .to_string()
}

fn else_if_pairs<'node>(
    node: tree_sitter::Node<'node>,
    source: &str,
) -> Vec<(TemplateHeader, Vec<tree_sitter::Node<'node>>)> {
    let mut pairs = Vec::new();
    let mut seen_main_condition = false;
    let mut walker = node.walk();
    if !walker.goto_first_child() {
        return pairs;
    }

    loop {
        let child = walker.node();
        match walker.field_name() {
            Some("condition") => {
                if seen_main_condition {
                    if let Ok(text) = child.utf8_text(source.as_bytes()) {
                        pairs.push((TemplateHeader::parse_control(text.trim()), Vec::new()));
                    }
                } else {
                    seen_main_condition = true;
                }
            }
            Some("option") => {
                if let Some((_condition, option_children)) = pairs.last_mut() {
                    option_children.push(child);
                }
            }
            _ => {}
        }
        if !walker.goto_next_sibling() {
            break;
        }
    }

    pairs
}

fn helper_call_names(exprs: &[TemplateExpr]) -> Vec<String> {
    let mut out = Vec::new();
    for expr in exprs {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            let Some(name) = literal_helper_call_callee(function, args) else {
                return;
            };
            if !name.is_empty() && !out.iter().any(|existing| existing == name) {
                out.push(name.to_string());
            }
        });
    }
    out
}

fn dedup_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let trimmed = item.trim().to_string();
        if !trimmed.is_empty() && seen.insert(trimmed.clone()) {
            out.push(trimmed);
        }
    }
    out
}

fn static_literal_outputs(expr: &TemplateExpr) -> Vec<String> {
    let Some(value) = eval_expr(expr, &EvalEnv::default()).value else {
        return Vec::new();
    };
    let strings = value.strings();
    if strings.len() == 1 {
        strings.into_iter().collect()
    } else {
        Vec::new()
    }
}
