use std::collections::HashSet;

use helm_schema_ast::{
    ResourceSpan, TemplateExpr, TemplateHeader, decode_guard, decode_guard_expr, parse_expr_text,
    parse_go_template, parse_helm_template,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, Predicate, ResourceRef};

use crate::YamlPath;
use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::node_eval::{
    BranchOutcome, NodeAction, NodeActionEffectSink, NodeEvalRuntime, eval_template_body,
    node_action,
};

const MAX_RECURSION_DEPTH: usize = 12;

pub(crate) fn collect_resource_spans(
    source: &str,
    analysis_db: &IrAnalysisDb,
) -> Vec<ResourceSpan> {
    let mut spans = Vec::new();
    for (start, end) in source_document_spans(source) {
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
pub(crate) struct OutputEvaluator {
    seen: HashSet<String>,
}

#[derive(Clone, Copy)]
enum BodyOutputMode {
    WholeHelper,
    ApiVersionHeader,
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
        self.evaluate_template_body(
            source,
            node,
            analysis_db,
            depth,
            BodyOutputMode::WholeHelper,
        )
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

    fn evaluate_template_body(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
        depth: usize,
        mode: BodyOutputMode,
    ) -> HelperBranchBody {
        let mut runtime = ResourceOutputRuntime {
            evaluator: self,
            source,
            analysis_db,
            depth,
            mode,
            parts: OutputParts::default(),
            no_output_depth: 0,
        };
        eval_template_body(&mut runtime, node);
        runtime.into_body()
    }

    fn evaluate_resource_output(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
    ) -> (Option<String>, HelperBranchBody) {
        let mut runtime = ResourceOutputRuntime {
            evaluator: self,
            source,
            analysis_db,
            depth: 0,
            mode: BodyOutputMode::ApiVersionHeader,
            parts: OutputParts::default(),
            no_output_depth: 0,
        };
        eval_template_body(&mut runtime, node);
        runtime.into_output()
    }
}

#[derive(Clone, Default)]
struct OutputParts {
    literals: Vec<String>,
    branches: Vec<HelperBranch>,
    kind: Option<String>,
}

impl OutputParts {
    fn append_body(&mut self, body: HelperBranchBody) {
        match body {
            HelperBranchBody::Literals { values } => self.literals.extend(values),
            HelperBranchBody::Nested { branches } => self.branches.extend(branches),
        }
    }

    fn append_parts(&mut self, other: Self) {
        self.literals.extend(other.literals);
        self.branches.extend(other.branches);
        if self.kind.is_none() {
            self.kind = other.kind;
        }
    }

    fn delta_after(&self, base: &Self) -> Self {
        Self {
            literals: self
                .literals
                .iter()
                .skip(base.literals.len())
                .cloned()
                .collect(),
            branches: self
                .branches
                .iter()
                .skip(base.branches.len())
                .cloned()
                .collect(),
            kind: base.kind.is_none().then(|| self.kind.clone()).flatten(),
        }
    }

    fn is_empty(&self) -> bool {
        self.literals.is_empty() && self.branches.is_empty()
    }

    fn into_body(self, mode: BodyOutputMode) -> HelperBranchBody {
        match mode {
            BodyOutputMode::WholeHelper => body_from_helper_parts(self.literals, self.branches),
            BodyOutputMode::ApiVersionHeader => body_from_parts(self.literals, self.branches),
        }
    }
}

#[derive(Clone)]
struct ResourceOutputSnapshot {
    parts: OutputParts,
    no_output_depth: usize,
}

#[derive(Clone)]
struct ResourceConditionPlan {
    output_guard: Option<CapabilityGuard>,
}

struct ResourceOutputRuntime<'a, 'source> {
    evaluator: &'a mut OutputEvaluator,
    source: &'source str,
    analysis_db: &'a IrAnalysisDb,
    depth: usize,
    mode: BodyOutputMode,
    parts: OutputParts,
    no_output_depth: usize,
}

impl ResourceOutputRuntime<'_, '_> {
    fn into_body(self) -> HelperBranchBody {
        self.parts.into_body(self.mode)
    }

    fn into_output(self) -> (Option<String>, HelperBranchBody) {
        let kind = self.parts.kind.clone();
        (kind, self.into_body())
    }
}

impl NodeActionEffectSink for ResourceOutputRuntime<'_, '_> {
    fn push_predicate_if_absent(&mut self, _predicate: Predicate) {}

    fn push_dot_binding(&mut self, _binding: Option<AbstractValue>) {}
}

impl NodeEvalRuntime for ResourceOutputRuntime<'_, '_> {
    type ScopeSnapshot = ResourceOutputSnapshot;
    type ConditionPlan = ResourceConditionPlan;
    type RangePlan = ();

    fn source(&self) -> &str {
        self.source
    }

    fn enter_node(&mut self, node: tree_sitter::Node<'_>) {
        if self.no_output_depth > 0 {
            return;
        }
        match node_action(self.source, node) {
            NodeAction::Text if matches!(self.mode, BodyOutputMode::WholeHelper) => {
                if let Ok(text) = node.utf8_text(self.source.as_bytes()) {
                    push_nonempty(text, &mut self.parts.literals);
                }
            }
            NodeAction::Text => {
                for body in api_version_outputs_in_span(
                    self.source,
                    node.start_byte(),
                    node.end_byte(),
                    self.analysis_db,
                ) {
                    self.parts.append_body(body);
                }
                if self.parts.kind.is_none()
                    && let Some(kind) =
                        header_lines_in_span(self.source, node.start_byte(), node.end_byte())
                            .find_map(|line| header_line_value(line, "kind"))
                {
                    self.parts.kind = Some(unquote_yaml_scalar(kind).to_string());
                }
            }
            NodeAction::Suppressed
            | NodeAction::Assignment(_)
            | NodeAction::If(_)
            | NodeAction::With(_)
            | NodeAction::Range(_)
            | NodeAction::Output(_)
            | NodeAction::Descend => {}
        }
    }

    fn scope_snapshot(&self) -> Self::ScopeSnapshot {
        ResourceOutputSnapshot {
            parts: self.parts.clone(),
            no_output_depth: self.no_output_depth,
        }
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        self.parts = snapshot.parts;
        self.no_output_depth = snapshot.no_output_depth;
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.parts = entry.parts.clone();
        for outcome in outcomes {
            self.parts
                .append_parts(outcome.parts.delta_after(&entry.parts));
        }
        self.no_output_depth = entry.no_output_depth;
    }

    fn join_condition_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        branches: Vec<BranchOutcome<Self::ConditionPlan, Self::ScopeSnapshot>>,
    ) {
        let has_output_guards = branches.iter().any(|branch| {
            branch
                .plan
                .as_ref()
                .and_then(|plan| plan.output_guard.as_ref())
                .is_some()
        });
        if !has_output_guards {
            self.join_branch_scopes(
                entry,
                branches.into_iter().map(|branch| branch.outcome).collect(),
            );
            return;
        }

        self.parts = entry.parts.clone();
        for branch in branches {
            if self.parts.kind.is_none() {
                self.parts.kind = branch.outcome.parts.kind.clone();
            }
            let parts = branch.outcome.parts.delta_after(&entry.parts);
            if parts.is_empty() {
                continue;
            }
            self.parts.branches.push(HelperBranch {
                guard: branch.plan.and_then(|plan| plan.output_guard),
                body: parts.into_body(self.mode),
            });
        }
        self.no_output_depth = entry.no_output_depth;
    }

    fn enter_no_output(&mut self) {
        self.no_output_depth += 1;
    }

    fn exit_no_output(&mut self) {
        self.no_output_depth = self.no_output_depth.saturating_sub(1);
    }

    fn handle_output_node(&mut self, _node: tree_sitter::Node<'_>, exprs: &[TemplateExpr]) {
        if self.no_output_depth > 0 || !matches!(self.mode, BodyOutputMode::WholeHelper) {
            return;
        }
        if let Some(body) = self
            .evaluator
            .action_body(exprs, self.analysis_db, self.depth)
        {
            self.parts.append_body(body);
        }
    }

    fn plan_if_condition(&mut self, header: &TemplateHeader) -> Self::ConditionPlan {
        ResourceConditionPlan {
            output_guard: Some(guard_from_header(header)),
        }
    }

    fn activate_if_condition(&mut self, _plan: &Self::ConditionPlan) {}

    fn plan_with_condition(&mut self, _header: &TemplateHeader) -> Self::ConditionPlan {
        ResourceConditionPlan { output_guard: None }
    }

    fn activate_with_condition(&mut self, _plan: &Self::ConditionPlan) {}

    fn activate_condition_alternative(&mut self, _plan: &Self::ConditionPlan) {}

    fn plan_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        _header: Option<&TemplateHeader>,
        _current_path: &YamlPath,
        _mapping_entry_path: Option<&YamlPath>,
    ) -> Self::RangePlan {
    }

    fn activate_range_action(
        &mut self,
        _node: tree_sitter::Node<'_>,
        _plan: &Self::RangePlan,
        _current_path: &YamlPath,
    ) {
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

fn detect_resource_in_source(source: &str, analysis_db: &IrAnalysisDb) -> Option<ResourceRef> {
    let tree = parse_go_template(source)?;
    let (kind, api_version_output) =
        OutputEvaluator::default().evaluate_resource_output(source, tree.root_node(), analysis_db);
    resource_from_parts(kind, api_version_output)
}

fn resource_from_parts(
    kind: Option<String>,
    api_version_output: HelperBranchBody,
) -> Option<ResourceRef> {
    let kind = kind.filter(|kind| !kind.is_empty())?;
    let mut api_versions = Vec::new();
    let mut api_version_branches = Vec::new();
    record_api_version_output(
        api_version_output,
        &mut api_versions,
        &mut api_version_branches,
    );
    let api_version = api_versions.first().cloned().unwrap_or_default();
    if !api_version.is_empty() {
        api_versions.retain(|version| version != &api_version);
    }
    Some(ResourceRef {
        api_version,
        kind,
        api_version_candidates: api_versions,
        api_version_branches,
    })
}

fn record_api_version_output(
    output: HelperBranchBody,
    versions: &mut Vec<String>,
    branches: &mut Vec<HelperBranch>,
) {
    match output {
        HelperBranchBody::Literals { values } => insert_api_versions(values, versions),
        HelperBranchBody::Nested { branches: nested } => {
            record_api_version_branches(nested, versions, branches);
        }
    }
}

fn record_api_version_branches(
    nested: Vec<HelperBranch>,
    versions: &mut Vec<String>,
    branches: &mut Vec<HelperBranch>,
) {
    if nested.is_empty() {
        return;
    }
    if nested.len() == 1 {
        record_api_version_output(
            nested.into_iter().next().expect("single branch").body,
            versions,
            branches,
        );
        return;
    }
    for branch in &nested {
        insert_api_versions(branch.body.all_literals(), versions);
    }
    branches.extend(nested);
}

fn insert_api_versions(values: impl IntoIterator<Item = String>, versions: &mut Vec<String>) {
    for value in values {
        if !value.is_empty() && !versions.contains(&value) {
            versions.push(value);
        }
    }
}

fn api_version_outputs_in_span(
    source: &str,
    start: usize,
    end: usize,
    analysis_db: &IrAnalysisDb,
) -> Vec<HelperBranchBody> {
    header_lines_in_span(source, start, end)
        .filter_map(|line| {
            let value = header_line_value(line, "apiVersion")?;
            Some(api_version_body_from_header_value(value, analysis_db))
        })
        .collect()
}

fn api_version_body_from_header_value(value: &str, analysis_db: &IrAnalysisDb) -> HelperBranchBody {
    if value.contains("{{") || value.contains("}}") {
        let exprs = parse_expr_text(value);
        return OutputEvaluator::default()
            .action_body(&exprs, analysis_db, 0)
            .unwrap_or_else(|| HelperBranchBody::literals(Vec::new()));
    }
    HelperBranchBody::literals(vec![unquote_yaml_scalar(value).to_string()])
}

fn header_lines_in_span(source: &str, start: usize, end: usize) -> impl Iterator<Item = &str> {
    let mut byte = 0usize;
    source.split_inclusive('\n').filter_map(move |line| {
        let line_start = byte;
        byte += line.len();
        (start <= line_start && line_start < end).then_some(line.trim_end_matches(['\r', '\n']))
    })
}

fn header_line_value<'source>(line: &'source str, key: &str) -> Option<&'source str> {
    let trimmed = line.trim_start();
    if line.len() != trimmed.len() || trimmed.starts_with('#') {
        return None;
    }
    let colon = helm_schema_ast::first_mapping_colon_offset(trimmed)?;
    (trimmed[..colon].trim() == key)
        .then(|| trimmed[colon + 1..].trim())
        .filter(|value| !value.is_empty())
}

fn unquote_yaml_scalar(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
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

fn source_document_spans(source: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut start = 0usize;
    let mut byte = 0usize;
    for line in source.split_inclusive('\n') {
        if line.trim() == "---" {
            if start < byte {
                spans.push((start, byte));
            }
            start = byte + line.len();
        }
        byte += line.len();
    }
    if start < source.len() {
        spans.push((start, source.len()));
    }
    spans
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
