//! The fragment interpreter: one abstract evaluation of a templated-YAML
//! document over the `helm-schema-syntax` CST, producing a
//! [`Guarded<AbstractFragment>`] plus the pathless value reads that never
//! render (condition reads, assignment right-hand sides, helper-internal
//! guard reads).
//!
//! Control regions become guarded arms: each branch's contributions are
//! evaluated under the branch's decoded [`PathCondition`] and dissolve into
//! the surrounding container, so guard structure lives in the tree instead
//! of ambient per-row stacks. The interpreter still keeps a small predicate
//! stack, but only to stamp root-to-leaf guards onto the pathless reads that
//! have no tree position.
//!
//! Reused machinery (nothing here re-derives what the current pipeline
//! already knows how to compute):
//!
//! - condition decoding: [`ValuePathContext`] predicate decoding over
//!   `TemplateHeader`s,
//! - expression evaluation: the `AbstractValue` lattice with bound-helper
//!   resolution (`document_result_from_expr`), so helper calls flow through
//!   the existing memoized summarize machinery,
//! - range headers: `range_header_from_source` /
//!   `range_has_destructured_variable_definition` on the shared Go-template
//!   parse (body shape now comes from the CST instead of line scans),
//! - local state: [`SymbolicLocalState`] with the same branch-join rules as
//!   the symbolic walker.
//!
//! Known B1 boundaries (kept honest in the differential scoreboard rather
//! than patched around): document-scope ranges run one symbolic iteration
//! (the current document walker's model); destructured range variables stay
//! unbound at document scope (also the current model); static file
//! templates and exact helper-body inlining are not evaluated; ill-nested
//! regions re-adopt escaped siblings by span, so late children of a
//! branch-opened container inherit the branch guard.

use std::collections::HashMap;

use helm_schema_ast::{
    TemplateHeader, range_has_destructured_variable_definition, range_header_from_source,
};
use helm_schema_syntax as syntax;
use helm_schema_syntax::{
    Node, OpaqueKind, ScalarPart, ScalarParts, Span, TemplatedDocument, parse_go_template,
};

use crate::Guard;
use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::contract_sink::merge_guards;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_summary::HelperOutputMeta;
use crate::node_eval::control_header;
use crate::symbolic_local_state::SymbolicLocalState;
use crate::value_path_context::ValuePathContext;
use helm_schema_core::Predicate;

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, PathCondition,
    Sequence, StringPart,
};

/// The result of evaluating one template source: the abstract rendered
/// document plus the pathless value reads observed along the way.
#[derive(Debug, Default)]
pub struct EvaluatedDocument {
    /// The guarded abstract fragment for the whole source (all YAML
    /// documents merged; per-document projection is a later-stage concern).
    pub root: Guarded<AbstractFragment>,
    /// `.Values` reads that never render: condition reads, assignment
    /// right-hand sides, helper-internal guard reads, and range headers.
    pub reads: Vec<ValueRead>,
}

/// One pathless `.Values` read with the guards active at the read site.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ValueRead {
    /// The dotted `.Values` path that was read.
    pub values_path: String,
    /// The guards active at the read site (root-to-leaf, in push order).
    pub guards: Vec<Guard>,
}

/// Evaluate one template source into its abstract fragment.
pub(crate) fn eval_document(source: &str, db: &IrAnalysisDb) -> EvaluatedDocument {
    let Some(tree) = parse_go_template(source) else {
        return EvaluatedDocument::default();
    };
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let mut control_facts = HashMap::new();
    collect_control_facts(tree.root_node(), source, &mut control_facts);
    let mut inline_regions = Vec::new();
    collect_inline_regions(document.roots(), &mut inline_regions);

    let mut interpreter = Interpreter {
        source,
        db,
        control_facts,
        inline_regions,
        locals: SymbolicLocalState::default(),
        dot_stack: Vec::new(),
        root_bindings: HashMap::new(),
        active_predicates: Vec::new(),
        reads: Vec::new(),
    };
    let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
    let contributions = interpreter.eval_node_list(&roots);
    EvaluatedDocument {
        root: contributions.assemble(),
        reads: interpreter.reads,
    }
}

