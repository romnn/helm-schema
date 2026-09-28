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
    decode_guard_expr, decode_header_guard, span_expressions, unquote_yaml_scalar,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, ResourceRef, ValuesPath};
use helm_schema_syntax::{
    ControlKind, ControlRegion, MappingEntry, Node as CstNode, ScalarPart, ScalarParts, Span,
    TemplatedDocument,
};

use crate::analysis_db::IrAnalysisDb;
use crate::eval_env::EvalEnv;
use crate::expr_eval::{eval_expr, literal_helper_call_callee};
use crate::node_eval::{NodeAction, else_if_pairs, node_action};

const MAX_RECURSION_DEPTH: usize = 12;

/// One parsed template body as identity recovery reads it: the source text,
/// its Go-template tree, and the analysis database that resolves helpers.
#[derive(Clone, Copy)]
struct IdentitySource<'a> {
    text: &'a str,
    root: tree_sitter::Node<'a>,
    db: &'a IrAnalysisDb,
}

pub(crate) fn collect_resource_spans(
    document: &TemplatedDocument<'_>,
    root: tree_sitter::Node<'_>,
    analysis_db: &IrAnalysisDb,
) -> Vec<ResourceSpan> {
    let source = IdentitySource {
        text: document.source(),
        root,
        db: analysis_db,
    };
    let roots = sorted_nodes(document.roots());
    let mut spans = Vec::new();
    for window in document.document_spans() {
        collect_window_spans(&roots, *window, source, Vec::new(), &mut spans);
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
    source: IdentitySource<'_>,
    path_prefix: Vec<String>,
    out: &mut Vec<ResourceSpan>,
) {
    let Some(top_indent) = min_entry_indent(nodes, window) else {
        return;
    };
    let mut parts = HeaderParts::default();
    collect_header_parts(nodes, window, top_indent, source, &mut parts);
    // Some arm writes a kind no literal covers, beside resolved ones: the
    // resolved kinds are not exhaustive, so identity proves no kind.
    if parts.kind_unresolved && parts.kind.is_some() {
        return;
    }
    let mut kind_selector = None;
    if parts.kind.is_none()
        && let Some(selector) = parts.kind_selector.as_deref()
    {
        let mut candidates = Vec::new();
        collect_kind_partition_literals(nodes, window, source, selector, &mut candidates);
        if !candidates.is_empty() {
            parts.kind = Some(candidates.remove(0));
            parts.kind_candidates.extend(candidates);
            kind_selector = Some(ValuesPath::parse(selector));
        }
    }
    if let Some(selector) = parts.kind_selected_by.as_deref() {
        kind_selector = Some(ValuesPath::parse(selector));
    }
    let kind = parts.kind.take();
    let kind_candidates = std::mem::take(&mut parts.kind_candidates);
    let mut kind_branch_sources = std::mem::take(&mut parts.kind_branch_sources);
    let resource_kinds = kind
        .iter()
        .chain(&kind_candidates)
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let branch_kinds = kind_branch_sources
        .iter()
        .map(|branch| branch.kind.as_str())
        .collect::<HashSet<_>>();
    if branch_kinds != resource_kinds {
        kind_branch_sources.clear();
    }
    let Some(mut resource) =
        resource_from_parts(kind, kind_candidates, parts.into_api_version_body())
    else {
        return;
    };
    resource.kind_selector = kind_selector;
    if is_kubernetes_list_envelope(&resource) {
        let Some(entry) = items_entry(nodes, window, source.text) else {
            return;
        };
        for item in entry.sequence_items() {
            if !starts_in(item.span.start, window) {
                continue;
            }
            let children = sorted_nodes(&item.children);
            let mut item_prefix = path_prefix.clone();
            item_prefix.push("items[*]".to_string());
            collect_window_spans(&children, item.content_span(), source, item_prefix, out);
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
/// literal, guarded arms carry their parsed condition bound in the
/// document's own dot (see [`HelperArm`]), and the chain ends
/// in an unguarded `else` — without it some render states produce no kind
/// at all and the recorded partition would be incomplete. Capability-guarded
/// arms abstain: their liveness is an oracle question, not a values
/// predicate.
fn inline_kind_branch_sources(arms: &[HelperArm]) -> Vec<KindBranchSource> {
    if arms.len() < 2 {
        return Vec::new();
    }
    let mut sources = Vec::new();
    for (index, arm) in arms.iter().enumerate() {
        let HelperBranchBody::Literals { values } = &arm.branch.body else {
            return Vec::new();
        };
        let [kind] = values.as_slice() else {
            return Vec::new();
        };
        if kind.is_empty() {
            return Vec::new();
        }
        let last = index == arms.len() - 1;
        let condition = match (&arm.branch.guard, &arm.condition, last) {
            (Some(CapabilityGuard::Opaque { text }), Some(condition), false)
                if !text.trim().is_empty() =>
            {
                Some(condition.clone())
            }
            (None, _, true) => None,
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
    /// The values path an `if` chain selects the kind by: every arm that
    /// writes a kind is guarded by `eq <path> "<that kind>"`, except that a
    /// trailing `else` may write one more kind.
    kind_selected_by: Option<String>,
    /// A `kind:` entry was written whose scalar resolves to no literal: the
    /// resolved kinds then do not cover every render.
    kind_unresolved: bool,
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
    source: IdentitySource<'_>,
    parts: &mut HeaderParts,
) {
    for node in nodes {
        if !node_intersects(node, window) {
            continue;
        }
        match node {
            CstNode::Mapping(entry) => {
                if entry.indent == top_indent && starts_in(entry.span.start, window) {
                    capture_header_entry(entry, source, parts);
                }
                // Layout recovery can hang same-indent siblings under an
                // open entry (ill-nested regions); the line scan this walk
                // replaces saw those header lines, so descend for them.
                let children = sorted_nodes(&entry.children);
                collect_header_parts(&children, window, top_indent, source, parts);
            }
            CstNode::Sequence(item) => {
                let children = sorted_nodes(&item.children);
                collect_header_parts(&children, window, top_indent, source, parts);
            }
            CstNode::Control(region) => match region.kind {
                // Define/block bodies render nothing at document scope.
                ControlKind::Define | ControlKind::Block => {}
                ControlKind::If => {
                    collect_if_region(region, window, top_indent, source, parts);
                }
                // `with`/`range` bodies contribute headers unguarded (the
                // branch structure never carried apiVersion guards).
                ControlKind::With | ControlKind::Range => {
                    for branch in &region.branches {
                        let children = sorted_nodes(&branch.body);
                        collect_header_parts(&children, window, top_indent, source, parts);
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
    source: IdentitySource<'_>,
    parts: &mut HeaderParts,
) {
    let kind_before = parts.kind.is_some() || !parts.kind_candidates.is_empty();
    let mut selector: Option<String> = None;
    let mut selected_kinds: Vec<String> = Vec::new();
    let mut proven = true;
    let mut kind_arms = 0;
    let mut inherited = false;
    for (index, branch) in region.branches.iter().enumerate() {
        let mut sub = HeaderParts::default();
        let children = sorted_nodes(&branch.body);
        collect_header_parts(&children, window, top_indent, source, &mut sub);
        kind_arms += usize::from(sub.kind.is_some() || sub.kind_unresolved);
        if sub.kind_unresolved {
            // An arm writing a kind no literal resolves: nothing proves
            // which kind renders there.
            proven = false;
            parts.kind_unresolved = true;
        }
        if let Some(kind) = &sub.kind
            && let Some(path) = &sub.kind_selected_by
        {
            // A nested chain already selects every kind of this arm by `path`.
            inherited = true;
            selected_kinds.push(kind.clone());
            selected_kinds.extend(sub.kind_candidates.iter().cloned());
            proven &= selector.get_or_insert_with(|| path.clone()) == path;
        } else if let Some(kind) = &sub.kind {
            let arm_selector = branch_condition_header(source, branch.header)
                .map(|header| kind_equality_selector(header.expr(), kind));
            let trailing_else = index > 0 && index + 1 == region.branches.len();
            proven &= sub.kind_candidates.is_empty()
                && match arm_selector {
                    Some(Some(path)) => {
                        selected_kinds.push(kind.clone());
                        selector.get_or_insert_with(|| path.clone()) == &path
                    }
                    Some(None) => false,
                    // The trailing `else` renders its kind exactly where no
                    // earlier arm's literal matched.
                    None => trailing_else && !selected_kinds.contains(kind),
                };
        }
        let sub_kind_branch_sources = std::mem::take(&mut sub.kind_branch_sources);
        if let Some(kind) = sub.kind.take() {
            if parts.kind.is_none() {
                parts.kind = Some(kind);
                parts.kind_branch_sources = sub_kind_branch_sources;
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
            guard: branch_condition_header(source, branch.header)
                .map(|header| decode_header_guard(&header)),
            body: sub.into_api_version_body(),
        });
    }
    // A nested chain proves its path only as the region's sole kind-writing
    // arm: any other arm renders its kind whatever that path holds.
    proven &= !inherited || kind_arms == 1;
    if !kind_before && proven && !in_rebinding_action(source, region.span.start) {
        parts.kind_selected_by = selector;
    }
}

/// Whether `byte` renders inside a `with` or `range` action, which rebinds
/// the dot there. A kind's arm conditions and chain selector lower in the
/// scope of the rows the kind types, not the header's, so a kind written
/// under a rebound dot proves neither. Read from the Go-template tree: the
/// YAML layout can let a header entry escape an ill-nested region.
fn in_rebinding_action(source: IdentitySource<'_>, byte: usize) -> bool {
    let mut node = source.root.descendant_for_byte_range(byte, byte);
    while let Some(current) = node {
        if matches!(current.kind(), "with_action" | "range_action") {
            return true;
        }
        node = current.parent();
    }
    false
}

/// The values path of an `eq <path> "<kind>"` condition (either operand
/// order) whose literal is exactly `kind`.
fn kind_equality_selector(condition: &TemplateExpr, kind: &str) -> Option<String> {
    let TemplateExpr::Call { function, args } = condition.deparen() else {
        return None;
    };
    if function != "eq" {
        return None;
    }
    let (path, value) = values_path_literal_comparison(args)?;
    (value == kind).then_some(path)
}

/// A two-operand comparison of a direct values path with a string literal,
/// in either order: the path and the literal.
fn values_path_literal_comparison(args: &[TemplateExpr]) -> Option<(String, &String)> {
    let [left, right] = args else {
        return None;
    };
    match (left.deparen(), right.deparen()) {
        (path, TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)))
        | (TemplateExpr::Literal(Literal::String(value) | Literal::RawString(value)), path) => {
            Some((crate::expr_eval::direct_values_path(path)?, value))
        }
        _ => None,
    }
}

fn capture_header_entry(entry: &MappingEntry, source: IdentitySource<'_>, parts: &mut HeaderParts) {
    let Some(key) = source.text.get(entry.key.span.start..entry.key.span.end) else {
        return;
    };
    let Some(value) = entry.value.as_ref().filter(|value| {
        source
            .text
            .get(value.span.start..value.span.end)
            .is_some_and(|text| !text.trim().is_empty())
    }) else {
        return;
    };
    match key.trim() {
        "apiVersion" => parts.append_body(scalar_value_body(value, source).0),
        "kind" => {
            let (body, kind_branch_sources) = scalar_value_body(value, source);
            // An inline conditional selecting between literal kinds
            // keeps its per-arm parsed guards beside the flat candidate
            // list: the evaluator later lowers them into predicates the
            // builder can match row conjunctions against.
            if parts.kind.is_none()
                && matches!(body, HelperBranchBody::Nested { .. })
                && !in_rebinding_action(source, entry.span.start)
            {
                parts.kind_branch_sources = kind_branch_sources;
            }
            let mut kinds = body.all_literals();
            kinds.retain(|kind| !kind.is_empty());
            parts.kind_unresolved |= kinds.is_empty();
            if parts.kind.is_none() && !kinds.is_empty() {
                parts.kind = Some(kinds.remove(0));
            }
            for kind in kinds {
                if parts.kind.as_ref() != Some(&kind) && !parts.kind_candidates.contains(&kind) {
                    parts.kind_candidates.push(kind);
                }
            }
            // Under a rebound dot the scalar reads the region's values, not
            // the path the document's comparisons name: no selector, so no
            // harvested candidates either.
            if parts.kind.is_none() && !in_rebinding_action(source, entry.span.start) {
                parts.kind_selector = span_expressions(source.root, source.text, value.span)
                    .iter()
                    .find_map(crate::expr_eval::direct_values_path);
            }
        }
        _ => {}
    }
}

fn collect_kind_partition_literals(
    nodes: &[&CstNode],
    window: Span,
    source: IdentitySource<'_>,
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
                    if let Some(header) = branch_condition_header(source, branch.header) {
                        header.expr().walk(|expr| {
                            let TemplateExpr::Call { function, args } = expr.deparen() else {
                                return;
                            };
                            if !matches!(function.as_str(), "eq" | "ne") {
                                return;
                            }
                            if let Some((path, candidate)) = values_path_literal_comparison(args)
                                && path == selector
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

/// A header scalar's output, with the per-arm sources of an inline kind
/// conditional: a plain scalar is its own unquoted literal, and a templated
/// one evaluates the template nodes inside its span.
fn scalar_value_body(
    value: &ScalarParts,
    source: IdentitySource<'_>,
) -> (HelperBranchBody, Vec<KindBranchSource>) {
    let templated = value
        .parts
        .iter()
        .any(|part| matches!(part, ScalarPart::Hole(_)));
    if !templated {
        let text = source
            .text
            .get(value.span.start..value.span.end)
            .unwrap_or_default();
        return (
            HelperBranchBody::literals(vec![unquote_yaml_scalar(text).to_string()]),
            Vec::new(),
        );
    }
    let mut parts = HelperParts::default();
    HelperOutputEvaluator::default().collect_span_parts(
        source,
        source.root,
        value.span,
        &mut parts,
    );
    let kind_branch_sources = inline_kind_branch_sources(&parts.arms);
    (parts.into_body(), kind_branch_sources)
}

/// The parsed condition of an `{{ if … }}` / `{{ else if … }}` branch
/// header; `None` for a bare `{{ else }}` and for other control headers.
fn branch_condition_header(source: IdentitySource<'_>, header: Span) -> Option<TemplateHeader> {
    let node = source
        .root
        .descendant_for_byte_range(header.start, header.end)
        .filter(|node| node.kind() == "if_action")?;
    let mut cursor = node.walk();
    let condition = node
        .children_by_field_name("condition", &mut cursor)
        .find(|child| header.start <= child.start_byte() && child.end_byte() <= header.end)?;
    Some(TemplateHeader::from_node(condition, source.text))
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
        kind_selector: None,
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

/// An evaluated template's output: candidate literals and guarded arms.
#[derive(Default)]
pub(crate) struct HelperParts {
    literals: Vec<String>,
    arms: Vec<HelperArm>,
}

/// One guarded branch of an evaluated output. An `if` arm keeps its parsed
/// condition beside the decoded guard while that condition binds in the
/// evaluated template's own dot: also through a helper called with that
/// dot (`include "kind" .`), but not through one handed another context,
/// nor inside a `with`/`range` body that rebinds the dot. Nested and
/// ternary branches carry none.
struct HelperArm {
    branch: HelperBranch,
    condition: Option<TemplateExpr>,
}

impl HelperParts {
    fn append_body(&mut self, body: HelperBranchBody) {
        match body {
            HelperBranchBody::Literals { values } => self.literals.extend(values),
            HelperBranchBody::Nested { branches } => {
                self.arms
                    .extend(branches.into_iter().map(|branch| HelperArm {
                        branch,
                        condition: None,
                    }));
            }
        }
    }

    /// Append a called helper's whole output. A mixed-content body (literal
    /// text around branches) is not a pure delegation, so it flattens to
    /// candidate literals; a branch-only body keeps its arms.
    fn append_helper_output(&mut self, output: HelperParts) {
        if output.arms.is_empty() {
            self.literals.extend(dedup_preserve_order(output.literals));
        } else if output.literals.is_empty() {
            self.arms.extend(output.arms);
        } else {
            self.append_body(output.into_body());
        }
    }

    fn extend(&mut self, other: HelperParts) {
        self.literals.extend(other.literals);
        self.arms.extend(other.arms);
    }

    /// Forget the arms' conditions: they bind in a dot other than the
    /// consuming template's.
    fn unbind_conditions(&mut self) {
        for arm in &mut self.arms {
            arm.condition = None;
        }
    }

    /// Record one template text run: it lands in a YAML scalar position of
    /// the consuming document, so a literal written as `"policy/v1"`
    /// (quotes included) denotes the unquoted scalar once the composed
    /// manifest is parsed.
    fn push_text(&mut self, text: &str) {
        let trimmed = unquote_yaml_scalar(text.trim());
        if !trimmed.is_empty() {
            self.literals.push(trimmed.to_string());
        }
    }

    fn is_empty(&self) -> bool {
        self.literals.is_empty() && self.arms.is_empty()
    }

    fn into_branches(self) -> (Vec<String>, Vec<HelperBranch>) {
        let branches = self.arms.into_iter().map(|arm| arm.branch).collect();
        (self.literals, branches)
    }

    pub(crate) fn into_body(self) -> HelperBranchBody {
        let (literals, branches) = self.into_branches();
        body_from_helper_parts(literals, branches)
    }
}

impl HelperOutputEvaluator {
    /// A template body's output parts; depth-capped (empty at the cap).
    pub(crate) fn body_parts(
        &mut self,
        source: &str,
        node: tree_sitter::Node<'_>,
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> HelperParts {
        let mut parts = HelperParts::default();
        if depth < MAX_RECURSION_DEPTH {
            self.collect_body_parts(source, node, analysis_db, depth, &mut parts);
        }
        parts
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
                    parts.push_text(text);
                }
            }
            // Assignment right-hand sides render nothing; nested defines are
            // suppressed bodies.
            NodeAction::Suppressed | NodeAction::Assignment(_) => {}
            NodeAction::Output(exprs) => {
                let output = self.action_parts(&exprs, analysis_db, depth);
                parts.extend(output);
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
                    parts.arms.push(HelperArm {
                        branch: HelperBranch {
                            guard: arm_header.as_ref().map(decode_header_guard),
                            body: sub.into_body(),
                        },
                        condition: arm_header.map(|header| header.expr().clone()),
                    });
                }
            }
            // `with`/`range` branch bodies contribute unguarded; the bodies
            // that rebind the dot unbind their arms' conditions, while the
            // `else` alternative runs in the enclosing dot.
            NodeAction::With(_) => {
                let mut rebound = HelperParts::default();
                for child in children_with_field(node, "consequence") {
                    self.collect_body_parts(source, child, analysis_db, depth, &mut rebound);
                }
                for (_, children) in else_if_pairs(node, source) {
                    for child in children {
                        self.collect_body_parts(source, child, analysis_db, depth, &mut rebound);
                    }
                }
                rebound.unbind_conditions();
                parts.extend(rebound);
                for child in children_with_field(node, "alternative") {
                    self.collect_body_parts(source, child, analysis_db, depth, parts);
                }
            }
            NodeAction::Range(_) => {
                let mut rebound = HelperParts::default();
                for child in children_with_field(node, "body") {
                    self.collect_body_parts(source, child, analysis_db, depth, &mut rebound);
                }
                rebound.unbind_conditions();
                parts.extend(rebound);
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

    /// Evaluate the template nodes inside `span` of the tree below `node`: a
    /// text run crossing the span contributes the part inside it, and a node
    /// enclosing the span contributes its children inside it.
    fn collect_span_parts(
        &mut self,
        source: IdentitySource<'_>,
        node: tree_sitter::Node<'_>,
        span: Span,
        parts: &mut HelperParts,
    ) {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        for child in children {
            if child.end_byte() <= span.start || child.start_byte() >= span.end {
                continue;
            }
            if span.start <= child.start_byte() && child.end_byte() <= span.end {
                self.collect_body_parts(source.text, child, source.db, 0, parts);
            } else if matches!(child.kind(), "text" | "yaml_no_injection_text") {
                let start = child.start_byte().max(span.start);
                let end = child.end_byte().min(span.end);
                parts.push_text(source.text.get(start..end).unwrap_or_default());
            } else {
                self.collect_span_parts(source, child, span, parts);
            }
        }
    }

    /// One output action's parts. Called helpers contribute their whole
    /// outputs; once any of them branches, their literals drop out.
    fn action_parts(
        &mut self,
        exprs: &[TemplateExpr],
        analysis_db: &IrAnalysisDb,
        depth: usize,
    ) -> HelperParts {
        let mut parts = HelperParts::default();
        let calls = helper_calls(exprs);
        if !calls.is_empty() {
            for call in calls {
                if let Some(mut output) =
                    self.with_helper_body(&call.name, analysis_db, |this, body| {
                        this.body_parts(body.source, body.tree.root_node(), analysis_db, depth + 1)
                    })
                {
                    if !call.passes_dot {
                        output.unbind_conditions();
                    }
                    parts.append_helper_output(output);
                }
            }
            if parts.arms.is_empty() {
                parts.literals = dedup_preserve_order(parts.literals);
            } else {
                parts.literals.clear();
            }
            return parts;
        }

        if let Some(body) = capability_ternary_body(exprs) {
            parts.append_body(body);
            return parts;
        }

        parts.literals =
            dedup_preserve_order(exprs.iter().flat_map(static_literal_outputs).collect());
        parts
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

/// One called helper of an output action.
struct HelperCall {
    name: String,
    /// Every call of this helper in the action hands it the caller's own
    /// dot (`include "kind" .`), so the helper's dot-relative reads bind as
    /// the caller's do.
    passes_dot: bool,
}

/// The helpers an output action calls, once each, in first-call order.
fn helper_calls(exprs: &[TemplateExpr]) -> Vec<HelperCall> {
    let mut out: Vec<HelperCall> = Vec::new();
    for expr in exprs {
        expr.walk(|node| {
            let TemplateExpr::Call { function, args } = node else {
                return;
            };
            let Some(name) = literal_helper_call_callee(function, args) else {
                return;
            };
            if name.is_empty() {
                return;
            }
            let passes_dot = matches!(
                args.as_slice(),
                [_, context] if *context.deparen() == TemplateExpr::Field(Vec::new())
            );
            match out.iter_mut().find(|call| call.name == name) {
                Some(call) => call.passes_dot &= passes_dot,
                None => out.push(HelperCall {
                    name: name.to_string(),
                    passes_dot,
                }),
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
