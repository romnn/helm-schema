//! Resource identity: the kind/apiVersion of each manifest document, read
//! structurally off the templated-YAML CST.
//!
//! Documents are the CST's document windows; a document's identity comes
//! from its top-level `kind:` / `apiVersion:` mapping entries, looking
//! through control regions branch-aware (each `if` arm becomes a
//! [`HelperBranch`] whose guard decodes from the branch header, so
//! capability-gated apiVersion chains keep their exact guard trees).
//! `kind: List` / `apiVersion: v1` envelopes are transparent: the resources
//! are the `items:` sequence entries, whose emitted paths rebase below
//! `items[*]`.
//!
//! Helper-resolved apiVersion values (`apiVersion: {{ include "x" . }}`)
//! evaluate the helper body's Go-template tree directly instead of the
//! fragment summary: the branch trees need each `if` header's raw
//! capability condition ([`CapabilityGuard`], including `Opaque` texts),
//! which the fragment domain's `PathCondition` lattice intentionally does
//! not carry.

use std::collections::HashSet;

use helm_schema_ast::{
    KindBranchSource, Literal, ResourceSpan, TemplateExpr, TemplateHeader, children_with_field,
    decode_guard, decode_guard_expr, parse_expr_text, parse_go_template, unquote_yaml_scalar,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, ResourceRef};
use helm_schema_syntax::{
    ControlKind, ControlRegion, MappingEntry, Node as CstNode, Span, TemplatedDocument,
};

use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::node_eval::{NodeAction, else_if_pairs, node_action};

const MAX_RECURSION_DEPTH: usize = 12;

pub(crate) fn collect_resource_spans(
    document: &TemplatedDocument<'_>,
    analysis_db: &IrAnalysisDb,
) -> Vec<ResourceSpan> {
    let source = document.source();
    let roots = sorted_nodes(document.roots());
    let mut spans = Vec::new();
    for window in document.document_spans() {
        collect_window_spans(&roots, *window, source, Vec::new(), analysis_db, &mut spans);
    }
    spans.sort_by(|left, right| {
        left.start
            .cmp(&right.start)
            .then_with(|| left.end.cmp(&right.end))
    });
    spans
}

/// CST child lists append siblings escaping ill-nested regions at
/// container-close time; span order is document order, and first-seen
/// (kind, primary apiVersion) semantics depend on it.
fn sorted_nodes(nodes: &[CstNode]) -> Vec<&CstNode> {
    let mut out: Vec<&CstNode> = nodes.iter().collect();
    out.sort_by_key(|node| node.span_start());
    out
}

fn node_intersects(node: &CstNode, window: Span) -> bool {
    node.span_start() < window.end && node.subtree_end() > window.start
}

fn starts_in(byte: usize, window: Span) -> bool {
    window.start <= byte && byte < window.end
}

fn collect_window_spans(
    nodes: &[&CstNode],
    window: Span,
    source: &str,
    path_prefix: Vec<String>,
    analysis_db: &IrAnalysisDb,
    out: &mut Vec<ResourceSpan>,
) {
    let Some(top_indent) = min_entry_indent(nodes, window) else {
        return;
    };
    let mut parts = HeaderParts::default();
    collect_header_parts(nodes, window, top_indent, source, analysis_db, &mut parts);
    if parts.kind.is_none()
        && let Some(selector) = parts.kind_selector.as_deref()
    {
        let mut candidates = Vec::new();
        collect_kind_partition_literals(nodes, window, source, selector, &mut candidates);
        if !candidates.is_empty() {
            parts.kind = Some(candidates.remove(0));
            parts.kind_candidates.extend(candidates);
        }
    }
    let kind = parts.kind.take();
    let kind_candidates = std::mem::take(&mut parts.kind_candidates);
    let kind_branch_sources = std::mem::take(&mut parts.kind_branch_sources);
    let Some(resource) = resource_from_parts(kind, kind_candidates, parts.into_api_version_body())
    else {
        return;
    };
    if is_kubernetes_list_envelope(&resource) {
        let Some(entry) = items_entry(nodes, window, source) else {
            return;
        };
        for item in entry.sequence_items() {
            if !starts_in(item.span.start, window) {
                continue;
            }
            let children = sorted_nodes(&item.children);
            let mut item_prefix = path_prefix.clone();
            item_prefix.push("items[*]".to_string());
            collect_window_spans(
                &children,
                item.content_span(),
                source,
                item_prefix,
                analysis_db,
                out,
            );
        }
        return;
    }
    out.push(ResourceSpan {
        start: window.start,
        end: window.end,
        resource,
        path_prefix,
        kind_branch_sources,
    });
}