/// Control-header facts parsed once from the shared Go-template tree, keyed
/// by the region's opening-bracket byte (which equals the action node's
/// start byte).
pub(super) struct ControlFacts {
    pub(super) header: Option<TemplateHeader>,
    pub(super) range_destructured: bool,
}

fn collect_control_facts(
    node: tree_sitter::Node<'_>,
    source: &str,
    out: &mut HashMap<usize, ControlFacts>,
) {
    match node.kind() {
        "if_action" | "with_action" => {
            out.insert(
                node.start_byte(),
                ControlFacts {
                    header: control_header(source, node),
                    range_destructured: false,
                },
            );
        }
        "range_action" => {
            out.insert(
                node.start_byte(),
                ControlFacts {
                    header: range_header_from_source(node, source),
                    range_destructured: range_has_destructured_variable_definition(node),
                },
            );
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_control_facts(child, source, out);
    }
}

/// The byte where a container's first deeper *content* child appears (the
/// open-slot query's "mark": action lines, comments, and blanks are
/// transparent; only visible YAML lines deeper than the container mark it).
fn content_child_mark(children: &[Node], container_indent: usize) -> Option<usize> {
    children
        .iter()
        .filter_map(|child| match child {
            Node::Mapping(entry) if entry.indent > container_indent => Some(entry.span.start),
            Node::Sequence(item) if item.indent > container_indent => Some(item.span.start),
            Node::Scalar(line) if line.indent > container_indent => Some(line.span.start),
            _ => None,
        })
        .min()
}

fn collect_inline_regions(nodes: &[Node], out: &mut Vec<Span>) {
    for node in nodes {
        match node {
            Node::Opaque(opaque) if opaque.kind == OpaqueKind::InlineRegion => {
                out.push(opaque.span);
            }
            Node::Mapping(entry) => collect_inline_regions(&entry.children, out),
            Node::Sequence(item) => collect_inline_regions(&item.children, out),
            Node::Control(region) => {
                for branch in &region.branches {
                    collect_inline_regions(&branch.body, out);
                }
            }
            _ => {}
        }
    }
}

/// What one level of nodes contributes to its enclosing container.
#[derive(Default)]
pub(super) struct Contributions {
    pub(super) entries: Vec<MappingEntry>,
    pub(super) items: Vec<Guarded<AbstractFragment>>,
    pub(super) values: Guarded<AbstractFragment>,
    /// Fragment output carrying an explicit rendered indent
    /// (`… | nindent N`): it attaches to the nearest enclosing container
    /// whose indent is shallower than `N`, floating up past deeper open
    /// containers exactly like the attribution index's open-slot query.
    pub(super) floating: Vec<FloatingOutput>,
}

/// One explicitly-indented fragment output looking for its container.
pub(super) struct FloatingOutput {
    /// The rendered indent (`nindent`/`indent` width).
    pub(super) width: usize,
    /// The action's byte position (decides whether a same-indent container
    /// had already been "marked" by a deeper child when the output ran).
    pub(super) origin: usize,
    pub(super) value: Guarded<AbstractFragment>,
}

impl Contributions {
    pub(super) fn merge_entry(&mut self, key: EntryKey, value: Guarded<AbstractFragment>) {
        if let EntryKey::Literal(name) = &key
            && let Some(existing) = self.entries.iter_mut().find(
                |entry| matches!(&entry.key, EntryKey::Literal(existing_name) if existing_name == name),
            )
        {
            existing.value.extend(value);
            return;
        }
        self.entries.push(MappingEntry { key, value });
    }

    pub(super) fn push_value_arm(&mut self, arm: (PathCondition, AbstractFragment)) {
        self.values.arms.push(arm);
    }

    pub(super) fn guard_all(&mut self, condition: &PathCondition) {
        for entry in &mut self.entries {
            entry.value.guard_all(condition);
        }
        for item in &mut self.items {
            item.guard_all(condition);
        }
        self.values.guard_all(condition);
        for floating in &mut self.floating {
            floating.value.guard_all(condition);
        }
    }

    pub(super) fn extend(&mut self, other: Self) {
        for entry in other.entries {
            self.merge_entry(entry.key, entry.value);
        }
        self.items.extend(other.items);
        self.values.extend(other.values);
        self.floating.extend(other.floating);
    }

    /// Split off the floating output that renders *inside* a container of
    /// the given indent, returning it as one guarded value; shallower output
    /// keeps floating for an ancestor. A container opened without an inline
    /// value also accepts output rendered at its own indent, unless a deeper
    /// child had already "marked" it before the output ran (both rules from
    /// the open-slot query).
    pub(super) fn take_floating_below(
        &mut self,
        container_indent: usize,
        accepts_same_indent: bool,
        marked_at: Option<usize>,
    ) -> Guarded<AbstractFragment> {
        let mut attached = Guarded::empty();
        let mut keep = Vec::new();
        for floating in std::mem::take(&mut self.floating) {
            let same_indent_ok = floating.width == container_indent
                && accepts_same_indent
                && marked_at.is_none_or(|marked| marked >= floating.origin);
            if floating.width > container_indent || same_indent_ok {
                attached.extend(floating.value);
            } else {
                keep.push(floating);
            }
        }
        self.floating = keep;
        attached
    }

    pub(super) fn assemble(self) -> Guarded<AbstractFragment> {
        let mut out = Guarded::empty();
        if !self.entries.is_empty() {
            out.arms.push((
                Predicate::True,
                AbstractFragment::Mapping(Mapping {
                    entries: self.entries,
                }),
            ));
        }
        if !self.items.is_empty() {
            out.arms.push((
                Predicate::True,
                AbstractFragment::Sequence(Sequence { items: self.items }),
            ));
        }
        out.extend(self.values);
        // Floating output that never found a shallower container attaches
        // at this level.
        for floating in self.floating {
            out.extend(floating.value);
        }
        out
    }
}

/// A node reference plus an optional adoption child limit: children whose
/// spans start at or beyond the limit belong *after* the adopting control
/// region (source order) and are evaluated there instead of inside the
/// branch scope.
#[derive(Clone, Copy)]
pub(super) struct NodeView<'n> {
    pub(super) node: &'n Node,
    pub(super) child_limit: Option<usize>,
}

