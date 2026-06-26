use std::collections::HashSet;

use helm_schema_ast::{
    DefineIndex, HelmAst, HelmParser, ResourceSpan, TemplateAction, TemplateExpr, TreeSitterParser,
    decode_guard, decode_guard_expr, parse_helm_template,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, ResourceRef};

use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};

const MAX_RECURSION_DEPTH: usize = 12;

pub(crate) fn collect_resource_spans(source: &str, defines: &DefineIndex) -> Vec<ResourceSpan> {
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
            defines,
        ));
    }
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
    });
    spans
}

pub(crate) fn helper_body_defines_resource(name: &str, defines: &DefineIndex) -> bool {
    let Some(body) = defines.get(name) else {
        return false;
    };
    let ast = HelmAst::Document {
        items: body.to_vec(),
    };
    ResourceIdentityDetector::new(defines)
        .detect(&ast)
        .is_some()
}

pub(crate) struct ResourceIdentityDetector<'a> {
    defines: &'a DefineIndex,
}

impl<'a> ResourceIdentityDetector<'a> {
    pub(crate) fn new(defines: &'a DefineIndex) -> Self {
        Self { defines }
    }

    pub(crate) fn detect(&self, ast: &HelmAst) -> Option<ResourceRef> {
        let mut state = ResourceState::default();
        self.scan_node(ast, &mut state, true);
        state.into_resource()
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
            HelmAst::Pair { key, value } => match scalar_text(key) {
                Some("apiVersion") => {
                    let Some(value) = value.as_deref() else {
                        return;
                    };
                    let output = OutputEvaluator::new().evaluate_body(
                        std::slice::from_ref(value),
                        None,
                        self.defines,
                        0,
                    );
                    state.record_api_version_output(output);
                }
                Some("kind") => {
                    if let Some(kind) = value.as_deref().and_then(scalar_text) {
                        state.set_kind_if_empty(kind);
                    }
                }
                _ => {}
            },
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => {
                if capture_branches
                    && let Some(branches) = OutputEvaluator::new().branches_from_if(
                        node,
                        Some("apiVersion"),
                        self.defines,
                        0,
                    )
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
            HelmAst::Block { body, .. } => self.scan_items(body, state, capture_branches),
            HelmAst::Define { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::HelmComment { .. } => {}
        }
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
    fn set_kind_if_empty(&mut self, kind: &str) {
        if self.kind.is_none() && !kind.is_empty() {
            self.kind = Some(kind.to_string());
        }
    }

    fn record_api_version_output(&mut self, output: HelperBranchBody) {
        match output {
            HelperBranchBody::Literals { values } => {
                self.multi_branch |= values.len() > 1;
                for value in values {
                    self.insert_api_version(value);
                }
            }
            HelperBranchBody::Nested { branches } => self.record_api_version_branches(branches),
        }
    }

    fn record_api_version_branches(&mut self, branches: Vec<HelperBranch>) {
        if branches.is_empty() {
            return;
        }
        self.multi_branch = true;
        for branch in &branches {
            for value in branch.body.all_literals() {
                self.insert_api_version(value);
            }
        }
        self.api_version_branches.extend(branches);
    }

    fn insert_api_version(&mut self, value: String) {
        if !value.is_empty() && !self.api_versions.contains(&value) {
            self.api_versions.push(value);
        }
    }

    fn into_resource(self) -> Option<ResourceRef> {
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

pub(crate) struct OutputEvaluator {
    seen: HashSet<String>,
}

impl OutputEvaluator {
    pub(crate) fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    pub(crate) fn evaluate_body(
        &mut self,
        body: &[HelmAst],
        key_filter: Option<&str>,
        defines: &DefineIndex,
        depth: usize,
    ) -> HelperBranchBody {
        if depth >= MAX_RECURSION_DEPTH {
            return HelperBranchBody::literals(Vec::new());
        }
        if key_filter.is_none()
            && let Some(branches) = self.promoted_branches(body, defines, depth)
        {
            return HelperBranchBody::Nested { branches };
        }

        let mut literals = Vec::new();
        let mut branches = Vec::new();
        for node in body {
            self.collect_node(
                node,
                key_filter,
                defines,
                depth + 1,
                &mut literals,
                &mut branches,
            );
        }
        body_from_parts(literals, branches)
    }

    fn collect_node(
        &mut self,
        node: &HelmAst,
        key_filter: Option<&str>,
        defines: &DefineIndex,
        depth: usize,
        literals: &mut Vec<String>,
        branches: &mut Vec<HelperBranch>,
    ) {
        match node {
            HelmAst::Scalar { text } if key_filter.is_none() => push_nonempty(text, literals),
            HelmAst::HelmExpr { action } if key_filter.is_none() => {
                if let Some(body) = self.action_body(action, defines, depth) {
                    append_body(body, literals, branches, false);
                }
            }
            HelmAst::Pair { key, value }
                if key_filter.is_none() || key_filter == scalar_text(key) =>
            {
                if let Some(value) = value.as_deref() {
                    let body =
                        self.evaluate_body(std::slice::from_ref(value), None, defines, depth);
                    append_body(body, literals, branches, key_filter.is_some());
                }
            }
            HelmAst::If {
                then_branch,
                else_branch,
                ..
            } => {
                if let Some(key) = key_filter {
                    if let Some(found) = self.branches_from_if(node, Some(key), defines, depth) {
                        branches.extend(found);
                    } else {
                        self.collect_nodes(
                            then_branch,
                            Some(key),
                            defines,
                            depth,
                            literals,
                            branches,
                        );
                        self.collect_nodes(
                            else_branch,
                            Some(key),
                            defines,
                            depth,
                            literals,
                            branches,
                        );
                    }
                } else {
                    self.collect_nodes(then_branch, None, defines, depth, literals, branches);
                    self.collect_nodes(else_branch, None, defines, depth, literals, branches);
                }
            }
            HelmAst::Range {
                body, else_branch, ..
            }
            | HelmAst::With {
                body, else_branch, ..
            } => {
                self.collect_nodes(body, key_filter, defines, depth, literals, branches);
                self.collect_nodes(else_branch, key_filter, defines, depth, literals, branches);
            }
            HelmAst::Document { items } | HelmAst::Mapping { items } => {
                self.collect_nodes(items, key_filter, defines, depth, literals, branches);
            }
            HelmAst::Sequence { items } if key_filter.is_none() => {
                self.collect_nodes(items, None, defines, depth, literals, branches);
            }
            HelmAst::Define { body, .. } if key_filter.is_none() => {
                self.collect_nodes(body, None, defines, depth, literals, branches);
            }
            HelmAst::Block { body, .. } => {
                self.collect_nodes(body, key_filter, defines, depth, literals, branches);
            }
            HelmAst::Scalar { .. }
            | HelmAst::HelmExpr { .. }
            | HelmAst::Pair { .. }
            | HelmAst::Sequence { .. }
            | HelmAst::Define { .. }
            | HelmAst::HelmComment { .. } => {}
        }
    }

    fn collect_nodes(
        &mut self,
        nodes: &[HelmAst],
        key_filter: Option<&str>,
        defines: &DefineIndex,
        depth: usize,
        literals: &mut Vec<String>,
        branches: &mut Vec<HelperBranch>,
    ) {
        for node in nodes {
            self.collect_node(node, key_filter, defines, depth + 1, literals, branches);
        }
    }

    fn promoted_branches(
        &mut self,
        body: &[HelmAst],
        defines: &DefineIndex,
        depth: usize,
    ) -> Option<Vec<HelperBranch>> {
        match single_significant_node(body)? {
            HelmAst::If { .. } => {
                let branches =
                    self.branches_from_if(single_significant_node(body)?, None, defines, depth)?;
                let has_capability_guard = branches
                    .iter()
                    .any(|branch| guard_is_capability(&branch.guard));
                let has_literals = branches.iter().any(|branch| !branch.body.is_empty());
                (has_capability_guard && has_literals).then_some(branches)
            }
            HelmAst::HelmExpr { action } => {
                let callee = pure_helper_call(action)?;
                self.with_helper_body(&callee, defines, |this, body| {
                    this.promoted_branches(body, defines, depth + 1)
                })
                .flatten()
            }
            _ => None,
        }
    }

    fn branches_from_if(
        &mut self,
        node: &HelmAst,
        key_filter: Option<&str>,
        defines: &DefineIndex,
        depth: usize,
    ) -> Option<Vec<HelperBranch>> {
        if depth >= MAX_RECURSION_DEPTH {
            return None;
        }
        let HelmAst::If {
            condition,
            then_branch,
            else_branch,
        } = node
        else {
            return None;
        };
        let guard = decode_guard_expr(condition.expr(), condition.raw())
            .unwrap_or_else(|| decode_guard(condition.raw()));
        if key_filter.is_some() && !guard_is_capability(&Some(guard.clone())) {
            return None;
        }

        let mut branches = vec![HelperBranch {
            guard: Some(guard),
            body: self.evaluate_body(then_branch, key_filter, defines, depth + 1),
        }];
        if let Some(nested_if) = single_if(else_branch) {
            branches.extend(
                self.branches_from_if(nested_if, key_filter, defines, depth + 1)
                    .unwrap_or_default(),
            );
        } else if !else_branch.is_empty() {
            let body = self.evaluate_body(else_branch, key_filter, defines, depth + 1);
            if !body.is_empty() {
                branches.push(HelperBranch { guard: None, body });
            }
        }
        if key_filter.is_some() {
            branches.retain(|branch| !branch.body.is_empty());
        }
        (!branches.is_empty()).then_some(branches)
    }

    fn action_body(
        &mut self,
        action: &TemplateAction,
        defines: &DefineIndex,
        depth: usize,
    ) -> Option<HelperBranchBody> {
        let helper_names = helper_call_names(action);
        if !helper_names.is_empty() {
            let mut literals = Vec::new();
            let mut branches = Vec::new();
            for name in helper_names {
                if let Some(body) = self.with_helper_body(&name, defines, |this, body| {
                    this.evaluate_body(body, None, defines, depth + 1)
                }) {
                    append_body(body, &mut literals, &mut branches, true);
                }
            }
            return nonempty_body(literals, branches);
        }

        let literals = dedup_preserve_order(
            action
                .exprs()
                .iter()
                .flat_map(static_literal_outputs)
                .collect(),
        );
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
    }

    fn with_helper_body<T>(
        &mut self,
        name: &str,
        defines: &DefineIndex,
        f: impl FnOnce(&mut Self, &[HelmAst]) -> T,
    ) -> Option<T> {
        if !self.seen.insert(name.to_string()) {
            return None;
        }
        let result = defines.get(name).map(|body| f(self, body));
        self.seen.remove(name);
        result
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
        return list_item_sources(source, base_offset, path_prefix)
            .into_iter()
            .flat_map(|item| {
                resource_spans_for_manifest_source(
                    item.source,
                    item.start,
                    item.start,
                    item.end,
                    item.path_prefix,
                    defines,
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

fn detect_manifest_resource(source: &str, defines: &DefineIndex) -> Option<ResourceRef> {
    if let Some(resource) = TreeSitterParser
        .parse(source)
        .ok()
        .and_then(|ast| ResourceIdentityDetector::new(defines).detect(&ast))
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
        .and_then(|ast| ResourceIdentityDetector::new(defines).detect(&ast))
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
        "document" | "block_node" | "flow_node" | "block_sequence_item" => {
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
    text.strip_prefix('"')
        .and_then(|text| text.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|text| text.strip_suffix('\''))
        })
        .or(Some(text))
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
        branches.insert(0, HelperBranch::with_literals(None, literals));
    }
    HelperBranchBody::Nested { branches }
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
    preserve_nested: bool,
) {
    match body {
        HelperBranchBody::Literals { values } => literals.extend(values),
        HelperBranchBody::Nested { branches: nested } if preserve_nested => {
            branches.extend(nested);
        }
        HelperBranchBody::Nested { branches: nested } => {
            let mut seen = HashSet::new();
            for branch in nested {
                branch.body.append_all_literals(literals, &mut seen);
            }
        }
    }
}

fn single_significant_node(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let mut found = None;
    for node in nodes {
        match node {
            HelmAst::Scalar { text } if text.trim().is_empty() => {}
            HelmAst::HelmComment { .. } => {}
            _ if found.is_none() => found = Some(node),
            _ => return None,
        }
    }
    found
}

fn single_if(nodes: &[HelmAst]) -> Option<&HelmAst> {
    let node = single_significant_node(nodes)?;
    matches!(node, HelmAst::If { .. }).then_some(node)
}

fn scalar_text(node: &HelmAst) -> Option<&str> {
    match node {
        HelmAst::Scalar { text } => Some(text.trim()),
        _ => None,
    }
}

fn push_nonempty(text: &str, out: &mut Vec<String>) {
    let trimmed = text.trim();
    if !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
}

fn guard_is_capability(guard: &Option<CapabilityGuard>) -> bool {
    matches!(
        guard,
        Some(CapabilityGuard::Has { .. }) | Some(CapabilityGuard::NotHas { .. })
    )
}

fn pure_helper_call(action: &TemplateAction) -> Option<String> {
    let [TemplateExpr::Call { function, args }] = action.exprs() else {
        return None;
    };
    literal_helper_call_callee(function, args).map(str::to_string)
}

fn helper_call_names(action: &TemplateAction) -> Vec<String> {
    let mut out = Vec::new();
    for expr in action.exprs() {
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