/// The per-arm sources of an inline-conditional `kind:` value. Only
/// complete literal chains qualify: every arm yields exactly one kind
/// literal, guarded arms carry raw condition text, and the chain ends in
/// an unguarded `else` — without it some render states produce no kind at
/// all and the recorded partition would be incomplete. Capability-guarded
/// arms abstain: their liveness is an oracle question, not a values
/// predicate.
fn inline_kind_branch_sources(branches: &[HelperBranch]) -> Vec<KindBranchSource> {
    if branches.len() < 2 {
        return Vec::new();
    }
    let mut sources = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        let HelperBranchBody::Literals { values } = &branch.body else {
            return Vec::new();
        };
        let [kind] = values.as_slice() else {
            return Vec::new();
        };
        if kind.is_empty() {
            return Vec::new();
        }
        let last = index == branches.len() - 1;
        let condition = match (&branch.guard, last) {
            (Some(CapabilityGuard::Opaque { text }), false) if !text.trim().is_empty() => {
                Some(text.clone())
            }
            (None, true) => None,
            _ => return Vec::new(),
        };
        sources.push(KindBranchSource {
            condition,
            kind: kind.clone(),
        });
    }
    sources
}

/// The document's header indent: the shallowest mapping-entry indent among
/// the window's top-level nodes (looking through control regions). Normal
/// manifests put headers at column zero; List items put them at the item's
/// content indent.
fn min_entry_indent(nodes: &[&CstNode], window: Span) -> Option<usize> {
    let mut min: Option<usize> = None;
    for node in nodes {
        if !node_intersects(node, window) {
            continue;
        }
        let candidate = match node {
            CstNode::Mapping(entry) if starts_in(entry.span.start, window) => Some(entry.indent),
            CstNode::Control(region) => region
                .branches
                .iter()
                .filter_map(|branch| {
                    let children = sorted_nodes(&branch.body);
                    min_entry_indent(&children, window)
                })
                .min(),
            _ => None,
        };
        min = match (min, candidate) {
            (Some(current), Some(new)) => Some(current.min(new)),
            (current, new) => current.or(new),
        };
    }
    min
}

/// Accumulated header facts of one document window: apiVersion literals and
/// guarded branch trees in source order, plus the first captured kind.
#[derive(Default)]
struct HeaderParts {
    literals: Vec<String>,
    branches: Vec<HelperBranch>,
    kind: Option<String>,
    kind_candidates: Vec<String>,
    kind_selector: Option<String>,
    kind_branch_sources: Vec<KindBranchSource>,
}

impl HeaderParts {
    fn append_body(&mut self, body: HelperBranchBody) {
        match body {
            HelperBranchBody::Literals { values } => self.literals.extend(values),
            HelperBranchBody::Nested { branches } => self.branches.extend(branches),
        }
    }

    fn is_empty(&self) -> bool {
        self.literals.is_empty() && self.branches.is_empty()
    }

    fn into_api_version_body(self) -> HelperBranchBody {
        body_from_parts(self.literals, self.branches)
    }
}