impl<'n> NodeView<'n> {
    pub(super) fn plain(node: &'n Node) -> Self {
        Self {
            node,
            child_limit: None,
        }
    }

    /// The node's children that evaluate in place (before the child limit).
    /// The limit propagates so deeper descendants past it stay excluded too;
    /// the adopting control region re-attaches them outside its scope.
    pub(super) fn in_scope_children(&self) -> Vec<NodeView<'n>> {
        let children = match self.node {
            Node::Mapping(entry) => &entry.children,
            Node::Sequence(item) => &item.children,
            _ => return Vec::new(),
        };
        children
            .iter()
            .filter(|child| {
                self.child_limit
                    .is_none_or(|limit| child.span_start() < limit)
            })
            .map(|child| NodeView {
                node: child,
                child_limit: self.child_limit,
            })
            .collect()
    }
}

/// One adopted escaped sibling: its bounded in-scope view plus the
/// enclosing region bound that caps this region's deferral window.
pub(super) struct Adopted<'n> {
    pub(super) view: NodeView<'n>,
    pub(super) defer_upper: Option<usize>,
}

/// One arm's decoded activation.
pub(super) enum ArmSpec {
    If(Option<TemplateHeader>),
    With(Option<TemplateHeader>),
    Range {
        header: Option<TemplateHeader>,
        destructured: bool,
    },
    Else,
}

pub(super) struct Interpreter<'a> {
    pub(super) source: &'a str,
    pub(super) db: &'a IrAnalysisDb,
    pub(super) control_facts: HashMap<usize, ControlFacts>,
    pub(super) inline_regions: Vec<Span>,
    pub(super) locals: SymbolicLocalState,
    pub(super) dot_stack: Vec<Option<AbstractValue>>,
    pub(super) root_bindings: HashMap<String, AbstractValue>,
    pub(super) active_predicates: Vec<Predicate>,
    pub(super) reads: Vec<ValueRead>,
}

