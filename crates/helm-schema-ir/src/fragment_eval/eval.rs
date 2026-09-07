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
//! Reused machinery (nothing here re-derives what the pipeline already
//! knows how to compute):
//!
//! - condition decoding: [`ValuePathContext`] predicate decoding over
//!   `TemplateHeader`s,
//! - expression evaluation: the `AbstractValue` lattice with bound-helper
//!   resolution (`document_result_from_expr`), where helper calls resolve
//!   through their in-domain fragment summaries (`super::summary`),
//! - range headers: `range_header_from_source` /
//!   `range_has_destructured_variable_definition` on the shared Go-template
//!   parse (body shape comes from the CST),
//! - local state: [`SymbolicLocalState`] with shared branch-join rules.
//!
//! The same interpreter evaluates documents and helper bodies; a helper
//! scope additionally carries the call's root bindings, the resolved dot
//! pair, and the active call chain, and applies the summary lane's
//! flattening rules where the caller consumes summary facts (truthy range
//! markers, dependency-lane demotions, sibling-condition scoping).
//!
//! Known boundaries: document-scope ranges run one symbolic iteration
//! (helper scopes iterate statically known lists exactly); destructured
//! range variables stay unbound at document scope; static file templates
//! evaluate as nested fragments at output holes; ill-nested regions
//! re-adopt escaped siblings by span in both directions, so branch-window
//! content of a container opened before (or closed after) the region
//! inherits the branch guard.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{
    ResourceSpan, TemplateExpr, TemplateHeader, parse_expr_text,
    range_has_destructured_variable_definition, range_header_from_source,
};
use helm_schema_syntax as syntax;
use helm_schema_syntax::{
    ControlRegion, Node, OpaqueKind, ScalarPart, ScalarParts, Span, TemplatedDocument,
    parse_go_template,
};

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::bound_value_analysis::BoundValueContext;
use crate::eval_effect::{CaptureKind, FailCapture};
use crate::eval_env::{BindingEvaluationMode, EvalEnv};
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::{HelperOutputMeta, merge_provenance_sites};
use crate::node_eval::{NodeAction, control_headers, node_action};
use crate::observed_facts::ObservedFacts;
use crate::scalar_value::{ScalarValueDispatch, TruthCondition};
use crate::symbolic_local_state::SymbolicLocalState;
use crate::value_path_context::{RootDotIdentity, ValuePathContext};
use crate::{ContractProvenance, Guard, ResourceRef, SourceSpan};
use helm_schema_core::{GuardDnf, Predicate};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, Opaque,
    PathCondition, Sequence, SiteFacts, StringPart, and_conditions_with_memo,
};

/// The result of evaluating one template source: the abstract rendered
/// document plus the pathless value reads observed along the way.
#[derive(Debug, Default)]
pub(crate) struct EvaluatedDocument {
    /// The guarded abstract fragment for the whole source (all YAML
    /// documents merged; per-document projection is a later-stage concern).
    pub root: Guarded<AbstractFragment>,
    /// `.Values` reads that never render: condition reads, assignment
    /// right-hand sides, helper-internal guard reads, and range headers.
    pub reads: Vec<ValueRead>,
    pub(crate) observed_facts: ObservedFacts,
    /// Strictly string-consumed paths whose consumers execute BEFORE the
    /// values-root wrapper rewrite: their nodes must not gain the wrapper
    /// alternative (see the interpreter field of the same name).
    pub(crate) pre_rewrite_strict_paths: BTreeSet<helm_schema_core::ValuesPath>,
}

/// One pathless `.Values` read with the guards active at the read site.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ValueRead {
    /// The dotted `.Values` path that was read.
    pub values_path: helm_schema_core::ValuesPath,
    /// The value shape observed at the read (helper rows demoted at capture
    /// sites keep their fragment/scalar kind).
    pub kind: crate::ValueKind,
    /// The disjunction of predicate conjunctions under which the read occurs.
    pub condition: GuardDnf,
    /// The resource containing the read site, for site-scoped read classes
    /// (condition, bound-value, templated-key, and rendered-effect reads);
    /// helper-internal reads carry none.
    pub resource: Option<ResourceRef>,
    /// Source sites justifying the read.
    pub provenance: Vec<ContractProvenance>,
    /// Whether the read belongs to the dependency lane (helper rows demoted
    /// at capture sites) instead of the document lane.
    pub dependency: bool,
}

/// Evaluate one template source into its abstract fragment.
pub(crate) fn eval_document(
    source: &str,
    source_path: Option<&str>,
    db: &IrAnalysisDb,
) -> EvaluatedDocument {
    let Some(tree) = parse_go_template(source) else {
        return EvaluatedDocument::default();
    };
    let document = TemplatedDocument::parse_with_root(source, tree.root_node());
    let mut interpreter = Interpreter::for_source(source, source_path, db, &tree, &document);
    if let Some(name) = source_path.filter(|path| db.has_helper(path)) {
        // Only entry execution supplies Name; nested programs retain their caller's data.
        let template = interpreter
            .root_bindings
            .entry("Template".to_string())
            .or_insert_with(|| AbstractValue::Dict(BTreeMap::new()));
        if let AbstractValue::Dict(fields) = template {
            fields.insert(
                "Name".to_string(),
                AbstractValue::StringSet(BTreeSet::from([name.to_string()])),
            );
        }
    }
    let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
    let contributions = interpreter.eval_node_list(&roots);
    EvaluatedDocument {
        root: contributions.assemble(),
        reads: interpreter.reads,
        observed_facts: interpreter.observed_facts,
        pre_rewrite_strict_paths: interpreter.pre_rewrite_strict_paths,
    }
}

/// Control-header facts parsed once from the shared Go-template tree, keyed
/// by the region's opening-bracket byte (which equals the action node's
/// start byte).
pub(crate) struct ControlFacts {
    pub(super) arms: Vec<ArmSpec>,
    /// The whole region's end byte (through `{{ end }}`), for regions that
    /// only surface as holes (block-scalar bodies).
    pub(super) region_end: usize,
}

/// Source-only evaluation facts of one template body: independent of call
/// bindings, so helper bodies compute them once and reuse them across every
/// memoized-summary miss.
pub(crate) struct BodyEvalFacts {
    pub(super) control_facts: HashMap<usize, ControlFacts>,
    pub(super) resource_spans: Vec<ResourceSpan>,
    pub(super) adoption_plan: AdoptionPlan,
}

#[derive(Default)]
pub(super) struct AdoptionPlan {
    controls: HashMap<usize, Vec<EscapedControl>>,
    control_positions: HashMap<(usize, usize), usize>,
    crossing_priors: HashMap<usize, Vec<usize>>,
    /// Deepest real content end, excluding a control region's enclosing span.
    pub(super) content_ends: HashMap<usize, usize>,
    pub(super) child_indexes: HashMap<usize, ChildIndex>,
    pub(super) parent_shapes: HashMap<usize, ParentShape>,
    parent_slots: HashMap<ParentSlot, Vec<usize>>,
    layout_children: HashMap<usize, Vec<PlannedChild>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ParentKind {
    Entry,
    Item,
}

/// One rendered indentation slot inside a structural owner.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct ParentSlot {
    owner_start: Option<usize>,
    kind: ParentKind,
    indent: usize,
}

/// Immutable source facts for one container that can own deferred content.
#[derive(Clone, Copy)]
pub(super) struct ParentShape {
    pub(super) start: usize,
    pub(super) slot: ParentSlot,
    pub(super) indent: usize,
    pub(super) accepts_same_indent: bool,
    pub(super) established_content_mark: Option<usize>,
}

impl ParentShape {
    pub(super) fn kind(&self) -> ParentKind {
        self.slot.kind
    }
}

#[derive(Clone, Copy)]
struct PlannedChild {
    start: usize,
    disposition: LayoutDisposition,
}

#[derive(Clone, Copy)]
enum LayoutDisposition {
    OpenParent,
    Barrier,
    Transparent,
}

impl AdoptionPlan {
    pub(super) fn slot_members_through(&self, shape: &ParentShape) -> &[usize] {
        let Some(members) = self.parent_slots.get(&shape.slot) else {
            return &[];
        };
        let end = members.partition_point(|start| *start <= shape.start);
        members.get(..end).unwrap_or_default()
    }

    /// The source-known container suffix still open immediately before `end`.
    pub(super) fn trailing_open_chain(&self, root_start: usize, end: usize) -> Vec<ParentShape> {
        let mut chain = Vec::new();
        let mut current = root_start;
        loop {
            let Some(children) = self.layout_children.get(&current) else {
                break;
            };
            let Some(child) = children.iter().rev().find(|child| {
                child.start < end && !matches!(child.disposition, LayoutDisposition::Transparent)
            }) else {
                break;
            };
            if !matches!(child.disposition, LayoutDisposition::OpenParent) {
                break;
            }
            let Some(shape) = self.parent_shapes.get(&child.start).copied() else {
                break;
            };
            chain.push(shape);
            current = child.start;
        }
        chain
    }
}

#[derive(Clone, Copy)]
struct ChildExtent {
    start: usize,
    end: usize,
    index: usize,
}

/// Source-ordered child intervals for deferred range and overlap queries.
pub(super) struct ChildIndex {
    entries: Vec<ChildExtent>,
    leaf_base: usize,
    max_ends: Vec<usize>,
}

