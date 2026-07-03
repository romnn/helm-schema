use std::collections::HashSet;

use helm_schema_ast::{
    ResourceSpan, TemplateExpr, TemplateHeader, decode_guard, decode_guard_expr, parse_expr_text,
    parse_go_template, unquote_yaml_scalar,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, ResourceRef};
use helm_schema_syntax::{MappingEntry, Node as CstNode, TemplatedDocument};

use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::node_eval::{
    BranchOutcome, NodeAction, NodeEvalRuntime, eval_template_body, node_action,
};

const MAX_RECURSION_DEPTH: usize = 12;

pub(crate) fn collect_resource_spans(
    document: &TemplatedDocument<'_>,
    analysis_db: &IrAnalysisDb,
) -> Vec<ResourceSpan> {
    let source = document.source();
    let mut spans = Vec::new();
    for span in document.document_spans() {
        let Some(document_source) = source.get(span.start..span.end) else {
            continue;
        };
        spans.extend(resource_spans_for_manifest_source(
            document_source,
            span.start,
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
        self.evaluate_output(
            source,
            node,
            analysis_db,
            depth,
            BodyOutputMode::WholeHelper,
        )
        .1
    }

    fn action_body(
        &mut self,
        exprs: &[TemplateExpr],
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> Option<HelperBranchBody> {
        let helper_names = helper_call_names(exprs);
        if !helper_names.is_empty() {
            let mut parts = OutputParts::default();
            for name in helper_names {
                if let Some(body) = self.with_helper_body(&name, analysis_db, |this, body| {
                    this.evaluate_body(body.source, body.tree.root_node(), analysis_db, depth + 1)
                }) {
                    parts.append_body(body);
                }
            }
            return nonempty_body(parts.literals, parts.branches);
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

    fn evaluate_output(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
        depth: usize,
        mode: BodyOutputMode,
    ) -> (Option<String>, HelperBranchBody) {
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
        let kind = runtime.parts.kind.take();
        (kind, runtime.parts.into_body(mode))
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

impl NodeEvalRuntime for ResourceOutputRuntime<'_, '_> {
    /// `no_output_depth` is deliberately not part of the snapshot: the only
    /// enter/exit_no_output caller (`eval_assignment_node`) is strictly
    /// balanced, so the depth at every snapshot/restore point already equals
    /// the current value.
    type ScopeSnapshot = OutputParts;
    type ConditionPlan = ResourceConditionPlan;

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
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        self.parts.literals.push(trimmed.to_string());
                    }
                }
            }
            NodeAction::Text => {
                for line in header_lines_in_span(self.source, node.start_byte(), node.end_byte()) {
                    if let Some(value) = header_line_value(line, "apiVersion") {
                        self.parts.append_body(api_version_body_from_header_value(
                            value,
                            self.analysis_db,
                        ));
                    }
                    if self.parts.kind.is_none()
                        && let Some(kind) = header_line_value(line, "kind")
                    {
                        self.parts.kind = Some(unquote_yaml_scalar(kind).to_string());
                    }
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
        self.parts.clone()
    }

    fn restore_scope(&mut self, snapshot: Self::ScopeSnapshot) {
        self.parts = snapshot;
    }

    fn join_branch_scopes(
        &mut self,
        entry: &Self::ScopeSnapshot,
        outcomes: Vec<Self::ScopeSnapshot>,
    ) {
        self.parts = entry.clone();
        for outcome in outcomes {
            self.parts.append_parts(outcome.delta_after(entry));
        }
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

        self.parts = entry.clone();
        for branch in branches {
            if self.parts.kind.is_none() {
                self.parts.kind = branch.outcome.kind.clone();
            }
            let parts = branch.outcome.delta_after(entry);
            if parts.is_empty() {
                continue;
            }
            self.parts.branches.push(HelperBranch {
                guard: branch.plan.and_then(|plan| plan.output_guard),
                body: parts.into_body(self.mode),
            });
        }
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

    fn enter_if_condition(&mut self, header: &TemplateHeader) -> Self::ConditionPlan {
        let guard = decode_guard_expr(header.expr(), header.raw())
            .unwrap_or_else(|| decode_guard(header.raw()));
        ResourceConditionPlan {
            output_guard: Some(guard),
        }
    }

    fn enter_with_condition(&mut self, _header: &TemplateHeader) -> Self::ConditionPlan {
        ResourceConditionPlan { output_guard: None }
    }

    fn activate_condition_alternative(&mut self, _plan: &Self::ConditionPlan) {}
}

fn resource_spans_for_manifest_source(
    source: &str,
    base_offset: usize,
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
                    item.path_prefix,
                    analysis_db,
                )
            })
            .collect();
    }
    vec![ResourceSpan {
        start: base_offset,
        end: base_offset + source.len(),
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
    let (kind, api_version_output) = OutputEvaluator::default().evaluate_output(
        source,
        tree.root_node(),
        analysis_db,
        0,
        BodyOutputMode::ApiVersionHeader,
    );
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
    let nested = match output {
        HelperBranchBody::Literals { values } => return insert_api_versions(values, versions),
        HelperBranchBody::Nested { branches } => branches,
    };
    match nested.len() {
        0 => {}
        // A single branch carries no alternative; unwrap it into the summary.
        1 => {
            let branch = nested.into_iter().next().expect("single branch");
            record_api_version_output(branch.body, versions, branches);
        }
        _ => {
            for branch in &nested {
                insert_api_versions(branch.body.all_literals(), versions);
            }
            branches.extend(nested);
        }
    }
}

fn insert_api_versions(values: impl IntoIterator<Item = String>, versions: &mut Vec<String>) {
    for value in values {
        if !value.is_empty() && !versions.contains(&value) {
            versions.push(value);
        }
    }
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

fn is_kubernetes_list_envelope(resource: &ResourceRef) -> bool {
    resource.kind == "List"
        && resource.api_version == "v1"
        && resource.api_version_candidates.is_empty()
        && resource.api_version_branches.is_empty()
}

struct ListItemSource<'source> {
    source: &'source str,
    start: usize,
    path_prefix: Vec<String>,
}

fn list_item_sources<'source>(
    source: &'source str,
    base_offset: usize,
    path_prefix: Vec<String>,
) -> Vec<ListItemSource<'source>> {
    let document = TemplatedDocument::parse(source);
    let Some(entry) = top_level_items_entry(document.roots(), source) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for item in entry.sequence_items() {
        let span = item.content_span();
        let Some(item_source) = source.get(span.start..span.end) else {
            continue;
        };
        let mut item_prefix = path_prefix.clone();
        item_prefix.push("items[*]".to_string());
        items.push(ListItemSource {
            source: item_source,
            start: base_offset + span.start,
            path_prefix: item_prefix,
        });
    }
    items
}

/// The root-level `items:` mapping entry of one manifest document. Control
/// regions overlay container structure in the CST, so the entry (or the
/// items below it) can sit inside a region branch; look through branches.
fn top_level_items_entry<'nodes>(
    nodes: &'nodes [CstNode],
    source: &str,
) -> Option<&'nodes MappingEntry> {
    for node in nodes {
        match node {
            CstNode::Mapping(entry) => {
                let Some(key) = source.get(entry.key.span.start..entry.key.span.end) else {
                    continue;
                };
                if unquote_yaml_scalar(key.trim()) == "items" {
                    return Some(entry);
                }
            }
            CstNode::Control(region) => {
                for branch in &region.branches {
                    if let Some(entry) = top_level_items_entry(&branch.body, source) {
                        return Some(entry);
                    }
                }
            }
            _ => {}
        }
    }
    None
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