fn collect_header_parts(
    nodes: &[&CstNode],
    window: Span,
    top_indent: usize,
    source: &str,
    analysis_db: &IrAnalysisDb,
    parts: &mut HeaderParts,
) {
    for node in nodes {
        if !node_intersects(node, window) {
            continue;
        }
        match node {
            CstNode::Mapping(entry) => {
                if entry.indent == top_indent && starts_in(entry.span.start, window) {
                    capture_header_entry(entry, source, analysis_db, parts);
                }
                // Layout recovery can hang same-indent siblings under an
                // open entry (ill-nested regions); the line scan this walk
                // replaces saw those header lines, so descend for them.
                let children = sorted_nodes(&entry.children);
                collect_header_parts(&children, window, top_indent, source, analysis_db, parts);
            }
            CstNode::Sequence(item) => {
                let children = sorted_nodes(&item.children);
                collect_header_parts(&children, window, top_indent, source, analysis_db, parts);
            }
            CstNode::Control(region) => match region.kind {
                // Define/block bodies render nothing at document scope.
                ControlKind::Define | ControlKind::Block => {}
                ControlKind::If => {
                    collect_if_region(region, window, top_indent, source, analysis_db, parts);
                }
                // `with`/`range` bodies contribute headers unguarded (the
                // branch structure never carried apiVersion guards).
                ControlKind::With | ControlKind::Range => {
                    for branch in &region.branches {
                        let children = sorted_nodes(&branch.body);
                        collect_header_parts(
                            &children,
                            window,
                            top_indent,
                            source,
                            analysis_db,
                            parts,
                        );
                    }
                }
            },
            CstNode::Output(_) | CstNode::Comment(_) | CstNode::Scalar(_) | CstNode::Opaque(_) => {}
        }
    }
}

/// One `if` region: each arm's header contributions become a
/// [`HelperBranch`] under the arm's decoded guard (`None` for bare `else`);
/// arms without header contributions vanish. Kind is not branch-alternative
/// data: the first captured kind wins in source order.
fn collect_if_region(
    region: &ControlRegion,
    window: Span,
    top_indent: usize,
    source: &str,
    analysis_db: &IrAnalysisDb,
    parts: &mut HeaderParts,
) {
    for branch in &region.branches {
        let mut sub = HeaderParts::default();
        let children = sorted_nodes(&branch.body);
        collect_header_parts(&children, window, top_indent, source, analysis_db, &mut sub);
        if let Some(kind) = sub.kind.take() {
            if parts.kind.is_none() {
                parts.kind = Some(kind);
            } else if parts.kind.as_ref() != Some(&kind) && !parts.kind_candidates.contains(&kind) {
                parts.kind_candidates.push(kind);
            }
        }
        for candidate in &sub.kind_candidates {
            if !parts.kind_candidates.contains(candidate) {
                parts.kind_candidates.push(candidate.clone());
            }
        }
        if sub.is_empty() {
            continue;
        }
        parts.branches.push(HelperBranch {
            guard: branch_condition_guard(source, branch.header),
            body: sub.into_api_version_body(),
        });
    }
}

fn capture_header_entry(
    entry: &MappingEntry,
    source: &str,
    analysis_db: &IrAnalysisDb,
    parts: &mut HeaderParts,
) {
    let Some(key) = source.get(entry.key.span.start..entry.key.span.end) else {
        return;
    };
    let value = entry
        .value
        .as_ref()
        .and_then(|value| source.get(value.span.start..value.span.end))
        .map(str::trim)
        .filter(|text| !text.is_empty());
    match key.trim() {
        "apiVersion" => {
            if let Some(text) = value {
                parts.append_body(api_version_value_body(text, analysis_db));
            }
        }
        "kind" => {
            if let Some(text) = value {
                let body = scalar_value_body(text, analysis_db);
                // An inline conditional selecting between literal kinds
                // keeps its per-arm guard texts beside the flat candidate
                // list: the evaluator later lowers them into predicates
                // the builder can match row conjunctions against.
                if parts.kind.is_none()
                    && let HelperBranchBody::Nested { branches } = &body
                {
                    parts.kind_branch_sources = inline_kind_branch_sources(branches);
                }
                let mut kinds = body.all_literals();
                kinds.retain(|kind| !kind.is_empty());
                if parts.kind.is_none() && !kinds.is_empty() {
                    parts.kind = Some(kinds.remove(0));
                }
                for kind in kinds {
                    if parts.kind.as_ref() != Some(&kind) && !parts.kind_candidates.contains(&kind)
                    {
                        parts.kind_candidates.push(kind);
                    }
                }
                if parts.kind.is_none() {
                    parts.kind_selector = parse_expr_text(text)
                        .iter()
                        .find_map(crate::expr_eval::direct_values_path);
                }
            }
        }
        _ => {}
    }
}