impl ChildIndex {
    fn new(children: &[PlannedNode]) -> Self {
        let mut entries = children
            .iter()
            .enumerate()
            .map(|(index, child)| ChildExtent {
                start: child.start,
                end: child.content_end,
                index,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|child| child.start);
        let leaf_base = entries.len().next_power_of_two();
        let mut max_ends = vec![0; leaf_base * 2];
        for (offset, child) in entries.iter().enumerate() {
            max_ends[leaf_base + offset] = child.end;
        }
        for node in (1..leaf_base).rev() {
            max_ends[node] = max_ends[node * 2].max(max_ends[node * 2 + 1]);
        }
        Self {
            entries,
            leaf_base,
            max_ends,
        }
    }

    pub(super) fn in_window(&self, window: SourceWindow) -> Vec<usize> {
        let start = self
            .entries
            .partition_point(|child| child.start < window.start);
        let end = window.end.map_or(self.entries.len(), |end| {
            self.entries.partition_point(|child| child.start < end)
        });
        self.entries[start..end]
            .iter()
            .map(|child| child.index)
            .collect()
    }

    pub(super) fn crossing(&self, boundary: usize) -> Vec<usize> {
        let before = self.entries.partition_point(|child| child.start < boundary);
        let mut crossing = Vec::new();
        self.collect_crossing(1, 0, self.leaf_base, before, boundary, &mut crossing);
        crossing
    }

    fn collect_crossing(
        &self,
        node: usize,
        start: usize,
        end: usize,
        before: usize,
        boundary: usize,
        crossing: &mut Vec<usize>,
    ) {
        if start >= before || self.max_ends[node] <= boundary {
            return;
        }
        if end - start == 1 {
            if let Some(child) = self.entries.get(start) {
                crossing.push(child.index);
            }
            return;
        }
        let middle = start + (end - start) / 2;
        self.collect_crossing(node * 2, start, middle, before, boundary, crossing);
        self.collect_crossing(node * 2 + 1, middle, end, before, boundary, crossing);
    }
}

/// The exact CST path from an escaped container to the control that owns it.
struct EscapedControl {
    control_start: usize,
    path: Vec<NodePathStep>,
}

#[derive(Clone)]
enum NodePathStep {
    Child(usize),
    Branch { branch: usize, child: usize },
}

impl BodyEvalFacts {
    pub(crate) fn collect(
        source: &str,
        db: &IrAnalysisDb,
        tree: &tree_sitter::Tree,
        document: &TemplatedDocument<'_>,
    ) -> Self {
        let mut control_facts = HashMap::new();
        collect_control_facts(tree.root_node(), source, &mut control_facts);
        Self {
            control_facts,
            resource_spans: crate::resource_identity::collect_resource_spans(document, db),
            adoption_plan: collect_adoption_plan(source, document.roots()),
        }
    }
}

fn collect_adoption_plan(source: &str, nodes: &[Node]) -> AdoptionPlan {
    let mut plan = AdoptionPlan::default();
    let mut path = Vec::new();
    let mut containers = Vec::new();
    collect_adoptions(source, nodes, &mut path, &mut containers, &mut plan);
    for controls in plan.controls.values_mut() {
        controls.sort_by_key(|control| control.control_start);
        controls.dedup_by_key(|control| control.control_start);
    }
    for (node_start, controls) in &plan.controls {
        for (index, control) in controls.iter().enumerate() {
            plan.control_positions
                .insert((*node_start, control.control_start), index + 1);
        }
    }
    for priors in plan.crossing_priors.values_mut() {
        priors.sort_unstable();
        priors.dedup();
    }
    for members in plan.parent_slots.values_mut() {
        members.sort_unstable();
        members.dedup();
    }
    plan
}

#[derive(Clone, Copy)]
struct PlannedNode {
    start: usize,
    end: usize,
    content_end: usize,
    control_start: Option<usize>,
    can_defer: bool,
    disposition: LayoutDisposition,
}

fn collect_adoptions(
    source: &str,
    nodes: &[Node],
    path: &mut Vec<NodePathStep>,
    containers: &mut Vec<(usize, usize)>,
    plan: &mut AdoptionPlan,
) -> Vec<PlannedNode> {
    let mut planned = Vec::with_capacity(nodes.len());
    for (index, node) in nodes.iter().enumerate() {
        path.push(NodePathStep::Child(index));
        planned.push(collect_adoption_node(source, node, path, containers, plan));
        path.pop();
    }
    collect_crossing_priors(&planned, plan);
    planned
}

fn collect_adoption_node(
    source: &str,
    node: &Node,
    path: &mut Vec<NodePathStep>,
    containers: &mut Vec<(usize, usize)>,
    plan: &mut AdoptionPlan,
) -> PlannedNode {
    let start = node.span_start();
    let owner_start = containers.last().map(|(start, _)| *start);
    let (end, content_end, direct_control, can_defer, disposition) = match node {
        Node::Mapping(entry) => {
            containers.push((start, path.len()));
            let children = collect_adoptions(source, &entry.children, path, containers, plan);
            containers.pop();
            plan.child_indexes.insert(start, ChildIndex::new(&children));
            plan.layout_children
                .insert(start, planned_children_in_source_order(&children));
            let shape = ParentShape {
                start,
                slot: ParentSlot {
                    owner_start,
                    kind: ParentKind::Entry,
                    indent: entry.indent,
                },
                indent: entry.indent,
                accepts_same_indent: entry.value.is_none() && entry.block.is_none(),
                established_content_mark: established_content_mark_from_source(
                    source,
                    &entry.children,
                    entry.indent,
                ),
            };
            plan.parent_shapes.insert(start, shape);
            if entry.opens_scope {
                plan.parent_slots.entry(shape.slot).or_default().push(start);
            }
            (
                children
                    .iter()
                    .map(|child| child.end)
                    .fold(mapping_own_end(entry), usize::max),
                children
                    .iter()
                    .map(|child| child.content_end)
                    .fold(mapping_own_end(entry), usize::max),
                None,
                true,
                if entry.opens_scope {
                    LayoutDisposition::OpenParent
                } else {
                    LayoutDisposition::Barrier
                },
            )
        }
        Node::Sequence(item) => {
            containers.push((start, path.len()));
            let children = collect_adoptions(source, &item.children, path, containers, plan);
            containers.pop();
            plan.child_indexes.insert(start, ChildIndex::new(&children));
            plan.layout_children
                .insert(start, planned_children_in_source_order(&children));
            let opens_scope = item.value.is_none() && item.block.is_none();
            let shape = ParentShape {
                start,
                slot: ParentSlot {
                    owner_start,
                    kind: ParentKind::Item,
                    indent: item.indent,
                },
                indent: item.indent,
                accepts_same_indent: false,
                established_content_mark: established_content_mark_from_source(
                    source,
                    &item.children,
                    item.indent,
                ),
            };
            plan.parent_shapes.insert(start, shape);
            if opens_scope {
                plan.parent_slots.entry(shape.slot).or_default().push(start);
            }
            (
                children
                    .iter()
                    .map(|child| child.end)
                    .fold(sequence_own_end(item), usize::max),
                children
                    .iter()
                    .map(|child| child.content_end)
                    .fold(sequence_own_end(item), usize::max),
                None,
                true,
                if opens_scope {
                    LayoutDisposition::OpenParent
                } else {
                    LayoutDisposition::Barrier
                },
            )
        }
        Node::Control(region) => {
            if let Some((container_start, depth)) = containers
                .iter()
                .find(|(start, _)| *start > region.span.start)
            {
                plan.controls
                    .entry(*container_start)
                    .or_default()
                    .push(EscapedControl {
                        control_start: region.span.start,
                        path: path[*depth..].to_vec(),
                    });
            }
            let mut end = region.span.end;
            let mut content_end = 0;
            for (branch_index, branch) in region.branches.iter().enumerate() {
                let mut children = Vec::with_capacity(branch.body.len());
                for (child_index, child) in branch.body.iter().enumerate() {
                    path.push(NodePathStep::Branch {
                        branch: branch_index,
                        child: child_index,
                    });
                    children.push(collect_adoption_node(source, child, path, containers, plan));
                    path.pop();
                }
                collect_crossing_priors(&children, plan);
                end = children.iter().map(|child| child.end).fold(end, usize::max);
                content_end = children
                    .iter()
                    .map(|child| child.content_end)
                    .fold(content_end, usize::max);
            }
            (
                end,
                content_end,
                Some(region.span.start),
                false,
                LayoutDisposition::Transparent,
            )
        }
        Node::Output(action) => (
            action.span.end,
            action.span.end,
            None,
            false,
            LayoutDisposition::Barrier,
        ),
        Node::Comment(comment) => (
            comment.span.end,
            comment.span.end,
            None,
            false,
            LayoutDisposition::Transparent,
        ),
        Node::Scalar(line) => (
            scalar_parts_end(&line.content).max(line.span.end),
            scalar_parts_end(&line.content).max(line.span.end),
            None,
            false,
            LayoutDisposition::Barrier,
        ),
        Node::Opaque(opaque) => (
            opaque.span.end,
            opaque.span.end,
            None,
            false,
            LayoutDisposition::Transparent,
        ),
    };
    let control_start = direct_control.or_else(|| {
        plan.controls
            .get(&start)
            .and_then(|controls| controls.iter().map(|control| control.control_start).min())
    });
    plan.content_ends.insert(start, content_end);
    PlannedNode {
        start,
        end,
        content_end,
        control_start,
        can_defer,
        disposition,
    }
}

fn planned_children_in_source_order(children: &[PlannedNode]) -> Vec<PlannedChild> {
    let mut planned = children
        .iter()
        .map(|child| PlannedChild {
            start: child.start,
            disposition: child.disposition,
        })
        .collect::<Vec<_>>();
    planned.sort_by_key(|child| child.start);
    planned
}

fn collect_crossing_priors(nodes: &[PlannedNode], plan: &mut AdoptionPlan) {
    let mut ordered = nodes.to_vec();
    ordered.sort_by_key(|node| (node.control_start.unwrap_or(node.start), node.start));
    let mut active = BTreeSet::new();
    let mut expirations = BinaryHeap::new();
    for node in ordered {
        let evaluation_start = node.control_start.unwrap_or(node.start);
        while let Some(Reverse((end, start))) = expirations.peek().copied() {
            if end > evaluation_start {
                break;
            }
            expirations.pop();
            active.remove(&start);
        }
        if let Some(control_start) = node.control_start {
            plan.crossing_priors
                .entry(control_start)
                .or_default()
                .extend(active.iter().copied());
        }
        if node.can_defer {
            active.insert(node.start);
            expirations.push(Reverse((node.end, node.start)));
        }
    }
}

fn mapping_own_end(entry: &syntax::MappingEntry) -> usize {
    let mut end = entry.span.end;
    if let Some(value) = &entry.value {
        end = end.max(scalar_parts_end(value));
    }
    if let Some(block) = &entry.block {
        end = end.max(block.header.end).max(block.body.end);
    }
    end
}

fn sequence_own_end(item: &syntax::SequenceItem) -> usize {
    let mut end = item.span.end;
    if let Some(value) = &item.value {
        end = end.max(scalar_parts_end(value));
    }
    if let Some(block) = &item.block {
        end = end.max(block.header.end).max(block.body.end);
    }
    end
}

fn scalar_parts_end(parts: &ScalarParts) -> usize {
    parts.parts.iter().fold(parts.span.end, |end, part| {
        let (ScalarPart::Text(span) | ScalarPart::Hole(span)) = part;
        end.max(span.end)
    })
}

fn collect_control_facts(
    node: tree_sitter::Node<'_>,
    source: &str,
    out: &mut HashMap<usize, ControlFacts>,
) {
    match node.kind() {
        "if_action" | "with_action" => {
            let is_if = node.kind() == "if_action";
            let arms = control_headers(source, node)
                .into_iter()
                .map(|header| {
                    if is_if {
                        ArmSpec::If(header)
                    } else {
                        ArmSpec::With(header)
                    }
                })
                .chain(std::iter::once(ArmSpec::Else))
                .collect();
            out.insert(
                node.start_byte(),
                ControlFacts {
                    arms,
                    region_end: node.end_byte(),
                },
            );
        }
        "range_action" => {
            let arms = vec![
                ArmSpec::Range {
                    header: range_header_from_source(node, source),
                    destructured: range_has_destructured_variable_definition(node),
                    binding_kind: if helm_schema_ast::range_uses_assignment(node) {
                        crate::fragment_assignment::AssignmentKind::Assignment
                    } else {
                        crate::fragment_assignment::AssignmentKind::Declaration
                    },
                    value_variable: helm_schema_ast::range_destructured_value_variable(
                        node, source,
                    ),
                    key_variable: helm_schema_ast::range_destructured_key_variable(node, source),
                },
                ArmSpec::Else,
            ];
            out.insert(
                node.start_byte(),
                ControlFacts {
                    arms,
                    region_end: node.end_byte(),
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

/// Where a container first holds content of its own — the earliest child that
/// renders DEEPER than the container's indent, counting splices and control
/// bodies as well as visible lines.
///
/// Unlike [`content_child_mark`], which asks the narrower "has a visible line
/// established this as a mapping", this decides whether the container is still
/// an empty value slot at a given point in the source. Vault opens
/// `annotations:` and fills it with a splice written at column 0, then adds one
/// conditional key AFTER it; signoz opens `annotations:` and fills it with a
/// `nindent 4` splice BEFORE the column-0 splice that follows. Only the first
/// container is still open when its splice runs.
fn established_content_mark_from_source(
    source: &str,
    children: &[Node],
    container_indent: usize,
) -> Option<usize> {
    children
        .iter()
        .filter_map(|child| match child {
            Node::Mapping(entry) => (entry.indent > container_indent).then_some(entry.span.start),
            Node::Sequence(item) => (item.indent > container_indent).then_some(item.span.start),
            Node::Scalar(line) => (line.indent > container_indent).then_some(line.span.start),
            Node::Output(action) => (output_render_indent_from_source(source, action.span)
                > container_indent)
                .then_some(action.span.start),
            Node::Control(region) => region
                .branches
                .iter()
                .filter_map(|branch| {
                    established_content_mark_from_source(source, &branch.body, container_indent)
                })
                .min(),
            Node::Comment(_) | Node::Opaque(_) => None,
        })
        .min()
}

fn output_render_indent_from_source(source: &str, span: Span) -> usize {
    parse_expr_text(source.get(span.start..span.end).unwrap_or(""))
        .iter()
        .rev()
        .find_map(TemplateExpr::fragment_indent_width)
        .unwrap_or_else(|| {
            let line_start = source
                .get(..span.start)
                .and_then(|prefix| prefix.rfind('\n'))
                .map_or(0, |newline| newline + 1);
            source
                .get(line_start..)
                .map_or(0, |line| line.len() - line.trim_start_matches(' ').len())
        })
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

/// Whether a block-scalar entry key names a YAML document
/// (`jcasc-default-config.yaml`, `{{ $key }}.yml`). A config map's data keys
/// are file names, and the extension is the chart's only static statement
/// that whatever consumes the block parses it as YAML rather than keeping it
/// as opaque bytes (jenkins' `plugins.txt` beside its `JCasC` document).
fn key_names_yaml_document(key: &EntryKey) -> bool {
    let names_document = |text: &str| {
        // Not `Path::extension`: a templated key contributes only its literal
        // tail (`.yaml`), which that reads as a dotfile with no extension.
        text.trim_end()
            .rsplit_once('.')
            .is_some_and(|(_, extension)| {
                extension.eq_ignore_ascii_case("yaml") || extension.eq_ignore_ascii_case("yml")
            })
    };
    match key {
        EntryKey::Literal(literal) => names_document(literal),
        // A templated key names one only when its own literal tail does: the
        // rendered prefix cannot take the extension away.
        EntryKey::Dynamic(string) => match string.parts.last() {
            Some(StringPart::Text(alternatives)) => {
                !alternatives.is_empty() && alternatives.iter().all(|text| names_document(text))
            }
            _ => false,
        },
    }
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
#[derive(Clone, Default)]
pub(super) struct Contributions {
    pub(super) entries: Vec<MappingEntry>,
    pub(super) items: Vec<Guarded<AbstractFragment>>,
    pub(super) values: Guarded<AbstractFragment>,
    /// Fragment output carrying an explicit rendered indent
    /// (`… | nindent N`): it attaches to the nearest enclosing container
    /// whose indent is shallower than `N`, floating up past deeper open
    /// containers exactly like the attribution index's open-slot query.
    pub(super) floating: Vec<FloatingOutput>,
    pub(super) loop_control: LoopControl,
}

#[derive(Clone, Default)]
pub(super) struct LoopControl {
    pub(super) breaks: Vec<crate::symbolic_local_state::ControlOutcome>,
    pub(super) continues: Vec<crate::symbolic_local_state::ControlOutcome>,
}

impl LoopControl {
    pub(super) fn exit_condition(&self) -> PathCondition {
        any_conditions(
            self.breaks
                .iter()
                .chain(&self.continues)
                .map(crate::symbolic_local_state::ControlOutcome::condition)
                .collect(),
        )
    }

    pub(super) fn break_condition(&self) -> PathCondition {
        any_conditions(
            self.breaks
                .iter()
                .map(crate::symbolic_local_state::ControlOutcome::condition)
                .collect(),
        )
    }

    pub(super) fn guard_all(
        &mut self,
        condition: &PathCondition,
        memo: &helm_schema_core::PredicateMemo,
    ) {
        for exit in self.breaks.iter_mut().chain(&mut self.continues) {
            exit.guard_all(condition.clone(), memo);
        }
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.breaks.extend(other.breaks);
        self.continues.extend(other.continues);
    }

    pub(super) fn replace_states(
        &mut self,
        state: &crate::symbolic_local_state::SymbolicLocalState,
    ) {
        for exit in self.breaks.iter_mut().chain(&mut self.continues) {
            exit.replace_state(state);
        }
    }
}

fn any_conditions(mut conditions: Vec<PathCondition>) -> PathCondition {
    conditions.retain(|condition| *condition != Predicate::False);
    if conditions.contains(&Predicate::True) {
        return Predicate::True;
    }
    conditions.sort();
    conditions.dedup();
    match conditions.as_slice() {
        [] => Predicate::False,
        [condition] => condition.clone(),
        _ => Predicate::Or(conditions),
    }
}

/// One fragment output looking for its container.
#[derive(Clone)]
pub(super) struct FloatingOutput {
    /// The rendered indent (`nindent`/`indent` width).
    pub(super) width: usize,
    /// Whether `width` is only the action's source column, because the
    /// expression states no width of its own. Such a column is a LOWER bound:
    /// vault's `{{ template "vault.service.annotations" . }}` sits at column 0
    /// and expands a body that emits `nindent 4`. A container that has not
    /// held content yet therefore still owns the splice, while a stated width
    /// is exact and only a strictly deeper container does.
    pub(super) column_only: bool,
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

    pub(super) fn guard_all(
        &mut self,
        condition: &PathCondition,
        memo: &helm_schema_core::PredicateMemo,
    ) {
        self.entries.retain_mut(|entry| {
            let was_empty = entry.value.is_empty();
            entry.value.guard_all(condition, memo);
            was_empty || !entry.value.is_empty()
        });
        self.items.retain_mut(|item| {
            item.guard_all(condition, memo);
            !item.is_empty()
        });
        self.values.guard_all(condition, memo);
        self.floating.retain_mut(|floating| {
            floating.value.guard_all(condition, memo);
            !floating.value.is_empty()
        });
        self.loop_control.guard_all(condition, memo);
    }

    pub(super) fn extend(&mut self, other: Self) {
        for entry in other.entries {
            self.merge_entry(entry.key, entry.value);
        }
        self.items.extend(other.items);
        self.values.extend(other.values);
        self.floating.extend(other.floating);
        self.loop_control.extend(other.loop_control);
    }

    pub(super) fn extend_guarded_fragment(
        &mut self,
        guarded: Guarded<AbstractFragment>,
        memo: &helm_schema_core::PredicateMemo,
    ) {
        for (condition, fragment) in guarded.arms {
            if condition == Predicate::False {
                continue;
            }
            match fragment {
                AbstractFragment::Mapping(mapping) => {
                    for mut entry in mapping.entries {
                        entry.value.guard_all(&condition, memo);
                        entry
                            .value
                            .arms
                            .retain(|(condition, _)| *condition != Predicate::False);
                        if !entry.value.is_empty() {
                            self.merge_entry(entry.key, entry.value);
                        }
                    }
                }
                AbstractFragment::Sequence(sequence) => {
                    for mut item in sequence.items {
                        item.guard_all(&condition, memo);
                        item.arms
                            .retain(|(condition, _)| *condition != Predicate::False);
                        if !item.is_empty() {
                            self.items.push(item);
                        }
                    }
                }
                fragment => self.values.arms.push((condition, fragment)),
            }
        }
    }

    pub(super) fn take_loop_control(&mut self) -> LoopControl {
        std::mem::take(&mut self.loop_control)
    }

    fn repair_valueless_mapping_header(&mut self) {
        let Some((header_index, header_key)) =
            self.entries
                .iter()
                .enumerate()
                .rev()
                .find_map(|(index, entry)| match &entry.key {
                    EntryKey::Literal(key) if !key.is_empty() && entry.value.is_empty() => {
                        Some((index, key.clone()))
                    }
                    EntryKey::Literal(_) | EntryKey::Dynamic(_) => None,
                })
        else {
            return;
        };
        let Some(trailing) = self.entries.get(header_index + 1..) else {
            return;
        };
        if !trailing
            .iter()
            .all(|entry| matches!(entry.key, EntryKey::Dynamic(_)))
        {
            return;
        }
        let dynamic_entries = self.entries.split_off(header_index + 1);
        let mut continued_values = take_mapping_continuation_arms(&mut self.values, &header_key);
        self.floating.retain_mut(|floating| {
            continued_values.extend(take_mapping_continuation_arms(
                &mut floating.value,
                &header_key,
            ));
            !floating.value.is_empty()
        });
        if dynamic_entries.is_empty() && continued_values.is_empty() {
            return;
        }
        if let Some(header) = self.entries.get_mut(header_index) {
            header.value.extend(continued_values);
            if !dynamic_entries.is_empty() {
                header.value.arms.push((
                    Predicate::True,
                    AbstractFragment::Mapping(Mapping {
                        entries: dynamic_entries,
                    }),
                ));
            }
        }
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
        content_at: Option<usize>,
    ) -> Guarded<AbstractFragment> {
        let mut attached = Guarded::empty();
        let mut keep = Vec::new();
        for floating in std::mem::take(&mut self.floating) {
            // A stated width lands exactly, so only the container's own indent
            // (where a sequence may be written) is a second chance for it. A
            // column-only width is a lower bound, so any still-empty container
            // enclosing it is one — the width the splice really renders at can
            // live inside the body it expands.
            let same_indent_ok = if floating.column_only {
                accepts_same_indent && content_at.is_none_or(|at| at >= floating.origin)
            } else {
                floating.width == container_indent
                    && accepts_same_indent
                    && marked_at.is_none_or(|marked| marked >= floating.origin)
            };
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

fn take_mapping_continuation_arms(
    guarded: &mut Guarded<AbstractFragment>,
    header_key: &str,
) -> Guarded<AbstractFragment> {
    Guarded {
        arms: guarded
            .arms
            .extract_if(.., |(condition, fragment)| match fragment {
                AbstractFragment::Splice(_) => predicate_has_range(condition),
                AbstractFragment::Mapping(mapping) => mapping.entries.iter().all(|entry| {
                    matches!(entry.key, EntryKey::Dynamic(_))
                        || matches!(&entry.key, EntryKey::Literal(key)
                            if key == header_key && entry.value.is_empty())
                }),
                AbstractFragment::Sequence(_)
                | AbstractFragment::Scalar(_)
                | AbstractFragment::Opaque(_) => false,
            })
            .collect(),
    }
}

fn predicate_has_range(predicate: &Predicate) -> bool {
    match predicate.kind() {
        helm_schema_core::PredicateKind::Guard(Guard::Range { .. }) => true,
        helm_schema_core::PredicateKind::Not(inner) => predicate_has_range(inner),
        helm_schema_core::PredicateKind::And(predicates)
        | helm_schema_core::PredicateKind::Or(predicates) => {
            predicates.iter().any(predicate_has_range)
        }
        helm_schema_core::PredicateKind::True
        | helm_schema_core::PredicateKind::False
        | helm_schema_core::PredicateKind::Approximate { .. }
        | helm_schema_core::PredicateKind::Guard(_) => false,
    }
}

/// A half-open source window for descendants of an adopted node.
#[derive(Clone, Copy)]
pub(super) struct SourceWindow {
    pub(super) start: usize,
    pub(super) end: Option<usize>,
}

impl SourceWindow {
    fn all() -> Self {
        Self {
            start: 0,
            end: None,
        }
    }

    pub(super) fn bounded(start: usize, end: usize) -> Self {
        Self {
            start,
            end: Some(end.max(start)),
        }
    }

    pub(super) fn with_end(self, end: usize) -> Self {
        Self {
            start: self.start,
            end: Some(self.end.map_or(end, |current| current.min(end))),
        }
    }

    pub(super) fn contains(self, start: usize) -> bool {
        start >= self.start && self.end.is_none_or(|end| start < end)
    }

    pub(super) fn intersect(self, other: Self) -> Self {
        let end = match (self.end, other.end) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(end), None) | (None, Some(end)) => Some(end),
            (None, None) => None,
        };
        Self {
            start: self.start.max(other.start),
            end,
        }
    }

    pub(super) fn is_empty(self) -> bool {
        self.end.is_some_and(|end| self.start >= end)
    }
}

/// A node reference plus the source-window bounds of control adoption.
///
/// Children outside the window evaluate in another branch or after the
/// region.
/// Controls through the omitted boundary are evaluated by their owners
/// instead of through the adopted ancestor chain.
#[derive(Clone, Copy)]
pub(super) struct NodeView<'n> {
    pub(super) node: &'n Node,
    pub(super) window: SourceWindow,
    pub(super) omitted_control: Option<usize>,
    pub(super) control_cursor: usize,
}

impl<'n> NodeView<'n> {
    pub(super) fn plain(node: &'n Node) -> Self {
        Self {
            node,
            window: SourceWindow::all(),
            omitted_control: None,
            control_cursor: 0,
        }
    }

    /// The node's children that evaluate in this source window.
    ///
    /// The bounds propagate so later descendants and the owning control stay
    /// excluded until the adoption plan evaluates them.
    pub(super) fn in_scope_children(&self) -> Vec<NodeView<'n>> {
        let children = match self.node {
            Node::Mapping(entry) => &entry.children,
            Node::Sequence(item) => &item.children,
            _ => return Vec::new(),
        };
        children
            .iter()
            .filter(|child| {
                self.window.contains(child.span_start())
                    && !matches!(child, Node::Control(region)
                        if self.omitted_control.is_some_and(|omitted| region.span.start <= omitted))
            })
            .map(|child| NodeView {
                node: child,
                window: self.window,
                omitted_control: self.omitted_control,
                control_cursor: 0,
            })
            .collect()
    }
}

fn control_at_path<'n>(root: &'n Node, path: &[NodePathStep]) -> Option<&'n ControlRegion> {
    let mut node = root;
    for step in path {
        node = match (node, step) {
            (Node::Mapping(entry), NodePathStep::Child(index)) => entry.children.get(*index)?,
            (Node::Sequence(item), NodePathStep::Child(index)) => item.children.get(*index)?,
            (Node::Control(region), NodePathStep::Branch { branch, child }) => {
                region.branches.get(*branch)?.body.get(*child)?
            }
            _ => return None,
        };
    }
    match node {
        Node::Control(region) => Some(region),
        _ => None,
    }
}

/// Returns the branch and half-open source window containing a node start.
pub(super) fn branch_window(region: &ControlRegion, node_start: usize) -> (usize, SourceWindow) {
    let mut target = 0;
    for (index, branch) in region.branches.iter().enumerate() {
        if node_start >= branch.header.end {
            target = index;
        }
    }
    let start = region
        .branches
        .get(target)
        .map_or(region.span.start, |branch| branch.header.end);
    let end = region
        .branches
        .get(target + 1)
        .map_or(region.span.end, |branch| branch.header.start);
    (target, SourceWindow::bounded(start, end))
}

/// One adopted escaped sibling: its bounded in-scope view plus the
/// enclosing region bound that caps this region's deferral window.
pub(super) struct Adopted<'n> {
    pub(super) view: NodeView<'n>,
    pub(super) defer_window: SourceWindow,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum ParentShell {
    Entry(EntryKey),
    Item,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) struct ParentShellArm {
    pub(super) source_start: usize,
    pub(super) condition: PathCondition,
    pub(super) shell: ParentShell,
}

/// One arm's decoded activation.
#[derive(Clone)]
pub(super) enum ArmSpec {
    If(Option<TemplateHeader>),
    With(Option<TemplateHeader>),
    Range {
        header: Option<TemplateHeader>,
        destructured: bool,
        binding_kind: crate::fragment_assignment::AssignmentKind,
        value_variable: Option<String>,
        key_variable: Option<String>,
    },
    Else,
}

#[derive(Clone, Copy)]
pub(super) struct ScopeMark {
    predicates: usize,
    dots: usize,
    range_modes: usize,
    capture_approximates: usize,
    loop_depth: usize,
}

#[derive(Clone)]
pub(super) struct DotBinding {
    value: Option<AbstractValue>,
    mode: BindingEvaluationMode,
}

pub(super) struct Interpreter<'a> {
    pub(super) source: &'a str,
    pub(super) source_path: Option<&'a str>,
    /// Byte offset of this source within its file (helper bodies evaluate
    /// over the define body text; provenance spans stay file-absolute).
    pub(super) source_offset: usize,
    pub(super) db: &'a IrAnalysisDb,
    /// Source-only facts (control headers, resource spans), shared across
    /// the memoized evaluations of one helper body.
    pub(super) body_facts: Rc<BodyEvalFacts>,
    pub(super) inline_regions: Vec<Span>,
    /// Named helper bodies contributing to source provenance.
    pub(super) helper_provenance_chain: Vec<String>,
    /// Whether this interpreter evaluates a helper body (a summary run).
    pub(super) helper_scope: bool,
    /// Whether scalar output dispatches refine rendered holes. This is set
    /// only by the scalar-output summary pass; the document pass keeps its
    /// ordinary fragment projection so comparison facts cannot perturb
    /// unrelated sink inference.
    pub(super) scalar_output_projection: bool,
    /// The active helper call chain, threaded into expression evaluation so
    /// nested bound calls cut cycles.
    pub(super) helper_seen: HashSet<String>,
    /// Typed payload truth observed at whole-hole JSON helper outputs.
    pub(super) json_payload_truth_outputs: Vec<(Predicate, TruthCondition)>,
    pub(super) locals: SymbolicLocalState,
    pub(super) dot_stack: Vec<DotBinding>,
    /// The value-flavor dot of a helper scope's root frame (the call
    /// boundary resolves both flavors; see [`DotBinding`]).
    /// Document scope has none: its value dot derives from the fragment dot.
    pub(super) root_value_dot: Option<AbstractValue>,
    pub(super) root_bindings: HashMap<String, AbstractValue>,
    pub(super) root_truthy_predicates: HashMap<String, Predicate>,
    /// Exhaustive per-arm value alternatives for root-context fields set
    /// across complete if/else chains (see [`ScalarValueDispatch`]); condition
    /// decoding resolves root-field equalities through them.
    pub(super) root_value_dispatches: HashMap<String, ScalarValueDispatch>,
    /// Root-context replacements observed in source order and exported by helper summaries.
    pub(super) root_set_mutations_observed: BTreeMap<String, AbstractValue>,
    pub(super) root_set_predicates_observed: BTreeMap<String, Predicate>,
    pub(super) root_value_dispatches_observed: BTreeMap<String, ScalarValueDispatch>,
    /// Values paths with a strict STRING consumer that executed before the
    /// first values-root program-wrapper rewrite in this source: a wrapper
    /// map at such a path reaches the consumer raw and aborts (nats'
    /// `fullname | trunc` reads `nameOverride` before `nats.defaultValues`
    /// substitutes the tplYaml result), so those nodes must not gain the
    /// wrapper alternative. Tolerant pre-rewrite reads (`default`
    /// selections that only copy the value) stay wrapper-eligible.
    pub(super) pre_rewrite_strict_paths: BTreeSet<helm_schema_core::ValuesPath>,
    pub(super) active_predicates: Vec<Predicate>,
    /// Loop nesting depth of the evaluation point (block and inline range
    /// bodies): first-iteration reasoning is only sound at depth one.
    pub(super) loop_depth: usize,
    pub(super) reads: Vec<ValueRead>,
    /// Dedup shadow of `reads` (order lives in the vec).
    reads_seen: HashSet<ValueRead>,
    /// Paths consumed as serialized YAML by `fromYaml`; document-scope
    /// helper conditions import this narrow input contract without importing
    /// unrelated helper-body output transformations.
    pub(super) parsed_yaml_input_paths: BTreeSet<helm_schema_core::ValuesPath>,
    /// Paths whose helper output was serialized with `toYaml`; callers use
    /// this to recognize a matching `fromYaml` as a structural round trip.
    pub(super) yaml_serialized_paths: BTreeSet<helm_schema_core::ValuesPath>,
    pub(super) observed_facts: ObservedFacts,
    pub(super) evaluated_parent_shells: HashMap<usize, Vec<ParentShellArm>>,
    /// Captures that hold only where this source's rendered TEXT is consumed
    /// as YAML. A helper body renders at its caller's position, so its plain
    /// slots corrupt a document only when the caller splices the body raw
    /// into one; the caller certifies that and absorbs (or defers again).
    pub(super) text_captures: BTreeSet<FailCapture>,
    /// Paths whose text the CURRENT scalar run renders through `tpl`. Reset
    /// per run: the completed-token pass reads it to tell an identity-carrying
    /// taint from a genuinely transformed one.
    pub(super) run_templated_text_paths: BTreeSet<helm_schema_core::ValuesPath>,
    /// Whether the walk is inside a mapping-entry or sequence-item VALUE
    /// slot. Document-level content is not a slot: it renders whole manifests,
    /// where a `: ` is structure rather than a broken plain token.
    pub(super) in_value_slot: bool,
    /// Whether the block scalar being walked holds a YAML document — its
    /// entry key names one (`jcasc-default-config.yaml`). Block text is
    /// otherwise opaque to YAML: only a named document makes a splice's own
    /// characters structure again.
    pub(super) block_text_is_yaml: bool,
    /// Object-producing mutations observed before subsequent member reads.
    pub(super) member_host_conversions: BTreeSet<crate::eval_effect::MemberHostConversion>,
    /// Range identities active on the walk. Input and member identity remain
    /// separate because a derived iterable may visit values-backed members
    /// without iterating that values path itself.
    pub(super) active_range_modes:
        Vec<(helm_schema_core::ValuesPath, crate::range_modes::RangeMode)>,
    /// Approximate conjuncts for exact-range items that not every iterable
    /// alternative executes (nats' jsonpatch conditionally appends "from"
    /// to `$opPathKeys`). They condition CAPTURE conjunctions only — rows
    /// and type hints keep the join semantics ordinary contributions have —
    /// so strict captures inside such an item cannot bind unconditionally.
    pub(super) alternative_capture_approximates: Vec<Predicate>,
    /// Predicate paths severed by index-call narrowing anywhere in this
    /// source: guard reads of their strict ancestors are dropped from the
    /// summary (the narrowing proves the ancestor probe was a traversal
    /// step, not a condition on the ancestor itself).
    pub(super) suppress_predicate_paths: BTreeSet<helm_schema_core::ValuesPath>,
    /// Every chart-level `set … default` normalization observed anywhere in
    /// this source, unconditionally. `locals.chart_value_defaults` keeps the
    /// branch-intersected "definitely ran in source order" view for render
    /// sites; helper summaries export this accumulator instead (a summary's
    /// defaults are declarations for the caller, like they always were).
    pub(super) chart_defaults_observed: BTreeSet<helm_schema_core::ValuesPath>,
    /// The site facts of the hole or control region currently being
    /// evaluated; reads recorded during that evaluation carry them.
    pub(super) current_site: Option<Rc<SiteFacts>>,
}

impl<'a> Interpreter<'a> {
    pub(super) fn mark_scope(&self) -> ScopeMark {
        ScopeMark {
            predicates: self.active_predicates.len(),
            dots: self.dot_stack.len(),
            range_modes: self.active_range_modes.len(),
            capture_approximates: self.alternative_capture_approximates.len(),
            loop_depth: self.loop_depth,
        }
    }

    pub(super) fn rewind(&mut self, mark: ScopeMark) {
        self.active_predicates.truncate(mark.predicates);
        self.dot_stack.truncate(mark.dots);
        self.active_range_modes.truncate(mark.range_modes);
        self.alternative_capture_approximates
            .truncate(mark.capture_approximates);
        self.loop_depth = mark.loop_depth;
    }

    pub(super) fn push_dot(&mut self, value: Option<AbstractValue>, mode: BindingEvaluationMode) {
        self.dot_stack.push(DotBinding { value, mode });
    }

    /// A fresh interpreter over one parsed source: control-header facts,
    /// inline-region spans, and resource spans are collected up front; all
    /// evaluation state starts empty.
    pub(super) fn for_source(
        source: &'a str,
        source_path: Option<&'a str>,
        db: &'a IrAnalysisDb,
        tree: &tree_sitter::Tree,
        document: &TemplatedDocument<'_>,
    ) -> Self {
        let body_facts = Rc::new(BodyEvalFacts::collect(source, db, tree, document));
        Self::with_body_facts(source, source_path, db, document, body_facts)
    }

    /// A fresh interpreter reusing precomputed source-only facts (helper
    /// bodies share them across memoized evaluations).
    pub(super) fn with_body_facts(
        source: &'a str,
        source_path: Option<&'a str>,
        db: &'a IrAnalysisDb,
        document: &TemplatedDocument<'_>,
        body_facts: Rc<BodyEvalFacts>,
    ) -> Self {
        let mut inline_regions = Vec::new();
        collect_inline_regions(document.roots(), &mut inline_regions);
        Self {
            source,
            source_path,
            source_offset: 0,
            db,
            body_facts,
            inline_regions,
            helper_provenance_chain: Vec::new(),
            helper_scope: false,
            scalar_output_projection: false,
            helper_seen: HashSet::new(),
            json_payload_truth_outputs: Vec::new(),
            locals: SymbolicLocalState::with_root(
                AbstractValue::RootContext,
                BindingEvaluationMode::Direct,
            ),
            dot_stack: Vec::new(),
            root_value_dot: None,
            root_bindings: db.static_root_fields().clone(),
            root_truthy_predicates: HashMap::new(),
            root_value_dispatches: HashMap::new(),
            root_set_mutations_observed: BTreeMap::new(),
            root_set_predicates_observed: BTreeMap::new(),
            root_value_dispatches_observed: BTreeMap::new(),
            pre_rewrite_strict_paths: BTreeSet::new(),
            active_predicates: Vec::new(),
            loop_depth: 0,
            reads: Vec::new(),
            reads_seen: HashSet::new(),
            parsed_yaml_input_paths: BTreeSet::new(),
            yaml_serialized_paths: BTreeSet::new(),
            observed_facts: ObservedFacts::default(),
            evaluated_parent_shells: HashMap::new(),
            text_captures: BTreeSet::new(),
            run_templated_text_paths: BTreeSet::new(),
            in_value_slot: false,
            block_text_is_yaml: false,
            member_host_conversions: BTreeSet::new(),
            active_range_modes: Vec::new(),
            alternative_capture_approximates: Vec::new(),
            suppress_predicate_paths: BTreeSet::new(),
            chart_defaults_observed: BTreeSet::new(),
            current_site: None,
        }
    }

    pub(super) fn text(&self, span: Span) -> &'a str {
        self.source.get(span.start..span.end).unwrap_or("")
    }

    /// The site facts of one output hole: the smallest resource span
    /// containing the hole's start byte plus the hole's own provenance.
    pub(super) fn hole_site(&self, span: Span) -> Option<Rc<SiteFacts>> {
        let resource_span = self
            .body_facts
            .resource_spans
            .iter()
            .filter(|resource| resource.start <= span.start && span.start < resource.end)
            .min_by(|left, right| {
                let left_len = left.end.saturating_sub(left.start);
                let right_len = right.end.saturating_sub(right.start);
                left_len
                    .cmp(&right_len)
                    .then_with(|| right.start.cmp(&left.start))
            });
        self.site_facts(resource_span.map(|span| self.span_resource(span)), span)
    }

    /// The site facts of one control region: the region's resource is the
    /// unique resource intersecting the region span (a region spanning
    /// several manifest documents claims none).
    pub(super) fn region_site(&self, span: Span) -> Option<Rc<SiteFacts>> {
        let mut unique: Option<&ResourceSpan> = None;
        for resource in &self.body_facts.resource_spans {
            if resource.start >= span.end || span.start >= resource.end {
                continue;
            }
            match unique {
                Some(existing) if existing.resource != resource.resource => {
                    unique = None;
                    break;
                }
                Some(_) => {}
                None => unique = Some(resource),
            }
        }
        self.site_facts(unique.map(|span| self.span_resource(span)), span)
    }

    /// The span's resource for one tagged use, with any inline-conditional
    /// kind arms predicate-qualified through the CURRENT scope: the
    /// selecting locals bind above the header, so each guard lowers exactly
    /// as the body's own branch conditions do and the builder can match row
    /// conjunctions to arms structurally.
    fn span_resource(&self, resource_span: &ResourceSpan) -> (ResourceRef, Vec<String>) {
        let mut resource = resource_span.resource.clone();
        resource.kind_branches = self.resolved_kind_branches(&resource_span.kind_branch_sources);
        (resource, resource_span.path_prefix.clone())
    }

    /// Lower an inline kind chain's raw guard texts into per-arm
    /// predicates. An arm holds where its own guard does AND every earlier
    /// guard failed. Any undecodable guard abstains entirely: dropping one
    /// arm would leave an incomplete partition that misassigns rows.
    fn resolved_kind_branches(
        &self,
        sources: &[helm_schema_ast::KindBranchSource],
    ) -> Vec<helm_schema_core::KindBranch> {
        if sources.is_empty() {
            return Vec::new();
        }
        let context = self.value_path_context();
        let mut prior_negations: Vec<Predicate> = Vec::new();
        let mut branches = Vec::new();
        for source in sources {
            let mut conjuncts = prior_negations.clone();
            if let Some(text) = &source.condition {
                let wrapped = format!("{{{{ {} }}}}", text.trim());
                let exprs = helm_schema_ast::parse_action_expressions(&wrapped);
                let [expr] = exprs.as_slice() else {
                    return Vec::new();
                };
                if !context.condition_lowering_is_usable_for_control(expr) {
                    return Vec::new();
                }
                let predicate = self
                    .db
                    .predicate_memo()
                    .normalize(context.condition_predicate_expr(expr));
                prior_negations.push(self.db.predicate_memo().normalize(predicate.negated()));
                conjuncts.push(predicate);
            }
            branches.push(helm_schema_core::KindBranch {
                predicate: Predicate::all(conjuncts),
                kind: source.kind.clone(),
            });
        }
        branches
    }

    fn site_facts(
        &self,
        resource: Option<(ResourceRef, Vec<String>)>,
        span: Span,
    ) -> Option<Rc<SiteFacts>> {
        let provenance = self.source_path.map(|source_path| {
            let helper_chain = self.helper_provenance_chain.clone();
            ContractProvenance::new(
                source_path,
                SourceSpan::new(
                    self.source_offset + span.start,
                    self.source_offset + span.end,
                ),
                helper_chain,
            )
        });
        let (resource, path_prefix) = match resource {
            Some((resource, path_prefix)) => (Some(resource), path_prefix),
            None => (None, Vec::new()),
        };
        if resource.is_none() && provenance.is_none() {
            return None;
        }
        Some(Rc::new(SiteFacts {
            resource,
            path_prefix,
            provenance,
        }))
    }

    /// Run one evaluation step under the site facts of `span`, restoring the
    /// previous site afterwards.
    pub(super) fn enter_hole_site(&mut self, span: Span) -> Option<Rc<SiteFacts>> {
        let site = self.hole_site(span);
        std::mem::replace(&mut self.current_site, site)
    }

    pub(super) fn restore_site(&mut self, previous: Option<Rc<SiteFacts>>) {
        self.current_site = previous;
    }

    pub(super) fn current_dot_fragment(&self) -> Option<AbstractValue> {
        self.dot_stack
            .last()
            .and_then(|binding| binding.value.clone())
    }

    pub(super) fn current_dot_binding(&self) -> Option<AbstractValue> {
        if self.dot_stack.len() <= 1
            && let Some(root) = &self.root_value_dot
        {
            return Some(root.clone());
        }
        self.dot_stack
            .last()
            .and_then(|binding| binding.value.as_ref())
            .and_then(AbstractValue::to_current_dot_context_value)
    }

    /// The value-flavor dot for expression evaluation: a helper scope's root
    /// frame carries the call boundary's own value dot; everywhere else the
    /// fragment dot's context-value projection stands in.
    pub(super) fn current_value_dot(&self) -> Option<AbstractValue> {
        if self.dot_stack.len() <= 1
            && let Some(root) = &self.root_value_dot
        {
            return Some(root.clone());
        }
        self.current_dot_fragment()
            .map(|value| value.to_context_value())
            .or_else(|| self.current_dot_binding())
    }

    pub(super) fn current_dot_binding_mode(&self) -> BindingEvaluationMode {
        self.dot_stack
            .last()
            .map_or(BindingEvaluationMode::Direct, |binding| binding.mode)
    }

    pub(super) fn value_path_context(&self) -> ValuePathContext<'_> {
        // Member bindings resolve for conditions and assignments; explicit
        // fragment values shadow them where both exist.
        let mut template_bindings = self
            .locals
            .range_member_values
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    crate::eval_env::LocalBinding::direct(value.clone()),
                )
            })
            .collect::<HashMap<_, _>>();
        template_bindings.extend(
            self.locals
                .fragment_values
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        let current_dot_fragment = self.current_dot_fragment();
        let current_value_dot = self.current_value_dot();
        let root_dot_identity = match current_value_dot.as_ref() {
            None => RootDotIdentity::Unresolved,
            Some(AbstractValue::RootContext) => RootDotIdentity::ExplicitRoot,
            Some(_) => RootDotIdentity::Other,
        };
        let current_dot = current_dot_fragment
            .as_ref()
            .map(AbstractValue::to_context_value)
            .or(current_value_dot);
        let mut eval_env = EvalEnv::from_helper_context(
            Some(&self.root_bindings),
            current_dot.as_ref(),
            self.current_dot_binding_mode(),
        )
        .without_helper_call_args()
        .with_predicate_memo(std::rc::Rc::clone(self.db.predicate_memo()));
        // Locals and root bindings are distinct namespaces (see the hole
        // evaluator); roots resolve through `root_fields` only.
        eval_env.locals = template_bindings;
        eval_env.local_default_paths = self.locals.default_paths.clone();
        eval_env.local_output_meta = self.locals.output_meta.clone();
        eval_env.local_scalar_dispatches = self.locals.scalar_dispatches.clone();
        eval_env.local_truthy_reductions = self.locals.truthy_reductions.clone();
        eval_env.root_truthy_predicates = self.root_truthy_predicates.clone();
        eval_env.root_value_dispatches = self.root_value_dispatches.clone();
        eval_env.root_field_semantics_on_current_dot =
            self.root_value_dot.is_some() && self.dot_stack.len() <= 1;
        eval_env.bound_values =
            BoundValueContext::new(&self.locals.range_domains, &self.locals.get_bindings);
        ValuePathContext {
            helper_dispatch_depth: std::cell::Cell::new(0),
            eval_env,
            template_truthiness_abstentions: &self.locals.truthiness_abstentions,
            typeof_bindings: &self.locals.typeof_sources,
            int_cast_bindings: &self.locals.int_cast_sources,
            fragment_context: FragmentEvalContext::new(self.db),
            current_dot_fragment,
            root_dot_identity,
        }
    }