impl<'a> Interpreter<'a> {
    pub(super) fn text(&self, span: Span) -> &'a str {
        self.source.get(span.start..span.end).unwrap_or("")
    }

    pub(super) fn current_dot_fragment(&self) -> Option<AbstractValue> {
        self.dot_stack.last().cloned().flatten()
    }

    pub(super) fn current_dot_binding(&self) -> Option<AbstractValue> {
        self.dot_stack
            .last()
            .and_then(|binding| binding.as_ref())
            .and_then(AbstractValue::to_current_dot_context_value)
    }

    pub(super) fn value_path_context(&self) -> ValuePathContext<'_> {
        ValuePathContext {
            root_bindings: &self.root_bindings,
            template_bindings: &self.locals.fragment_values,
            range_domains: &self.locals.range_domains,
            get_bindings: &self.locals.get_bindings,
            template_default_paths: &self.locals.default_paths,
            template_output_meta: &self.locals.output_meta,
            fragment_context: FragmentEvalContext::new(self.db),
            current_dot_fragment: self.current_dot_fragment(),
            current_dot_binding: self.current_dot_binding(),
        }
    }

    pub(super) fn ambient_guards(&self) -> Vec<Guard> {
        Predicate::contract_guard_stack(&self.active_predicates)
    }

    pub(super) fn push_predicate(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() && !self.active_predicates.contains(&predicate) {
            self.active_predicates.push(predicate);
        }
    }

    pub(super) fn push_read(&mut self, values_path: &str, extra_guards: &[Guard]) {
        if values_path.trim().is_empty() {
            return;
        }
        let mut guards = self.ambient_guards();
        merge_guards(&mut guards, extra_guards);
        let read = ValueRead {
            values_path: values_path.to_string(),
            guards,
        };
        if !self.reads.contains(&read) {
            self.reads.push(read);
        }
    }

    /// Pathless reads for a helper meta row: one read per recorded predicate
    /// branch, carrying the branch guards and defaultedness.
    pub(super) fn push_meta_reads(&mut self, values_path: &str, meta: &HelperOutputMeta) {
        let branches: Vec<Vec<Predicate>> = if meta.predicates.is_empty() {
            vec![Vec::new()]
        } else {
            meta.predicates
                .iter()
                .map(|branch| branch.iter().cloned().collect())
                .collect()
        };
        for branch in branches {
            let mut extra = Predicate::contract_guard_stack(&branch);
            if meta.defaulted {
                let default_guard = Guard::Default {
                    path: values_path.to_string(),
                };
                if !extra.contains(&default_guard) {
                    extra.push(default_guard);
                }
            }
            self.push_read(values_path, &extra);
        }
    }

    pub(super) fn eval_node_list(&mut self, nodes: &[NodeView<'_>]) -> Contributions {
        // The CST appends children escaping an ill-nested region at
        // container-close time, which can put them before the region in list
        // order; span order is document order, and adoption depends on it.
        let mut ordered: Vec<NodeView<'_>> = nodes.to_vec();
        ordered.sort_by_key(|view| view.node.span_start());
        let nodes = &ordered;
        let mut out = Contributions::default();
        let mut index = 0;
        while let Some(view) = nodes.get(index) {
            match view.node {
                Node::Control(region) => {
                    // Re-adopt siblings that escaped an ill-nested region:
                    // their spans still lie inside the region, so they belong
                    // to a branch body (with its guards and dot bindings).
                    // Children of an adopted node that start after the region
                    // end stay outside its scope via the child limit.
                    let mut adopted = Vec::new();
                    while let Some(next) = nodes.get(index + 1) {
                        if next.node.span_start() < region.span.end {
                            // In-scope evaluation is bounded by the innermost
                            // region end; deferral hands descendants past this
                            // region (but within the enclosing bound) back to
                            // this region, and the rest to the enclosing one.
                            let in_scope = next
                                .child_limit
                                .map_or(region.span.end, |limit| limit.min(region.span.end));
                            adopted.push(Adopted {
                                view: NodeView {
                                    node: next.node,
                                    child_limit: Some(in_scope),
                                },
                                defer_upper: next.child_limit,
                            });
                            index += 1;
                        } else {
                            break;
                        }
                    }
                    out.extend(self.eval_control(region, &adopted));
                }
                Node::Output(action) => {
                    let consumed = self.eval_output_with_lookahead(action, nodes, index, &mut out);
                    index += consumed;
                }
                _ => {
                    let contributions = self.eval_node(*view);
                    out.extend(contributions);
                }
            }
            index += 1;
        }
        out
    }

    /// Evaluate a standalone output action, recognizing the templated
    /// mapping-key line shape (`{{ key-expr }}: value…`) from the trailing
    /// action-line text node. Returns how many extra sibling nodes were
    /// consumed.
    fn eval_output_with_lookahead(
        &mut self,
        action: &syntax::OutputAction,
        nodes: &[NodeView<'_>],
        index: usize,
        out: &mut Contributions,
    ) -> usize {
        let key_line = nodes.get(index + 1).and_then(|next| match next.node {
            Node::Opaque(opaque) if opaque.kind == OpaqueKind::ActionLineText => {
                let text = self.text(opaque.span);
                syntax::structural_mapping_colon(text).map(|colon| {
                    (
                        opaque.span,
                        text.get(..colon).unwrap_or("").to_string(),
                        text.get(colon + 1..).unwrap_or("").to_string(),
                    )
                })
            }
            _ => None,
        });
        let Some((text_span, key_suffix, rest)) = key_line else {
            let (value, width) = self.eval_output_action(self.text(action.span));
            match width {
                Some(width) => out.floating.push(FloatingOutput {
                    width,
                    origin: action.span.start,
                    value,
                }),
                None => out.values.extend(value),
            }
            return 0;
        };

        // `{{ key }}…: …` — a dynamic mapping entry: the action (plus any
        // literal key suffix before the structural colon) is the key; the
        // inline value is the literal text after the colon or a same-line
        // action.
        let mut key_string = self.hole_string(self.text(action.span));
        if !key_suffix.is_empty() {
            key_string
                .parts
                .push(StringPart::Text([key_suffix].into_iter().collect()));
        }
        let key = EntryKey::Dynamic(key_string);
        let mut consumed = 1;
        let value = if rest.trim().is_empty() {
            match nodes.get(index + 2).map(|view| view.node) {
                Some(Node::Output(value_action))
                    if self.same_line(text_span.end, value_action.span.start) =>
                {
                    consumed = 2;
                    self.eval_entire_hole(self.text(value_action.span))
                }
                _ => Guarded::empty(),
            }
        } else if rest.trim().starts_with('|') || rest.trim().starts_with('>') {
            // `{{ key }}…: |` — a block scalar under a templated key. The
            // layout cannot open a block frame for templated keys, so the
            // body arrives as deeper-indented sibling lines; consume them as
            // the entry's render-suppressed blob.
            let key_indent = self.line_indent(action.span.start);
            let (block, block_consumed) =
                self.consume_dynamic_block_body(nodes, index + 2, key_indent);
            consumed += block_consumed;
            block
        } else {
            Guarded::unconditional(AbstractFragment::Scalar(AbstractString::literal(
                rest.trim().to_string(),
            )))
        };
        out.merge_entry(key, value);
        consumed
    }

    /// Consume the deeper-indented sibling lines forming the body of a
    /// templated-key block scalar, evaluating their holes as suppressed
    /// parts. Returns the suppressed scalar and how many nodes were
    /// consumed.
    fn consume_dynamic_block_body(
        &mut self,
        nodes: &[NodeView<'_>],
        start_index: usize,
        key_indent: usize,
    ) -> (Guarded<AbstractFragment>, usize) {
        let mut parts: Vec<StringPart> = Vec::new();
        let mut consumed = 0;
        while let Some(next) = nodes.get(start_index + consumed) {
            let node = next.node;
            if self.line_indent(node.span_start()) <= key_indent {
                break;
            }
            match node {
                Node::Output(action) => {
                    for (_, hole_parts) in self.eval_hole_parts(self.text(action.span)) {
                        parts.extend(hole_parts);
                    }
                }
                Node::Scalar(line) => {
                    for part in &line.content.parts {
                        match part {
                            ScalarPart::Text(span) => {
                                let text = self.text(*span);
                                if !text.is_empty() {
                                    parts.push(StringPart::Text(
                                        [text.to_string()].into_iter().collect(),
                                    ));
                                }
                            }
                            ScalarPart::Hole(span) => {
                                for (_, hole_parts) in self.eval_hole_parts(self.text(*span)) {
                                    parts.extend(hole_parts);
                                }
                            }
                        }
                    }
                }
                Node::Opaque(opaque) if opaque.kind == OpaqueKind::ActionLineText => {
                    let text = self.text(opaque.span);
                    if !text.is_empty() {
                        parts.push(StringPart::Text([text.to_string()].into_iter().collect()));
                    }
                }
                _ => break,
            }
            consumed += 1;
        }
        let value = Guarded::unconditional(AbstractFragment::Scalar(AbstractString {
            parts,
            suppressed: true,
        }));
        (value, consumed)
    }

    fn same_line(&self, from: usize, to: usize) -> bool {
        self.source
            .get(from..to)
            .is_some_and(|between| !between.contains('\n') && between.trim().is_empty())
    }

    /// The indentation of the line containing `byte`.
    fn line_indent(&self, byte: usize) -> usize {
        let line_start = self
            .source
            .get(..byte)
            .and_then(|prefix| prefix.rfind('\n'))
            .map_or(0, |newline| newline + 1);
        self.source
            .get(line_start..)
            .map_or(0, |line| line.len() - line.trim_start_matches(' ').len())
    }

    fn eval_node(&mut self, view: NodeView<'_>) -> Contributions {
        let mut out = Contributions::default();
        match view.node {
            Node::Mapping(entry) => {
                let key = self.entry_key(&entry.key);
                let mut value = Guarded::empty();
                if let Some(block) = &entry.block {
                    value.extend(self.eval_block_scalar(block));
                }
                if let Some(parts) = &entry.value {
                    value.extend(self.eval_scalar_parts(parts));
                }
                let children = view.in_scope_children();
                if !children.is_empty() {
                    let mut child = self.eval_node_list(&children);
                    let opened_empty = entry.value.is_none() && entry.block.is_none();
                    let marked_at = content_child_mark(&entry.children, entry.indent);
                    value.extend(child.take_floating_below(entry.indent, opened_empty, marked_at));
                    out.floating.append(&mut child.floating);
                    value.extend(child.assemble());
                }
                out.merge_entry(key, value);
            }
            Node::Sequence(item) => {
                let mut value = Guarded::empty();
                if let Some(block) = &item.block {
                    value.extend(self.eval_block_scalar(block));
                }
                if let Some(parts) = &item.value {
                    value.extend(self.eval_scalar_parts(parts));
                }
                let children = view.in_scope_children();
                if !children.is_empty() {
                    let mut child = self.eval_node_list(&children);
                    // Items never accept same-indent output (the open-slot
                    // query pushes item frames without that allowance).
                    value.extend(child.take_floating_below(item.indent, false, None));
                    out.floating.append(&mut child.floating);
                    value.extend(child.assemble());
                }
                out.items.push(value);
            }
            Node::Scalar(line) => {
                if line
                    .content
                    .parts
                    .iter()
                    .any(|part| matches!(part, ScalarPart::Hole(_)))
                {
                    let value = self.eval_scalar_parts(&line.content);
                    out.values.extend(value);
                }
            }
            Node::Opaque(opaque) if opaque.kind == OpaqueKind::Assignment => {
                self.eval_assignment_text(self.text(opaque.span));
            }
            Node::Control(_) | Node::Output(_) | Node::Comment(_) | Node::Opaque(_) => {}
        }
        out
    }

    pub(super) fn entry_key(&mut self, parts: &ScalarParts) -> EntryKey {
        let has_hole = parts
            .parts
            .iter()
            .any(|part| matches!(part, ScalarPart::Hole(_)));
        if !has_hole {
            let key = syntax::unquote_yaml_scalar(self.text(parts.span).trim()).to_string();
            if !key.is_empty() {
                return EntryKey::Literal(key);
            }
        }
        let mut string = AbstractString::default();
        for part in &parts.parts {
            match part {
                ScalarPart::Text(span) => {
                    let text = self.text(*span);
                    if !text.is_empty() {
                        string
                            .parts
                            .push(StringPart::Text([text.to_string()].into_iter().collect()));
                    }
                }
                ScalarPart::Hole(span) => {
                    let hole = self.hole_string(self.text(*span));
                    string.parts.extend(hole.parts);
                }
            }
        }
        EntryKey::Dynamic(string)
    }

    /// Evaluate a hole into flattened string parts (conditions from
    /// alternatives are dropped; used for keys, where alternatives project
    /// pathlessly anyway).
    fn hole_string(&mut self, text: &str) -> AbstractString {
        let arms = self.eval_hole_parts(text);
        AbstractString {
            parts: arms.into_iter().flat_map(|(_, parts)| parts).collect(),
            suppressed: false,
        }
    }
}