fn collect_kind_partition_literals(
    nodes: &[&CstNode],
    window: Span,
    source: &str,
    selector: &str,
    out: &mut Vec<String>,
) {
    for node in nodes {
        if !node_intersects(node, window) {
            continue;
        }
        match node {
            CstNode::Control(region) => {
                for branch in &region.branches {
                    if let Some(text) = source.get(branch.header.start..branch.header.end)
                        && let Some(condition) = action_condition_text(text)
                    {
                        let header = TemplateHeader::parse_control(condition);
                        header.expr().walk(|expr| {
                            let TemplateExpr::Call { function, args } = expr.deparen() else {
                                return;
                            };
                            let [left, right] = args.as_slice() else {
                                return;
                            };
                            if !matches!(function.as_str(), "eq" | "ne") {
                                return;
                            }
                            let candidate = match (left.deparen(), right.deparen()) {
                                (
                                    path,
                                    TemplateExpr::Literal(
                                        Literal::String(value) | Literal::RawString(value),
                                    ),
                                ) if crate::expr_eval::direct_values_path(path).as_deref()
                                    == Some(selector) =>
                                {
                                    Some(value)
                                }
                                (
                                    TemplateExpr::Literal(
                                        Literal::String(value) | Literal::RawString(value),
                                    ),
                                    path,
                                ) if crate::expr_eval::direct_values_path(path).as_deref()
                                    == Some(selector) =>
                                {
                                    Some(value)
                                }
                                _ => None,
                            };
                            if let Some(candidate) = candidate
                                && !candidate.is_empty()
                                && !out.contains(candidate)
                            {
                                out.push(candidate.clone());
                            }
                        });
                    }
                    let children = sorted_nodes(&branch.body);
                    collect_kind_partition_literals(&children, window, source, selector, out);
                }
            }
            CstNode::Mapping(entry) => {
                let children = sorted_nodes(&entry.children);
                collect_kind_partition_literals(&children, window, source, selector, out);
            }
            CstNode::Sequence(item) => {
                let children = sorted_nodes(&item.children);
                collect_kind_partition_literals(&children, window, source, selector, out);
            }
            CstNode::Output(_) | CstNode::Comment(_) | CstNode::Scalar(_) | CstNode::Opaque(_) => {}
        }
    }
}

fn api_version_value_body(value: &str, analysis_db: &IrAnalysisDb) -> HelperBranchBody {
    scalar_value_body(value, analysis_db)
}

fn scalar_value_body(value: &str, analysis_db: &IrAnalysisDb) -> HelperBranchBody {
    if value.contains("{{") || value.contains("}}") {
        if let Some(tree) = parse_go_template(value) {
            return HelperOutputEvaluator::default().evaluate_body(
                value,
                tree.root_node(),
                analysis_db,
                0,
            );
        }
        return HelperBranchBody::literals(Vec::new());
    }
    HelperBranchBody::literals(vec![unquote_yaml_scalar(value).to_string()])
}

fn branch_condition_guard(source: &str, header: Span) -> Option<CapabilityGuard> {
    let text = source.get(header.start..header.end)?;
    let condition = action_condition_text(text)?;
    let header = TemplateHeader::parse_control(condition);
    Some(
        decode_guard_expr(header.expr(), header.raw())
            .unwrap_or_else(|| decode_guard(header.raw())),
    )
}

/// The condition text of an `{{ if … }}` / `{{ else if … }}` branch header
/// action; `None` for a bare `{{ else }}`.
fn action_condition_text(text: &str) -> Option<&str> {
    let mut inner = text.trim();
    if let Some(rest) = inner.strip_prefix("{{") {
        inner = rest.trim_start_matches('-').trim_start();
    }
    if let Some(rest) = inner.strip_suffix("}}") {
        inner = rest.trim_end_matches('-').trim_end();
    }
    if let Some(rest) = inner.strip_prefix("else") {
        inner = rest.trim_start();
    }
    let rest = inner.strip_prefix("if")?;
    rest.starts_with(|character: char| character.is_whitespace() || character == '(')
        .then(|| rest.trim())
}

fn is_kubernetes_list_envelope(resource: &ResourceRef) -> bool {
    resource.kind == "List"
        && resource.kind_candidates.is_empty()
        && resource.api_version == "v1"
        && resource.api_version_candidates.is_empty()
        && resource.api_version_branches.is_empty()
}

