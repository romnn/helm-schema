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
//! Scalar identity comes from frozen records of the document's execution.
//! Source syntax supplies layout and the remaining structural selector metadata.

use std::collections::HashSet;

use helm_schema_ast::{
    KindBranchSource, Literal, ResourceSpan, TemplateExpr, TemplateHeader, decode_header_guard,
    span_expressions, unquote_yaml_scalar,
};
use helm_schema_core::{CapabilityGuard, HelperBranch, HelperBranchBody, ResourceRef, ValuesPath};
use helm_schema_syntax::{
    ControlKind, ControlRegion, MappingEntry, Node as CstNode, ScalarParts, Span, TemplatedDocument,
};

use crate::value::DocumentExecution;

/// Source layout and frozen execution used to recover one document identity.
#[derive(Clone, Copy)]
struct IdentitySource<'a> {
    text: &'a str,
    root: tree_sitter::Node<'a>,
    execution: &'a DocumentExecution,
    source_path: Option<&'a str>,
}

pub(crate) fn collect_resource_spans(
    document: &TemplatedDocument<'_>,
    root: tree_sitter::Node<'_>,
    execution: &DocumentExecution,
    source_path: Option<&str>,
) -> Vec<ResourceSpan> {
    let source = IdentitySource {
        text: document.source(),
        root,
        execution,
        source_path,
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
    // An unresolved executed kind cannot be recovered from source comparisons:
    // those comparisons may belong to an unreachable branch.
    if parts.kind_unresolved {
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
            Some((direct_values_path(path)?, value))
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
                    .find_map(direct_values_path);
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

/// Projects a header scalar from the successful execution's frozen text alternatives.
fn scalar_value_body(
    value: &ScalarParts,
    source: IdentitySource<'_>,
) -> (HelperBranchBody, Vec<KindBranchSource>) {
    let span = quoted_scalar_content(value.span, source.text);
    if source
        .execution
        .text_for_span(span, source.source_path)
        .any(|(_, text)| text.is_none())
    {
        return (HelperBranchBody::literals(Vec::new()), Vec::new());
    }
    inline_scalar_body(source, span).unwrap_or_else(|| {
        (
            executed_scalar_body(source.execution, span, source.source_path),
            Vec::new(),
        )
    })
}

/// Keeps structural inline guards beside the corresponding executed scalar output.
fn inline_scalar_body(
    source: IdentitySource<'_>,
    span: Span,
) -> Option<(HelperBranchBody, Vec<KindBranchSource>)> {
    let mut pending = vec![source.root];
    let node = loop {
        let node = pending.pop()?;
        if node.end_byte() <= span.start || span.end <= node.start_byte() {
            continue;
        }
        if node.kind() == "if_action"
            && span.start <= node.start_byte()
            && node.end_byte() <= span.end
        {
            break node;
        }
        let mut cursor = node.walk();
        pending.extend(node.children(&mut cursor));
    };
    if !source
        .text
        .get(span.start..node.start_byte())?
        .trim()
        .is_empty()
        || !source
            .text
            .get(node.end_byte()..span.end)?
            .trim()
            .is_empty()
    {
        return None;
    }
    let mut arms: Vec<(Option<TemplateHeader>, Vec<tree_sitter::Node<'_>>)> = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match cursor.field_name() {
                Some("condition") => arms.push((
                    Some(TemplateHeader::from_node(child, source.text)),
                    Vec::new(),
                )),
                Some("consequence" | "option") => arms.last_mut()?.1.push(child),
                Some("alternative") => {
                    if arms.last()?.0.is_some() {
                        arms.push((None, Vec::new()));
                    }
                    arms.last_mut()?.1.push(child);
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    let mut branches = Vec::new();
    let mut sources = Vec::new();
    let mut complete = arms.len() >= 2 && arms.last()?.0.is_none();
    for (condition, children) in arms {
        let body_span = Span::new(children.first()?.start_byte(), children.last()?.end_byte());
        let body = executed_scalar_body(source.execution, body_span, source.source_path);
        let guard = condition.as_ref().map(decode_header_guard);
        let literals = body.all_literals();
        match literals.as_slice() {
            [kind] if !kind.is_empty() => {
                complete &= guard.as_ref().is_none_or(|guard| {
                    matches!(guard, CapabilityGuard::Opaque { text } if !text.trim().is_empty())
                });
                sources.push(KindBranchSource {
                    condition: condition.map(|header| header.expr().clone()),
                    kind: kind.clone(),
                });
            }
            _ => complete = false,
        }
        branches.push(HelperBranch { guard, body });
    }
    if !complete {
        sources.clear();
    }
    Some((HelperBranchBody::Nested { branches }, sources))
}

/// Projects exact scalar alternatives without evaluating their source expressions.
pub(crate) fn executed_scalar_body(
    execution: &DocumentExecution,
    span: Span,
    source_path: Option<&str>,
) -> HelperBranchBody {
    let mut literals = Vec::new();
    for (_, text) in execution.text_for_span(span, source_path) {
        let Some(text) = text else {
            return HelperBranchBody::literals(Vec::new());
        };
        let text = unquote_yaml_scalar(text.trim()).to_string();
        if !text.is_empty() && !literals.contains(&text) {
            literals.push(text);
        }
    }
    HelperBranchBody::literals(literals)
}

/// Recognizes only syntactic access to the document's original values root.
fn direct_values_path(expr: &TemplateExpr) -> Option<String> {
    let path = match expr.deparen() {
        TemplateExpr::Field(path) => path.clone(),
        TemplateExpr::Selector { operand, path } if matches!(operand.deparen(), TemplateExpr::Variable(name) if name.is_empty()) => {
            path.clone()
        }
        _ => return None,
    };
    let (head, tail) = path.split_first()?;
    (head == "Values").then(|| ValuesPath::from_segments(tail.iter().cloned()).encode())
}

/// The content of a quoted scalar: its quotes delimit the scalar and render
/// nothing, so `"{{ .Values.apiVersion }}"` projects only the action they
/// enclose. A plain scalar is its own content.
fn quoted_scalar_content(span: Span, text: &str) -> Span {
    let Some(raw) = text.get(span.start..span.end) else {
        return span;
    };
    let trimmed = raw.trim();
    let content = unquote_yaml_scalar(trimmed);
    if content.len() == trimmed.len() {
        return span;
    }
    // Both delimiters are one-byte quotes.
    let start = span.start + (raw.len() - raw.trim_start().len()) + 1;
    Span::new(start, start + content.len())
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