    /// Record that rendering FAILS unconditionally under the currently
    /// active predicates (`fail` calls): no valid values document may
    /// satisfy them. The RAW predicates are kept — the guard-DNF
    /// conversion drops conjuncts it cannot represent, which negation
    /// cannot tolerate.
    pub(super) fn record_fail_condition(&mut self) {
        let capture = FailCapture {
            conjunction: self.fail_capture_conjunction(Vec::new()),
            ranged: self.capture_ranged_modes(),
            kind: CaptureKind::Fail,
        };
        if capture
            .conjunction
            .iter()
            .any(|p| matches!(p.kind(), helm_schema_core::PredicateKind::False))
        {
            return;
        }
        self.observed_facts.captures.insert(capture);
    }

    /// Record a `required(message, subject)` guardrail: rendering fails
    /// under the ambient predicates whenever the subject is Helm-empty
    /// (absent, null, or the empty string).
    pub(super) fn record_required_condition(&mut self, subject_path: &str) {
        let empty = Predicate::Or(vec![
            Predicate::from(Guard::Absent {
                path: helm_schema_core::ValuesPath::parse(subject_path),
            }),
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse(subject_path),
                value: helm_schema_core::GuardValue::Null,
            }),
            Predicate::from(Guard::Eq {
                path: helm_schema_core::ValuesPath::parse(subject_path),
                value: helm_schema_core::GuardValue::string(""),
            }),
        ]);
        let capture = FailCapture {
            conjunction: self.fail_capture_conjunction(vec![empty]),
            ranged: self.capture_ranged_modes(),
            kind: CaptureKind::Fail,
        };
        if capture
            .conjunction
            .iter()
            .any(|p| matches!(p.kind(), helm_schema_core::PredicateKind::False))
        {
            return;
        }
        self.observed_facts.captures.insert(capture);
    }

    /// The ambient predicates plus `tail`.
    pub(super) fn fail_capture_conjunction(
        &self,
        tail: impl IntoIterator<Item = Predicate>,
    ) -> helm_schema_core::Conjunction {
        fn append(predicate: Predicate, out: &mut Vec<Predicate>) {
            match predicate.kind() {
                helm_schema_core::PredicateKind::True => {}
                helm_schema_core::PredicateKind::And(items)
                    if !predicate.contains_approximation() =>
                {
                    for item in items.iter().cloned() {
                        append(item, out);
                    }
                }
                _ => out.push(predicate),
            }
        }

        let mut predicates = Vec::new();
        for predicate in self
            .active_predicates
            .iter()
            .chain(&self.alternative_capture_approximates)
            .cloned()
            .chain(tail)
        {
            append(predicate, &mut predicates);
        }
        helm_schema_core::Conjunction::new(predicates)
    }

    /// The exact range facts active at a fail capture site.
    pub(super) fn capture_ranged_modes(&self) -> crate::range_modes::RangeModes {
        let mut ranged = crate::range_modes::RangeModes::default();
        for (path, mode) in &self.active_range_modes {
            if mode.input_identity {
                ranged.mark_input_identity(path);
            }
            if mode.member_identity {
                ranged.mark_member_identity(path);
            }
            if mode.json_decoded {
                ranged.mark_json_decoded(path);
            }
            if mode.destructured {
                ranged.mark_destructured(path);
            }
        }
        ranged
    }

    pub(super) fn ambient_condition(&self) -> GuardDnf {
        GuardDnf::from_conjunction(self.active_predicates.iter().cloned())
    }

    fn record_parent_shell(&mut self, node_start: usize, shell: ParentShell) {
        let condition = self
            .db
            .predicate_memo()
            .normalize(Predicate::all(self.active_predicates.clone()));
        let arm = ParentShellArm {
            source_start: node_start,
            condition,
            shell,
        };
        let arms = self.evaluated_parent_shells.entry(node_start).or_default();
        if !arms.contains(&arm) {
            arms.push(arm);
        }
    }

    pub(super) fn push_predicate(&mut self, predicate: Predicate) {
        // `False` is load-bearing: a decoded-dead branch (a `hasKey` probe
        // into a folded literal table that misses) must poison the captures
        // recorded under it. Only `True` is skippable noise.
        if !matches!(predicate.kind(), helm_schema_core::PredicateKind::True)
            && !self.active_predicates.contains(&predicate)
        {
            self.active_predicates.push(predicate);
        }
    }

    /// Whether an enclosing condition's lowering is APPROXIMATE (truthy
    /// fallbacks, dropped conjuncts). Activation pushes the approximation
    /// onto the predicate stack as a [`Predicate::Approximate`] conjunct
    /// (later arms carry it inside the negated prior), so scanning the
    /// active predicates sees exactly the enclosing approximations. Rows
    /// tolerate wider conditions; string-contract metadata and fail
    /// NEGATION abstain under one.
    pub(super) fn under_approximate_condition(&self) -> bool {
        self.active_predicates
            .iter()
            .any(Predicate::contains_approximation)
    }

    /// A site-scoped pathless read: condition operands, bound-value reads,
    /// templated-key splices, and rendered-effect reads carry the current
    /// site's resource and provenance (the same scoping the emission
    /// terminal applied to their rows).
    pub(super) fn push_read(&mut self, values_path: &str, extra_guards: &[Guard]) {
        let (resource, provenance) = match &self.current_site {
            Some(site) => (
                site.resource.clone(),
                site.provenance.iter().cloned().collect(),
            ),
            None => (None, Vec::new()),
        };
        self.push_read_row(
            values_path,
            crate::ValueKind::Scalar,
            extra_guards,
            resource,
            provenance,
            false,
        );
    }

    /// Records a control dependency without attributing the region's YAML sink.
    pub(super) fn push_control_read(&mut self, values_path: &str, extra_guards: &[Guard]) {
        let provenance = self
            .current_site
            .as_ref()
            .map(|site| site.provenance.iter().cloned().collect())
            .unwrap_or_default();
        self.push_read_row(
            values_path,
            crate::ValueKind::Scalar,
            extra_guards,
            None,
            provenance,
            false,
        );
    }

    pub(super) fn push_read_row(
        &mut self,
        values_path: &str,
        kind: crate::ValueKind,
        extra_guards: &[Guard],
        resource: Option<ResourceRef>,
        provenance: Vec<ContractProvenance>,
        dependency: bool,
    ) {
        let condition = self
            .ambient_condition()
            .conjoined_with_guards(extra_guards.iter().cloned());
        self.push_read_row_with_condition(
            helm_schema_core::ValuesPath::parse(values_path),
            kind,
            condition,
            resource,
            provenance,
            dependency,
        );
    }

    fn push_read_row_with_condition(
        &mut self,
        values_path: helm_schema_core::ValuesPath,
        kind: crate::ValueKind,
        condition: GuardDnf,
        resource: Option<ResourceRef>,
        provenance: Vec<ContractProvenance>,
        dependency: bool,
    ) {
        if values_path.segments().next().is_none() {
            return;
        }
        let read = ValueRead {
            values_path,
            kind,
            condition,
            resource,
            provenance,
            dependency,
        };
        if self.reads_seen.insert(read.clone()) {
            self.reads.push(read);
        }
    }

    /// Pathless reads for the splices of a templated mapping key. Keys have
    /// no guarded arms in the tree, so their reads are recorded at the eval
    /// site where the ambient predicates (branch and range conditions) are
    /// still active; the projection deliberately does not re-derive them.
    pub(super) fn push_key_reads(&mut self, key: &EntryKey) {
        let EntryKey::Dynamic(string) = key else {
            return;
        };
        // A key spliced WHOLE is an unquoted plain key, so a ranged
        // collection's key must keep that token intact: external-dns emits
        // `  {{ $key }}: …` under a Secret's `data:`, where a key holding
        // `: ` opens a nested mapping and Helm's decode fails.
        if let [StringPart::Splice(splice)] = string.parts.as_slice()
            && splice.meta.range_key
            && splice.values_path.segments().len() != 0
            && !self.helper_scope
        {
            let capture = crate::eval_effect::FailCapture {
                conjunction: self.fail_capture_conjunction(Vec::new()),
                ranged: self.capture_ranged_modes(),
                kind: crate::eval_effect::CaptureKind::RangeKeyPlainSlot {
                    paths: [splice.values_path.clone()].into_iter().collect(),
                },
            };
            self.observed_facts.captures.insert(capture);
        }
        for part in &string.parts {
            match part {
                StringPart::Text(_) => {}
                // A rendered RANGE KEY in a templated key says nothing about
                // the collection's VALUE domain.
                StringPart::Splice(splice) if splice.meta.range_key => {}
                StringPart::Splice(splice) => {
                    let mut extra = Vec::new();
                    if splice.meta.defaulted {
                        extra.push(Guard::Default {
                            path: splice.values_path.clone(),
                        });
                    }
                    // A key position formats every SCALAR (a numeric label
                    // key renders `7:` and YAML-to-JSON stringifies it), so
                    // the read carries the partial-scalar kind: a declared
                    // string default widens to the scalar union instead of
                    // pinning the raw input as a string.
                    let (resource, provenance) = match &self.current_site {
                        Some(site) => (
                            site.resource.clone(),
                            site.provenance.iter().cloned().collect(),
                        ),
                        None => (None, Vec::new()),
                    };
                    self.push_read_row(
                        &splice.values_path.encode(),
                        crate::ValueKind::PartialScalar,
                        &extra,
                        resource,
                        provenance,
                        false,
                    );
                }
                StringPart::Taint(taint) => {
                    if !taint.claims_value_kind {
                        continue;
                    }
                    for path in &taint.paths {
                        self.push_read(&path.encode(), &[]);
                    }
                }
            }
        }
    }

    /// Pathless reads for a helper meta row retain its predicate disjunction
    /// as one condition. Helper rows have no site resource; their provenance
    /// is the read site's plus the helper body sites recorded in the meta.
    pub(super) fn push_meta_reads(
        &mut self,
        values_path: &str,
        kind: crate::ValueKind,
        meta: &HelperOutputMeta,
        sibling_claims: &BTreeSet<String>,
        dependency: bool,
    ) {
        let helper_condition = if meta.predicates.is_empty() {
            GuardDnf::unconditional()
        } else {
            GuardDnf::from_disjunction(meta.predicates.iter().map(|branch| branch.iter().cloned()))
        };
        let mut provenance: Vec<ContractProvenance> = self
            .current_site
            .as_ref()
            .and_then(|site| site.provenance.clone())
            .into_iter()
            .collect();
        merge_provenance_sites(&mut provenance, &meta.provenance);
        let mut condition = self
            .claim_scoped_ambient_condition(values_path, sibling_claims)
            .conjoined(&helper_condition);
        if meta.defaulted {
            condition = condition.conjoined_with_guards([Guard::Default {
                path: helm_schema_core::ValuesPath::parse(values_path),
            }]);
        }
        self.push_read_row_with_condition(
            helm_schema_core::ValuesPath::parse(values_path),
            kind,
            condition,
            None,
            provenance,
            dependency,
        );
    }

    /// The ambient condition scoped to one helper claim: a truthiness
    /// condition about a *different* claim path of the same call describes a
    /// sibling's branch, not this row's, and is dropped unless the paths are
    /// related (the summary lane's sibling-source rule).
    fn claim_scoped_ambient_condition(
        &self,
        claim_path: &str,
        sibling_claims: &BTreeSet<String>,
    ) -> GuardDnf {
        if !self.helper_scope {
            return GuardDnf::from_conjunction(self.active_predicates.iter().cloned());
        }
        GuardDnf::from_conjunction(
            self.active_predicates
                .iter()
                .filter(|predicate| {
                    let path = match predicate.kind() {
                        helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) => path,
                        helm_schema_core::PredicateKind::Not(inner) => match inner.kind() {
                            helm_schema_core::PredicateKind::Guard(Guard::Truthy { path }) => path,
                            _ => return true,
                        },
                        _ => return true,
                    };
                    let path = path.encode();
                    path == claim_path
                        || !sibling_claims.contains(&path)
                        || crate::helper_meta::values_paths_are_related(&path, claim_path)
                })
                .cloned(),
        )
    }

    /// Values paths a strict STRING consumer captured so far, regardless
    /// of branch scope: a program-wrapper map reaching such a consumer raw
    /// aborts rendering (`trunc`/`contains` type-assert strings, and a
    /// wrapper map is truthy, so even self-guarded consumers run). The
    /// pre-rewrite wrapper-exclusion snapshot reads this — conditional
    /// captures count because engines guard their whole body with an
    /// idempotence flag exactly as conditional as the rewrite itself.
    pub(super) fn strict_string_capture_paths(&self) -> BTreeSet<helm_schema_core::ValuesPath> {
        let mut paths = BTreeSet::new();
        for capture in &self.observed_facts.captures {
            match &capture.kind {
                crate::eval_effect::CaptureKind::ValueType {
                    path, schema_type, ..
                } if schema_type == "string" => {
                    paths.insert(path.clone());
                }
                crate::eval_effect::CaptureKind::StringRequirement { path, .. }
                | crate::eval_effect::CaptureKind::ValuePattern { path, .. } => {
                    paths.insert(path.clone());
                }
                _ => {}
            }
        }
        paths.retain(|path| {
            path.segments().next().is_some()
                && !path
                    .segments()
                    .any(helm_schema_core::Segment::is_each_member)
        });
        paths
    }

    pub(super) fn absorb_scoped_captures<'capture>(
        &mut self,
        captures: impl IntoIterator<Item = &'capture FailCapture>,
    ) {
        for capture in self.scope_captures(captures) {
            self.observed_facts.captures.insert(capture);
        }
    }

    /// Record captures that hold only where this source's rendered text is
    /// consumed as YAML, at a site that certified exactly that. Inside a
    /// helper body the sink is still the caller's to certify, so they defer
    /// once more instead of binding here.
    pub(super) fn record_yaml_text_captures<'capture>(
        &mut self,
        captures: impl IntoIterator<Item = &'capture FailCapture>,
    ) {
        for capture in self.scope_captures(captures) {
            if self.helper_scope {
                self.text_captures.insert(capture);
            } else {
                self.observed_facts.captures.insert(capture);
            }
        }
    }

    fn scope_captures<'capture>(
        &self,
        captures: impl IntoIterator<Item = &'capture FailCapture>,
    ) -> Vec<FailCapture> {
        let mut scoped = Vec::new();
        for body_capture in captures {
            let (conjunction, ranged) = super::capture_scope::scope_capture_conditions(
                body_capture,
                self.fail_capture_conjunction(Vec::new()),
                self.capture_ranged_modes(),
            );
            let mut kind = body_capture.kind.clone();
            // Ambient execution scope turns a direct string contract into an implication.
            // Its conjunction is the complete scope.
            // Operand truthiness would admit consumed falsy values.
            if let CaptureKind::StringRequirement {
                path,
                route,
                selection,
            } = &kind
                && matches!(route, crate::eval_effect::StringRequirementRoute::Direct)
                && !conjunction.is_empty()
            {
                kind = CaptureKind::StringRequirement {
                    path: path.clone(),
                    route: crate::eval_effect::StringRequirementRoute::Scoped,
                    selection: selection.clone(),
                };
            }
            if let CaptureKind::MemberAccess { handled_kinds } = &mut kind {
                let target = conjunction
                    .iter()
                    .find_map(|predicate| match predicate.kind() {
                        helm_schema_core::PredicateKind::Not(inner) => match inner.kind() {
                            helm_schema_core::PredicateKind::Guard(Guard::TypeIs {
                                path,
                                schema_type,
                            }) if schema_type == "object" => Some(path),
                            _ => None,
                        },
                        _ => None,
                    });
                if let Some(target) = target {
                    handled_kinds.extend(
                        self.member_host_conversions
                            .iter()
                            .filter(|conversion| {
                                conversion.path == *target
                                    && conversion
                                        .outer_predicates
                                        .iter()
                                        .all(|predicate| conjunction.contains(predicate))
                            })
                            .map(|conversion| conversion.input_kind.clone()),
                    );
                }
            }
            let capture = FailCapture {
                conjunction,
                ranged,
                kind,
            };
            if capture.kind.sole_value_path().is_some_and(|path| {
                path.segments()
                    .any(helm_schema_core::Segment::is_each_member)
            }) && capture.requirement_is_implied_by(&Predicate::all(
                capture.conjunction.iter().cloned().collect(),
            )) {
                continue;
            }
            if capture
                .conjunction
                .iter()
                .any(|p| matches!(p.kind(), helm_schema_core::PredicateKind::False))
            {
                continue;
            }
            scoped.push(capture);
        }
        scoped
    }

    pub(super) fn absorb_helper_reads_with_suppression(
        &mut self,
        reads: &[ValueRead],
        suppressed: &BTreeSet<String>,
        sibling_claims: &BTreeSet<String>,
    ) {
        let site_provenance: Vec<ContractProvenance> = self
            .current_site
            .as_ref()
            .and_then(|site| site.provenance.clone())
            .into_iter()
            .collect();
        for read in reads {
            // Guard-path reads that are strict ancestors of a predicate path
            // the helper explicitly severed (index-call narrowing) are
            // dropped, the same way the summary lane always skipped them.
            if !read.dependency
                && !suppressed.contains(&read.values_path.encode())
                && suppressed.iter().any(|narrowed| {
                    helm_schema_core::ValuesPath::parse(narrowed)
                        .is_descendant_of(&read.values_path)
                })
            {
                continue;
            }
            let mut provenance = site_provenance.clone();
            merge_provenance_sites(&mut provenance, &read.provenance);
            let condition = self
                .claim_scoped_ambient_condition(&read.values_path.encode(), sibling_claims)
                .conjoined(&read.condition);
            self.push_read_row_with_condition(
                read.values_path.clone(),
                read.kind,
                condition,
                read.resource.clone(),
                provenance,
                read.dependency,
            );
        }
    }

    fn escaped_control<'n>(&self, view: NodeView<'n>) -> Option<(&'n ControlRegion, usize)> {
        let controls = self
            .body_facts
            .adoption_plan
            .controls
            .get(&view.node.span_start())?;
        let mut cursor = view.control_cursor;
        if view.omitted_control.is_some_and(|omitted| {
            controls
                .get(cursor)
                .is_some_and(|planned| planned.control_start <= omitted)
        }) {
            let omitted = view.omitted_control?;
            cursor = controls.partition_point(|planned| planned.control_start <= omitted);
        }
        let planned = controls.get(cursor)?;
        if !view.window.contains(planned.control_start) {
            return None;
        }
        Some((control_at_path(view.node, &planned.path)?, cursor + 1))
    }

    fn cursor_after_control(&self, view: NodeView<'_>, control_start: usize) -> usize {
        self.body_facts
            .adoption_plan
            .control_positions
            .get(&(view.node.span_start(), control_start))
            .copied()
            .map_or(view.control_cursor, |cursor| {
                cursor.max(view.control_cursor)
            })
    }

    pub(super) fn eval_node_list(&mut self, nodes: &[NodeView<'_>]) -> Contributions {
        // The precomputed path identifies the outermost escaped container for each control.
        // Ordering that container at the control opener keeps its complete ancestor chain and
        // eager holes in the source arm that executed them.
        let mut ordered = nodes
            .iter()
            .copied()
            .map(|view| {
                let control = self.escaped_control(view);
                (
                    control.map_or_else(|| view.node.span_start(), |(region, _)| region.span.start),
                    view,
                    control,
                )
            })
            .collect::<Vec<_>>();
        ordered.sort_by_key(|(start, _, _)| *start);
        let evaluation_starts = ordered
            .iter()
            .map(|(start, _, _)| *start)
            .collect::<Vec<_>>();
        let embedded_controls = ordered
            .iter()
            .map(|(_, _, control)| *control)
            .collect::<Vec<_>>();
        let ordered = ordered
            .into_iter()
            .map(|(_, view, _)| view)
            .collect::<Vec<_>>();
        let nodes = &ordered;
        let nodes_by_start = nodes
            .iter()
            .map(|view| (view.node.span_start(), *view))
            .collect::<HashMap<_, _>>();
        let mut out = Contributions::default();
        let mut index = 0;
        let mut remaining = Predicate::True;
        while let Some(view) = nodes.get(index) {
            if remaining == Predicate::False {
                break;
            }
            let local_entry = self.locals.clone();
            let entry_scope = self.mark_scope();
            self.push_predicate(remaining.clone());
            let mut next = Contributions::default();
            let control = match view.node {
                Node::Control(region) => Some((region, false, view.control_cursor)),
                _ => embedded_controls[index]
                    .map(|(region, next_cursor)| (region, true, next_cursor)),
            };
            if let Some((region, embedded, next_cursor)) = control {
                // Re-adopt nodes that escaped an ill-nested region: their spans still lie inside
                // the region, so they belong to a branch body with its guards and dot bindings.
                // The adopted view stops at the next branch boundary.
                // Later branch and post-region descendants retain the same
                // container chain without reevaluating its eager holes.
                let mut adopted = Vec::new();
                if embedded {
                    let (_, branch) = branch_window(region, view.node.span_start());
                    adopted.push(Adopted {
                        view: NodeView {
                            node: view.node,
                            window: SourceWindow {
                                start: branch.start.max(view.window.start),
                                end: Some(view.window.end.map_or_else(
                                    || branch.end.unwrap_or(region.span.end),
                                    |end| end.min(branch.end.unwrap_or(region.span.end)),
                                )),
                            },
                            omitted_control: Some(region.span.start),
                            control_cursor: next_cursor,
                        },
                        defer_window: view.window,
                    });
                }
                while let Some(next) = nodes.get(index + 1) {
                    if next.node.span_start() < region.span.end {
                        // In-scope evaluation is bounded by the innermost
                        // region end; deferral hands descendants past this
                        // region (but within the enclosing bound) back to
                        // this region, and the rest to the enclosing one.
                        let (_, branch) = branch_window(region, next.node.span_start());
                        let in_scope = SourceWindow {
                            start: branch.start.max(next.window.start),
                            end: Some(next.window.end.map_or_else(
                                || branch.end.unwrap_or(region.span.end),
                                |end| end.min(branch.end.unwrap_or(region.span.end)),
                            )),
                        };
                        adopted.push(Adopted {
                            view: NodeView {
                                node: next.node,
                                window: in_scope,
                                omitted_control: Some(
                                    next.omitted_control.map_or(region.span.start, |omitted| {
                                        omitted.max(region.span.start)
                                    }),
                                ),
                                control_cursor: self.cursor_after_control(*next, region.span.start),
                            },
                            defer_window: next.window,
                        });
                        index += 1;
                    } else {
                        break;
                    }
                }
                // Descendants of *earlier* siblings that escaped forward
                // into the region (a branch contributing to a container
                // opened before it) belong to branch bodies too; their
                // in-place evaluation was bounded at the region start.
                let mut escaped = Vec::new();
                for prior_start in self
                    .body_facts
                    .adoption_plan
                    .crossing_priors
                    .get(&region.span.start)
                    .into_iter()
                    .flatten()
                {
                    let Some(prior) = nodes_by_start.get(prior_start) else {
                        continue;
                    };
                    let mut chain = Vec::new();
                    super::control::collect_deferred(
                        prior.node,
                        SourceWindow {
                            start: region.span.start,
                            end: prior.window.end,
                        },
                        prior.omitted_control,
                        &self.body_facts.adoption_plan,
                        &self.evaluated_parent_shells,
                        &mut chain,
                        &mut escaped,
                    );
                }
                next.extend(self.eval_control(region, &adopted, escaped));
            } else {
                match view.node {
                    Node::Output(action) => {
                        let consumed =
                            self.eval_output_with_lookahead(action, nodes, index, &mut next);
                        index += consumed;
                    }
                    _ => {
                        // Evaluation stops at the next control sibling's start:
                        // descendants escaping into that region evaluate inside
                        // its branches instead of unguarded in place.
                        let mut bounded = *view;
                        if let Some(region_start) = evaluation_starts.get(index + 1).copied() {
                            bounded.window = bounded.window.with_end(region_start);
                        }
                        next.extend(self.eval_node(bounded));
                    }
                }
            }
            self.rewind(entry_scope);
            if remaining != Predicate::True {
                let executed = self.locals.clone();
                let skipped = local_entry.clone();
                self.locals.join_control_outcomes(
                    &local_entry,
                    &[
                        crate::symbolic_local_state::ControlOutcome::new(
                            TruthCondition::exact_with_memo(
                                remaining.clone(),
                                self.db.predicate_memo().as_ref(),
                            ),
                            executed,
                        ),
                        crate::symbolic_local_state::ControlOutcome::new(
                            TruthCondition::exact_with_memo(
                                remaining.clone().negated(),
                                self.db.predicate_memo().as_ref(),
                            ),
                            skipped,
                        ),
                    ],
                    self.db.predicate_memo().as_ref(),
                );
            }
            let exit_condition = next.loop_control.exit_condition();
            next.guard_all(&remaining, self.db.predicate_memo().as_ref());
            out.extend(next);
            remaining = if exit_condition == Predicate::False {
                remaining
            } else if exit_condition == Predicate::True {
                Predicate::False
            } else {
                and_conditions_with_memo(
                    remaining,
                    exit_condition.negated(),
                    self.db.predicate_memo().as_ref(),
                )
            };
            index += 1;
        }
        out.repair_valueless_mapping_header();
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
            let (value, width) = self.eval_output_action(action.span);
            // Every standalone splice floats: a stated width lands exactly,
            // and a bare one still renders at its own column, which bounds
            // how far out it can belong. Keeping widthless splices out of the
            // float pool used to pin them to whichever container the CST left
            // open, however much deeper that was than their own line.
            out.floating.push(FloatingOutput {
                width: width.unwrap_or_else(|| self.line_indent(action.span.start)),
                column_only: width.is_none(),
                origin: action.span.start,
                value,
            });
            return 0;
        };

        // `{{ key }}…: …` — a dynamic mapping entry: the action (plus any
        // literal key suffix before the structural colon) is the key; the
        // inline value is the literal text after the colon or a same-line
        // action.
        let previous_site = self.enter_hole_site(action.span);
        let mut key_string = self.hole_string(action.span);
        if !key_suffix.is_empty() {
            key_string
                .parts
                .push(StringPart::Text([key_suffix].into_iter().collect()));
        }
        let key = EntryKey::Dynamic(key_string);
        self.push_key_reads(&key);
        self.restore_site(previous_site);
        let mut consumed = 1;
        let value = if rest.trim().is_empty() {
            match nodes.get(index + 2).map(|view| view.node) {
                Some(Node::Output(value_action))
                    if self.same_line(text_span.end, value_action.span.start) =>
                {
                    consumed = 2;
                    let previous_slot = std::mem::replace(&mut self.in_value_slot, true);
                    let value = self.eval_entire_hole(value_action.span);
                    self.in_value_slot = previous_slot;
                    value
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
                    for (_, hole_parts) in self.eval_hole_parts(action.span) {
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
                                for (_, hole_parts) in self.eval_hole_parts(*span) {
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

    /// The rendered indent of a templated mapping-entry line (the key
    /// hole's explicit `nindent` width when present, else the line indent).
    pub(super) fn dynamic_entry_render_indent(&self, span: Span) -> usize {
        let line_start = self
            .source
            .get(..span.start)
            .and_then(|prefix| prefix.rfind('\n'))
            .map_or(0, |newline| newline + 1);
        let line_end = self
            .source
            .get(span.start..)
            .and_then(|rest| rest.find('\n'))
            .map_or(self.source.len(), |offset| span.start + offset);
        let line = self.source.get(line_start..line_end).unwrap_or("");
        for expr in helm_schema_ast::parse_action_expressions(line) {
            if let Some(width) = expr.fragment_indent_width() {
                return width;
            }
        }
        self.line_indent(span.start)
    }

    /// Minimum caller-relative indent of a helper's rendered root.
    ///
    /// A left-trimmed bare output at the helper root removes its source
    /// indentation before emitting text. Its caller's `nindent` therefore
    /// starts from column zero; adding the helper source column would place
    /// an appended list fragment inside the preceding item's last field.
    pub(super) fn helper_root_content_indent(&self, node: &Node) -> Option<usize> {
        match node {
            Node::Mapping(entry) => Some(entry.indent),
            Node::Sequence(item) => Some(item.indent),
            Node::Scalar(line) => Some(line.indent),
            Node::Control(region) => region
                .branches
                .iter()
                .flat_map(|branch| &branch.body)
                .filter_map(|child| self.helper_root_content_indent(child))
                .min(),
            Node::Output(action)
                if parse_expr_text(self.text(action.span))
                    .iter()
                    .rev()
                    .find_map(TemplateExpr::fragment_indent_width)
                    .is_none()
                    && self.text(action.span).trim_start().starts_with("{{-") =>
            {
                Some(0)
            }
            Node::Output(action) => Some(self.output_render_indent(action.span)),
            Node::Comment(_) | Node::Opaque(_) => None,
        }
    }

    /// The column a bare output's text renders at: the width its expression
    /// states, else the action's own source column.
    pub(super) fn output_render_indent(&self, span: Span) -> usize {
        parse_expr_text(self.text(span))
            .iter()
            .rev()
            .find_map(TemplateExpr::fragment_indent_width)
            .unwrap_or_else(|| self.line_indent(span.start))
    }

    /// The indentation of the line containing `byte`.
    pub(super) fn line_indent(&self, byte: usize) -> usize {
        let line_start = self
            .source
            .get(..byte)
            .and_then(|prefix| prefix.rfind('\n'))
            .map_or(0, |newline| newline + 1);
        self.source
            .get(line_start..)
            .map_or(0, |line| line.len() - line.trim_start_matches(' ').len())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "keeping this semantic operation together makes its state transitions easier to audit"
    )]
    fn eval_node(&mut self, view: NodeView<'_>) -> Contributions {
        let mut out = Contributions::default();
        match view.node {
            Node::Mapping(entry) => {
                let previous_site = self.enter_hole_site(entry.key.span);
                let key = self.entry_key(&entry.key);
                self.record_parent_shell(entry.span.start, ParentShell::Entry(key.clone()));
                self.push_key_reads(&key);
                self.restore_site(previous_site);
                let mut value = Guarded::empty();
                let block_holds_yaml = key_names_yaml_document(&key);
                if let Some(block) = &entry.block {
                    let previous_block =
                        std::mem::replace(&mut self.block_text_is_yaml, block_holds_yaml);
                    value.extend(self.eval_block_scalar(block));
                    self.block_text_is_yaml = previous_block;
                }
                if let Some(parts) = &entry.value {
                    let previous_slot = std::mem::replace(&mut self.in_value_slot, true);
                    let evaluated = self.eval_scalar_parts(parts);
                    self.in_value_slot = previous_slot;
                    // A sourced value hole that lowered to nothing still
                    // occupies the value position: without an arm the entry
                    // would read as an OPEN header and adopt a following
                    // floated splice as its own value.
                    if evaluated.is_empty() {
                        value.extend(Guarded::unconditional(AbstractFragment::Opaque(
                            Opaque::default(),
                        )));
                    } else {
                        value.extend(evaluated);
                    }
                }
                let (mut children, siblings) = self.split_structural_children(
                    view,
                    ParentKind::Entry,
                    entry.indent,
                    entry.value.is_none() && entry.block.is_none(),
                );
                // A bare output hanging at or above a block-scalar entry's
                // indent (`key: |` followed by a column-0 `{{- include … }}`)
                // renders into the still-open block whenever its text is
                // deeper than the entry. The CST can retain that escaped
                // output as either a direct child or a sibling; both are
                // block text, not structure in the parent container.
                let siblings = if let Some(block) = &entry.block {
                    let mut adopted = Vec::new();
                    let mut escaped_controls = Vec::new();
                    let mut remaining_children = Vec::new();
                    for child in children {
                        match child.node {
                            Node::Output(_) => adopted.push(child),
                            Node::Control(region)
                                if self.control_renders_below_block(region, entry.indent) =>
                            {
                                adopted.push(child);
                            }
                            Node::Control(region) if region.span.start < block.body.end => {
                                escaped_controls.push(child);
                            }
                            _ => remaining_children.push(child),
                        }
                    }
                    children = remaining_children;
                    let (adopted_siblings, rest): (Vec<_>, Vec<_>) = siblings
                        .into_iter()
                        .partition(|child| matches!(child.node, Node::Output(_)));
                    adopted.extend(adopted_siblings);
                    let previous_block =
                        std::mem::replace(&mut self.block_text_is_yaml, block_holds_yaml);
                    for adopted_view in adopted {
                        match adopted_view.node {
                            Node::Output(action) => {
                                value.extend(self.eval_block_adopted_output(action.span));
                            }
                            Node::Control(region) => {
                                value.extend(self.eval_block_adopted_control(region.span));
                            }
                            _ => {}
                        }
                    }
                    self.block_text_is_yaml = previous_block;
                    for escaped_view in escaped_controls {
                        let Node::Control(region) = escaped_view.node else {
                            continue;
                        };
                        let value = self.eval_structural_control_output(region.span);
                        let width = self
                            .control_render_indent(region.span)
                            .unwrap_or_else(|| self.line_indent(region.span.start));
                        out.floating.push(FloatingOutput {
                            width,
                            column_only: false,
                            origin: region.span.start,
                            value,
                        });
                    }
                    rest
                } else {
                    siblings
                };
                let mut child = self.eval_node_list(&children);
                let siblings = self.float_escaping_outputs(siblings, &mut child);
                let opened_empty = entry.value.is_none() && entry.block.is_none();
                let marked_at = content_child_mark(&entry.children, entry.indent);
                let content_at = self
                    .body_facts
                    .adoption_plan
                    .parent_shapes
                    .get(&entry.span.start)
                    .and_then(|shape| shape.established_content_mark);
                value.extend(child.take_floating_below(
                    entry.indent,
                    opened_empty,
                    marked_at,
                    content_at,
                ));
                out.floating.append(&mut child.floating);
                out.loop_control.extend(child.take_loop_control());
                value.extend(child.assemble());
                out.merge_entry(key, value);
                if !siblings.is_empty() {
                    out.extend(self.eval_node_list(&siblings));
                }
            }
            Node::Sequence(item) => {
                self.record_parent_shell(item.span.start, ParentShell::Item);
                let mut value = Guarded::empty();
                if let Some(block) = &item.block {
                    value.extend(self.eval_block_scalar(block));
                }
                if let Some(parts) = &item.value {
                    let previous_slot = std::mem::replace(&mut self.in_value_slot, true);
                    value.extend(self.eval_scalar_parts(parts));
                    self.in_value_slot = previous_slot;
                }
                let (mut children, siblings) =
                    self.split_structural_children(view, ParentKind::Item, item.indent, false);
                // `- |` items adopt shallow bare outputs as block text, the
                // same as block-scalar mapping entries above. The CST can
                // retain an escaped output as a child or a sibling.
                let siblings = if let Some(block) = &item.block {
                    let mut adopted = Vec::new();
                    let mut escaped_controls = Vec::new();
                    let mut remaining_children = Vec::new();
                    for child in children {
                        match child.node {
                            Node::Output(_) => adopted.push(child),
                            Node::Control(region)
                                if self.control_renders_below_block(region, item.indent) =>
                            {
                                adopted.push(child);
                            }
                            Node::Control(region) if region.span.start < block.body.end => {
                                escaped_controls.push(child);
                            }
                            _ => remaining_children.push(child),
                        }
                    }
                    children = remaining_children;
                    let (adopted_siblings, rest): (Vec<_>, Vec<_>) = siblings
                        .into_iter()
                        .partition(|child| matches!(child.node, Node::Output(_)));
                    adopted.extend(adopted_siblings);
                    for adopted_view in adopted {
                        match adopted_view.node {
                            Node::Output(action) => {
                                value.extend(self.eval_block_adopted_output(action.span));
                            }
                            Node::Control(region) => {
                                value.extend(self.eval_block_adopted_control(region.span));
                            }
                            _ => {}
                        }
                    }
                    for escaped_view in escaped_controls {
                        let Node::Control(region) = escaped_view.node else {
                            continue;
                        };
                        let value = self.eval_structural_control_output(region.span);
                        let width = self
                            .control_render_indent(region.span)
                            .unwrap_or_else(|| self.line_indent(region.span.start));
                        out.floating.push(FloatingOutput {
                            width,
                            column_only: false,
                            origin: region.span.start,
                            value,
                        });
                    }
                    rest
                } else {
                    siblings
                };
                let mut child = self.eval_node_list(&children);
                let siblings = self.float_escaping_outputs(siblings, &mut child);
                // Items never accept same-indent output (the open-slot
                // query pushes item frames without that allowance).
                value.extend(child.take_floating_below(item.indent, false, None, None));
                out.floating.append(&mut child.floating);
                out.loop_control.extend(child.take_loop_control());
                value.extend(child.assemble());
                out.items.push(value);
                if !siblings.is_empty() {
                    out.extend(self.eval_node_list(&siblings));
                }
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
                self.eval_assignment_span(opaque.span);
            }
            Node::Opaque(opaque) if opaque.kind == OpaqueKind::Break => {
                out.loop_control
                    .breaks
                    .push(crate::symbolic_local_state::ControlOutcome::new(
                        TruthCondition::exact(Predicate::True),
                        self.locals.clone(),
                    ));
            }
            Node::Opaque(opaque) if opaque.kind == OpaqueKind::Continue => {
                out.loop_control
                    .continues
                    .push(crate::symbolic_local_state::ControlOutcome::new(
                        TruthCondition::exact(Predicate::True),
                        self.locals.clone(),
                    ));
            }
            Node::Control(_) | Node::Output(_) | Node::Comment(_) | Node::Opaque(_) => {}
        }
        out
    }

    /// Move bare outputs that render OUTSIDE the container they were nested
    /// under into the float pool, keyed by the column they render at, and
    /// return the siblings that stay.
    ///
    /// The CST attaches an action-only line to whatever entry was still open,
    /// which can sit several levels below the column the line renders at:
    /// vault writes `{{ template "injector.strategy" . }}` at column 2 under a
    /// `matchLabels:` whose members are at 6, and signoz's service account ends
    /// `name: {{ include … }}` — a templated value opens a scope — before a
    /// column-0 `{{- include "signoz.imagePullSecrets" . }}`. Evaluating such
    /// a line beside its container strands it exactly ONE level up; the float
    /// pool already knows how to carry content out until some container's own
    /// indent contains it, so the escape becomes transitive by construction.
    fn float_escaping_outputs<'n>(
        &mut self,
        siblings: Vec<NodeView<'n>>,
        out: &mut Contributions,
    ) -> Vec<NodeView<'n>> {
        let mut ordered = siblings;
        ordered.sort_by_key(|view| view.node.span_start());
        let mut kept = Vec::with_capacity(ordered.len());
        for (index, view) in ordered.iter().copied().enumerate() {
            let Node::Output(action) = view.node else {
                kept.push(view);
                continue;
            };
            // `{{ key }}: value` is a mapping-entry line, not a bare splice:
            // its trailing literal text has to stay beside the action so the
            // dynamic-key lookahead still sees it.
            let opens_dynamic_entry = matches!(
                ordered.get(index + 1).map(|next| next.node),
                Some(Node::Opaque(opaque)) if opaque.kind == OpaqueKind::ActionLineText
            );
            if opens_dynamic_entry {
                kept.push(view);
                continue;
            }
            let (value, width) = self.eval_output_action(action.span);
            out.floating.push(FloatingOutput {
                width: width.unwrap_or_else(|| self.line_indent(action.span.start)),
                column_only: width.is_none(),
                origin: action.span.start,
                value,
            });
        }
        kept
    }

    /// Split a container's in-scope children into real children and nodes
    /// the layout recovery hung under the container: YAML containers hold
    /// strictly deeper content — except sequence items, which may sit at
    /// their parent key's own indent — so anything else at or above the
    /// container indent evaluates as a sibling at the container's level (the
    /// line model's pop-by-indent rule). Explicitly-indented outputs float;
    /// the float rules own their placement.
    fn split_structural_children<'n>(
        &self,
        view: NodeView<'n>,
        parent_kind: ParentKind,
        container_indent: usize,
        accepts_same_indent: bool,
    ) -> (Vec<NodeView<'n>>, Vec<NodeView<'n>>) {
        view.in_scope_children().into_iter().partition(|child| {
            self.node_belongs_inside(
                child.node,
                parent_kind,
                container_indent,
                accepts_same_indent,
            )
        })
    }

    pub(super) fn node_belongs_inside(
        &self,
        node: &Node,
        parent_kind: ParentKind,
        container_indent: usize,
        accepts_same_indent: bool,
    ) -> bool {
        match node {
            Node::Mapping(entry) => entry.indent > container_indent,
            Node::Sequence(item) => {
                item.indent > container_indent
                    || (item.indent == container_indent
                        && parent_kind == ParentKind::Entry
                        && accepts_same_indent)
            }
            Node::Scalar(line) => line.indent > container_indent,
            Node::Control(region) => {
                region
                    .branches
                    .iter()
                    .flat_map(|branch| &branch.body)
                    .all(|child| {
                        self.node_belongs_inside(
                            child,
                            parent_kind,
                            container_indent,
                            accepts_same_indent,
                        )
                    })
            }
            Node::Output(action) => {
                // Deeper lines always belong; the explicit-width probe (a
                // re-parse) only runs for the rare same-or-shallower case.
                self.line_indent(action.span.start) > container_indent
                    || parse_expr_text(self.text(action.span))
                        .iter()
                        .rev()
                        .any(|expr| expr.fragment_indent_width().is_some())
            }
            Node::Comment(_) | Node::Opaque(_) => true,
        }
    }

    fn control_renders_below_block(
        &self,
        region: &helm_schema_syntax::ControlRegion,
        block_indent: usize,
    ) -> bool {
        self.control_render_indent(region.span)
            .is_some_and(|indent| indent > block_indent)
    }

    pub(super) fn control_render_indent(&self, span: Span) -> Option<usize> {
        let text = self.text(span);
        let tree = parse_go_template(text)?;
        self.template_render_indent(tree.root_node(), text, span.start)
    }

    fn template_render_indent(
        &self,
        node: tree_sitter::Node<'_>,
        source: &str,
        source_offset: usize,
    ) -> Option<usize> {
        match node_action(source, node) {
            NodeAction::Text => {
                let text = node.utf8_text(source.as_bytes()).ok()?;
                text.split_inclusive('\n')
                    .scan(node.start_byte(), |line_start, line| {
                        let start = *line_start;
                        *line_start += line.len();
                        Some((start, line))
                    })
                    .filter_map(|(line_start, line)| {
                        let content_offset =
                            line.char_indices().find_map(|(offset, character)| {
                                (!character.is_whitespace()).then_some(offset)
                            })?;
                        Some(self.line_indent(source_offset + line_start + content_offset))
                    })
                    .min()
            }
            NodeAction::Output(expressions) => expressions
                .as_ref()
                .and_then(|expressions| {
                    expressions
                        .iter()
                        .rev()
                        .find_map(TemplateExpr::fragment_indent_width)
                })
                .or_else(|| Some(self.line_indent(source_offset + node.start_byte()))),
            NodeAction::If(_) | NodeAction::With | NodeAction::Range => {
                let mut cursor = node.walk();
                let mut minimum = None;
                if cursor.goto_first_child() {
                    loop {
                        if matches!(
                            cursor.field_name(),
                            Some("consequence" | "alternative" | "option")
                        ) && let Some(indent) =
                            self.template_render_indent(cursor.node(), source, source_offset)
                        {
                            minimum =
                                Some(minimum.map_or(indent, |current: usize| current.min(indent)));
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                minimum
            }
            NodeAction::Descend => {
                let mut cursor = node.walk();
                node.named_children(&mut cursor)
                    .filter_map(|child| self.template_render_indent(child, source, source_offset))
                    .min()
            }
            NodeAction::Suppressed | NodeAction::Assignment(_) => None,
        }
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
                    let hole = self.hole_string(*span);
                    string.parts.extend(hole.parts);
                }
            }
        }
        EntryKey::Dynamic(string)
    }

    /// Evaluate a hole into flattened string parts (conditions from
    /// alternatives are dropped; used for keys, where alternatives project
    /// pathlessly anyway).
    fn hole_string(&mut self, span: Span) -> AbstractString {
        let arms = self.eval_hole_parts(span);
        AbstractString {
            parts: arms.into_iter().flat_map(|(_, parts)| parts).collect(),
            suppressed: false,
        }
    }
}