/// The window's top-level `items:` mapping entry (looking through control
/// regions, which overlay container structure).
fn items_entry<'nodes>(
    nodes: &[&'nodes CstNode],
    window: Span,
    source: &str,
) -> Option<&'nodes MappingEntry> {
    for node in nodes {
        if !node_intersects(node, window) {
            continue;
        }
        match node {
            CstNode::Mapping(entry) if starts_in(entry.span.start, window) => {
                let Some(key) = source.get(entry.key.span.start..entry.key.span.end) else {
                    continue;
                };
                if unquote_yaml_scalar(key.trim()) == "items" {
                    return Some(entry);
                }
            }
            CstNode::Control(region) => {
                for branch in &region.branches {
                    let children = sorted_nodes(&branch.body);
                    if let Some(entry) = items_entry(&children, window, source) {
                        return Some(entry);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn resource_from_parts(
    kind: Option<String>,
    kind_candidates: Vec<String>,
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
        kind_candidates,
        api_version_candidates: api_versions,
        api_version_branches,
        kind_branches: Vec::new(),
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
            if let Some(branch) = nested.into_iter().next() {
                record_api_version_output(branch.body, versions, branches);
            }
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

/// Helper-body output evaluation for apiVersion values: literal text runs
/// and statically resolvable expression outputs, with `if` chains preserved
/// as guarded branch trees and nested helper calls resolved recursively
/// (cycle-guarded, depth-capped).
#[derive(Default)]
pub(crate) struct HelperOutputEvaluator {
    seen: HashSet<String>,
}

#[derive(Default)]
struct HelperParts {
    literals: Vec<String>,
    branches: Vec<HelperBranch>,
}

impl HelperParts {
    fn append_body(&mut self, body: HelperBranchBody) {
        match body {
            HelperBranchBody::Literals { values } => self.literals.extend(values),
            HelperBranchBody::Nested { branches } => self.branches.extend(branches),
        }
    }

    fn is_empty(&self) -> bool {
        self.literals.is_empty() && self.branches.is_empty()
    }

    fn into_body(self) -> HelperBranchBody {
        body_from_helper_parts(self.literals, self.branches)
    }
}

impl HelperOutputEvaluator {
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
        let mut parts = HelperParts::default();
        self.collect_body_parts(source, node, analysis_db, depth, &mut parts);
        parts.into_body()
    }

    fn collect_body_parts(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
        depth: usize,
        parts: &mut HelperParts,
    ) {
        match node_action(source, node) {
            NodeAction::Text => {
                if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    // The helper's text lands in a YAML scalar position of
                    // the consuming document, so a body literal written as
                    // `"policy/v1"` (quotes included) denotes the unquoted
                    // scalar once the composed manifest is parsed.
                    let trimmed = unquote_yaml_scalar(text.trim());
                    if !trimmed.is_empty() {
                        parts.literals.push(trimmed.to_string());
                    }
                }
            }
            // Assignment right-hand sides render nothing; nested defines are
            // suppressed bodies.
            NodeAction::Suppressed | NodeAction::Assignment(_) => {}
            NodeAction::Output(exprs) => {
                if let Some(exprs) = exprs
                    && let Some(body) = self.action_body(&exprs, analysis_db, depth)
                {
                    parts.append_body(body);
                }
            }
            NodeAction::If(header) => {
                let mut arms = vec![(header, children_with_field(node, "consequence"))];
                arms.extend(else_if_pairs(node, source));
                arms.push((None, children_with_field(node, "alternative")));
                for (arm_header, children) in arms {
                    let mut sub = HelperParts::default();
                    for child in children {
                        self.collect_body_parts(source, child, analysis_db, depth, &mut sub);
                    }
                    if sub.is_empty() {
                        continue;
                    }
                    let guard = arm_header.as_ref().map(|header| {
                        decode_guard_expr(header.expr(), header.raw())
                            .unwrap_or_else(|| decode_guard(header.raw()))
                    });
                    parts.branches.push(HelperBranch {
                        guard,
                        body: sub.into_body(),
                    });
                }
            }
            // `with`/`range` branch bodies contribute unguarded.
            NodeAction::With(_) => {
                for child in children_with_field(node, "consequence") {
                    self.collect_body_parts(source, child, analysis_db, depth, parts);
                }
                for (_, children) in else_if_pairs(node, source) {
                    for child in children {
                        self.collect_body_parts(source, child, analysis_db, depth, parts);
                    }
                }
                for child in children_with_field(node, "alternative") {
                    self.collect_body_parts(source, child, analysis_db, depth, parts);
                }
            }
            NodeAction::Range(_) => {
                for child in children_with_field(node, "body") {
                    self.collect_body_parts(source, child, analysis_db, depth, parts);
                }
                for child in children_with_field(node, "alternative") {
                    self.collect_body_parts(source, child, analysis_db, depth, parts);
                }
            }
            NodeAction::Descend => {
                let mut cursor = node.walk();
                let children: Vec<_> = node.children(&mut cursor).collect();
                for child in children {
                    self.collect_body_parts(source, child, analysis_db, depth, parts);
                }
            }
        }
    }

    fn action_body(
        &mut self,
        exprs: &[TemplateExpr],
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> Option<HelperBranchBody> {
        let helper_names = helper_call_names(exprs);
        if !helper_names.is_empty() {
            let mut parts = HelperParts::default();
            for name in helper_names {
                if let Some(body) = self.with_helper_body(&name, analysis_db, |this, body| {
                    this.evaluate_body(body.source, body.tree.root_node(), analysis_db, depth + 1)
                }) {
                    parts.append_body(body);
                }
            }
            return nonempty_body(parts.literals, parts.branches);
        }

        if let Some(body) = capability_ternary_body(exprs) {
            return Some(body);
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

/// A whole helper's output: a mixed-content body (literal text around
/// branches) is not a pure delegation, so it flattens to candidate
/// literals; branch-only bodies keep their typed structure.
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
    if branches.is_empty() {
        let literals = dedup_preserve_order(literals);
        (!literals.is_empty()).then_some(HelperBranchBody::literals(literals))
    } else {
        Some(HelperBranchBody::Nested { branches })
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

/// `COND | ternary "on" "off"` (and `ternary "on" "off" COND`) selects one
/// of two literal scalars exactly like an `if COND`/`else` pair, so a
/// decodable capability condition yields guard-qualified branch literals
/// instead of an unresolvable two-way choice (signoz's HPA apiVersion
/// pipeline). An undecodable condition abstains — the identity stays
/// unresolved rather than fabricating an unguarded candidate pair.
fn capability_ternary_body(exprs: &[TemplateExpr]) -> Option<HelperBranchBody> {
    let [expr] = exprs else {
        return None;
    };
    let literal_string = |expr: &TemplateExpr| match expr.deparen() {
        TemplateExpr::Literal(Literal::String(text) | Literal::RawString(text)) => {
            Some(text.clone())
        }
        _ => None,
    };
    let (condition, on_true, on_false) = match expr.deparen() {
        TemplateExpr::Pipeline(stages) => {
            let (last, condition_stages) = stages.split_last()?;
            let TemplateExpr::Call { function, args } = last.deparen() else {
                return None;
            };
            let [on_true, on_false] = args.as_slice() else {
                return None;
            };
            if function != "ternary" || condition_stages.is_empty() {
                return None;
            }
            (
                TemplateExpr::Pipeline(condition_stages.to_vec()),
                literal_string(on_true)?,
                literal_string(on_false)?,
            )
        }
        TemplateExpr::Call { function, args } if function == "ternary" => {
            let [on_true, on_false, condition] = args.as_slice() else {
                return None;
            };
            (
                condition.clone(),
                literal_string(on_true)?,
                literal_string(on_false)?,
            )
        }
        _ => return None,
    };
    let guard = decode_guard_expr(&condition, "")?;
    if matches!(guard, CapabilityGuard::Opaque { .. }) {
        return None;
    }
    Some(HelperBranchBody::Nested {
        branches: vec![
            HelperBranch {
                guard: Some(guard),
                body: HelperBranchBody::literals(vec![on_true]),
            },
            HelperBranch {
                guard: None,
                body: HelperBranchBody::literals(vec![on_false]),
            },
        ],
    })
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
