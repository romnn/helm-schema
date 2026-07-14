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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;

use helm_schema_ast::{
    ResourceSpan, TemplateExpr, TemplateHeader, parse_expr_text,
    range_has_destructured_variable_definition, range_header_from_source,
};
use helm_schema_syntax as syntax;
use helm_schema_syntax::{
    Node, OpaqueKind, ScalarPart, ScalarParts, Span, TemplatedDocument, parse_go_template,
};

use crate::abstract_value::AbstractValue;
use crate::analysis_db::IrAnalysisDb;
use crate::eval_effect::FailCapture;
use crate::fragment_expr_eval::FragmentEvalContext;
use crate::helper_meta::{HelperOutputMeta, merge_provenance_sites};
use crate::node_eval::control_header;
use crate::symbolic_local_state::SymbolicLocalState;
use crate::value_path_context::ValuePathContext;
use crate::{ContractProvenance, Guard, ResourceRef, SourceSpan};
use helm_schema_core::{GuardDnf, Predicate};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, Mapping, MappingEntry, PathCondition,
    Sequence, SiteFacts, StringPart,
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
    /// Declared input-type hints observed at unconditional rendered holes.
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Input-type hints observed only under branch predicates: they hold
    /// where those branches render, never at the unconditional base.
    pub(crate) guarded_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Paths consumed through total stringifications (`quote`, `toString`,
    /// `join`, `printf`) anywhere in the source: the chart tolerates any
    /// input type at them even when no placed row exists.
    pub(crate) shape_erased_paths: BTreeSet<String>,
    /// Paths carrying a real runtime string contract (`trunc`, `b64enc`,
    /// `fromYaml`, a dynamic `printf` format) somewhere in the source.
    pub(crate) string_contract_paths: BTreeSet<String>,
    /// Paths a `range` iterates DIRECTLY (`range .Values.x`), as opposed to
    /// ranging a derived expression over them: only these carry an iterable
    /// input domain.
    pub(crate) direct_range_source_paths: BTreeSet<String>,
    /// The subset of direct range sources iterated with TWO variables:
    /// integers iterate single-variable ranges only.
    pub(crate) destructured_range_source_paths: BTreeSet<String>,
    /// `fail` captures (see [`FailCapture`]): no valid values document may
    /// satisfy one of these conjunctions.
    pub(crate) fail_conditions: Vec<FailCapture>,
}

/// One pathless `.Values` read with the guards active at the read site.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueRead {
    /// The dotted `.Values` path that was read.
    pub values_path: String,
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
    let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
    let contributions = interpreter.eval_node_list(&roots);
    EvaluatedDocument {
        root: contributions.assemble(),
        reads: interpreter.reads,
        type_hints: interpreter.type_hints,
        guarded_type_hints: interpreter.guarded_type_hints,
        shape_erased_paths: interpreter.shape_erased_paths,
        string_contract_paths: interpreter.string_contract_paths,
        direct_range_source_paths: interpreter.direct_range_source_paths,
        destructured_range_source_paths: interpreter.destructured_range_source_paths,
        fail_conditions: interpreter.fail_conditions,
    }
}

/// Control-header facts parsed once from the shared Go-template tree, keyed
/// by the region's opening-bracket byte (which equals the action node's
/// start byte).
pub(crate) struct ControlFacts {
    pub(super) header: Option<TemplateHeader>,
    pub(super) range_destructured: bool,
    /// The VALUE variable of a destructured range header (`$v` in
    /// `range $k, $v := …`).
    pub(super) range_value_variable: Option<String>,
    /// The KEY variable of a destructured range header (`$k` in
    /// `range $k, $v := …`).
    pub(super) range_key_variable: Option<String>,
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
        }
    }
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
                    range_value_variable: None,
                    range_key_variable: None,
                    region_end: node.end_byte(),
                },
            );
        }
        "range_action" => {
            out.insert(
                node.start_byte(),
                ControlFacts {
                    header: range_header_from_source(node, source),
                    range_destructured: range_has_destructured_variable_definition(node),
                    range_value_variable: helm_schema_ast::range_destructured_value_variable(
                        node, source,
                    ),
                    range_key_variable: helm_schema_ast::range_destructured_key_variable(
                        node, source,
                    ),
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
        value_variable: Option<String>,
        key_variable: Option<String>,
    },
    Else,
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
    /// Static file templates currently being inlined (cycle prevention for
    /// `.Files.Get`-style template requests).
    pub(super) inline_files: Vec<String>,
    /// Whether this interpreter evaluates a helper body (a summary run).
    pub(super) helper_scope: bool,
    /// The active helper call chain, threaded into expression evaluation so
    /// nested bound calls cut cycles.
    pub(super) helper_seen: HashSet<String>,
    pub(super) locals: SymbolicLocalState,
    pub(super) dot_stack: Vec<Option<AbstractValue>>,
    /// The value-flavor dot of a helper scope's root frame (the call
    /// boundary resolves both flavors; see `DotFrame`). Document scope has
    /// none: its value dot derives from the fragment dot.
    pub(super) root_value_dot: Option<AbstractValue>,
    pub(super) root_bindings: HashMap<String, AbstractValue>,
    pub(super) active_predicates: Vec<Predicate>,
    pub(super) reads: Vec<ValueRead>,
    /// Dedup shadow of `reads` (order lives in the vec).
    reads_seen: HashSet<ValueRead>,
    pub(super) type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Paths consumed as serialized YAML by `fromYaml`; document-scope
    /// helper conditions import this narrow input contract without importing
    /// unrelated helper-body output transformations.
    pub(super) parsed_yaml_input_paths: BTreeSet<String>,
    /// Paths whose helper output was serialized with `toYaml`; callers use
    /// this to recognize a matching `fromYaml` as a structural round trip.
    pub(super) yaml_serialized_paths: BTreeSet<String>,
    /// Input-type hints observed while branch predicates were active: they
    /// hold only where those branches render, so they may type conditional
    /// overlays but never the unconditional base.
    pub(super) guarded_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Paths consumed only through total stringifications (`quote`,
    /// `toString`, `join`, `printf`): the chart tolerates any input type at
    /// them even when no placed row exists.
    pub(super) shape_erased_paths: BTreeSet<String>,
    /// Paths on which a string-consuming transform bound a real runtime
    /// string contract somewhere in the source.
    pub(super) string_contract_paths: BTreeSet<String>,
    /// Paths a `range` iterates DIRECTLY (`range .Values.x`): only these
    /// carry an iterable input domain.
    pub(super) direct_range_source_paths: BTreeSet<String>,
    /// The subset of direct range sources iterated with TWO variables
    /// (`range $k, $v := …`): integers iterate single-variable ranges only
    /// ("can't use 2 to iterate over more than one variable").
    pub(super) destructured_range_source_paths: BTreeSet<String>,
    /// `fail` captures (see [`FailCapture`]): no valid values document may
    /// satisfy one of these conjunctions.
    pub(super) fail_conditions: Vec<FailCapture>,
    /// Values paths of enclosing conditions whose lowering is APPROXIMATE
    /// (truthy fallbacks, dropped conjuncts), stacked with the region walk.
    /// Rows tolerate wider conditions; fail NEGATION abstains when the
    /// imprecision touches the tested path, so captures snapshot this.
    pub(super) approximate_condition_paths: Vec<String>,
    /// Directly ranged paths active on the walk. Helper-scope ranges mark
    /// membership with truthy predicates (the summary lane's flavor), so
    /// fail capture re-adds the range facts from here.
    pub(super) active_direct_ranged_paths: Vec<String>,
    /// Predicate paths severed by index-call narrowing anywhere in this
    /// source: guard reads of their strict ancestors are dropped from the
    /// summary (the narrowing proves the ancestor probe was a traversal
    /// step, not a condition on the ancestor itself).
    pub(super) suppress_predicate_paths: BTreeSet<String>,
    /// Every chart-level `set … default` normalization observed anywhere in
    /// this source, unconditionally. `locals.chart_value_defaults` keeps the
    /// branch-intersected "definitely ran in source order" view for render
    /// sites; helper summaries export this accumulator instead (a summary's
    /// defaults are declarations for the caller, like they always were).
    pub(super) chart_defaults_observed: BTreeSet<String>,
    /// The site facts of the hole or control region currently being
    /// evaluated; reads recorded during that evaluation carry them.
    pub(super) current_site: Option<Rc<SiteFacts>>,
}

impl<'a> Interpreter<'a> {
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
            inline_files: Vec::new(),
            helper_scope: false,
            helper_seen: HashSet::new(),
            locals: SymbolicLocalState::default(),
            dot_stack: Vec::new(),
            root_value_dot: None,
            root_bindings: HashMap::new(),
            active_predicates: Vec::new(),
            reads: Vec::new(),
            reads_seen: HashSet::new(),
            type_hints: BTreeMap::new(),
            parsed_yaml_input_paths: BTreeSet::new(),
            yaml_serialized_paths: BTreeSet::new(),
            guarded_type_hints: BTreeMap::new(),
            shape_erased_paths: BTreeSet::new(),
            string_contract_paths: BTreeSet::new(),
            direct_range_source_paths: BTreeSet::new(),
            destructured_range_source_paths: BTreeSet::new(),
            fail_conditions: Vec::new(),
            approximate_condition_paths: Vec::new(),
            active_direct_ranged_paths: Vec::new(),
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
        self.site_facts(
            resource_span.map(|resource| (resource.resource.clone(), resource.path_prefix.clone())),
            span,
        )
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
        self.site_facts(
            unique.map(|resource| (resource.resource.clone(), resource.path_prefix.clone())),
            span,
        )
    }

    fn site_facts(
        &self,
        resource: Option<(ResourceRef, Vec<String>)>,
        span: Span,
    ) -> Option<Rc<SiteFacts>> {
        let provenance = self.source_path.map(|source_path| {
            let helper_chain = self
                .inline_files
                .iter()
                .filter_map(|entry| entry.strip_prefix("define:"))
                .map(std::string::ToString::to_string)
                .collect();
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
        self.dot_stack.last().cloned().flatten()
    }

    pub(super) fn current_dot_binding(&self) -> Option<AbstractValue> {
        if self.dot_stack.len() <= 1
            && let Some(root) = &self.root_value_dot
        {
            return Some(root.clone());
        }
        self.dot_stack
            .last()
            .and_then(|binding| binding.as_ref())
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

    pub(super) fn value_path_context(&self) -> ValuePathContext<'_> {
        // Member bindings resolve for conditions and assignments; explicit
        // fragment values shadow them where both exist.
        let mut template_bindings = self.locals.range_member_values.clone();
        template_bindings.extend(
            self.locals
                .fragment_values
                .iter()
                .map(|(name, value)| (name.clone(), value.clone())),
        );
        ValuePathContext {
            root_bindings: &self.root_bindings,
            template_bindings,
            range_domains: &self.locals.range_domains,
            get_bindings: &self.locals.get_bindings,
            template_default_paths: &self.locals.default_paths,
            template_output_meta: &self.locals.output_meta,
            typeof_bindings: &self.locals.typeof_sources,
            fragment_context: FragmentEvalContext::new(self.db),
            current_dot_fragment: self.current_dot_fragment(),
            current_dot_binding: self.current_value_dot(),
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
            approximate_condition_paths: self.approximate_condition_paths.iter().cloned().collect(),
            direct_ranged_paths: self.active_direct_ranged_paths.iter().cloned().collect(),
            member_access: false,
        };
        if capture
            .conjunction
            .iter()
            .any(|p| matches!(p, Predicate::False))
        {
            return;
        }
        if !self.fail_conditions.contains(&capture) {
            self.fail_conditions.push(capture);
        }
    }

    /// Record a `required(message, subject)` guardrail: rendering fails
    /// under the ambient predicates whenever the subject is Helm-empty
    /// (absent, null, or the empty string).
    pub(super) fn record_required_condition(&mut self, subject_path: &str) {
        let empty = Predicate::Or(vec![
            Predicate::from(Guard::Absent {
                path: subject_path.to_string(),
            }),
            Predicate::from(Guard::Eq {
                path: subject_path.to_string(),
                value: helm_schema_core::GuardValue::Null,
            }),
            Predicate::from(Guard::Eq {
                path: subject_path.to_string(),
                value: helm_schema_core::GuardValue::string(""),
            }),
        ]);
        let capture = FailCapture {
            conjunction: self.fail_capture_conjunction(vec![empty]),
            approximate_condition_paths: self.approximate_condition_paths.iter().cloned().collect(),
            direct_ranged_paths: self.active_direct_ranged_paths.iter().cloned().collect(),
            member_access: false,
        };
        if capture
            .conjunction
            .iter()
            .any(|p| matches!(p, Predicate::False))
        {
            return;
        }
        if !self.fail_conditions.contains(&capture) {
            self.fail_conditions.push(capture);
        }
    }

    /// The ambient predicates plus `tail`, with the active direct range
    /// facts re-added (helper-scope ranges push truthy flavors only).
    fn fail_capture_conjunction(&self, tail: Vec<Predicate>) -> Vec<Predicate> {
        let mut conjunction = self.active_predicates.clone();
        for path in &self.active_direct_ranged_paths {
            let range = Predicate::from(Guard::Range { path: path.clone() });
            if !conjunction.contains(&range) {
                conjunction.push(range);
            }
        }
        conjunction.extend(tail);
        conjunction
    }

    pub(super) fn ambient_condition(&self) -> GuardDnf {
        GuardDnf::from_contract_predicate_conjunction(self.active_predicates.iter().cloned())
    }

    pub(super) fn push_predicate(&mut self, predicate: Predicate) {
        if !predicate.is_trivial() && !self.active_predicates.contains(&predicate) {
            self.active_predicates.push(predicate);
        }
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
            values_path,
            kind,
            condition,
            resource,
            provenance,
            dependency,
        );
    }

    fn push_read_row_with_condition(
        &mut self,
        values_path: &str,
        kind: crate::ValueKind,
        condition: GuardDnf,
        resource: Option<ResourceRef>,
        provenance: Vec<ContractProvenance>,
        dependency: bool,
    ) {
        if values_path.trim().is_empty() {
            return;
        }
        let read = ValueRead {
            values_path: values_path.to_string(),
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

    /// Absorb one nested interpreter's read verbatim (nested static-file
    /// evaluations already stamped their own guards and sites).
    pub(super) fn push_nested_read(&mut self, read: ValueRead) {
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
        for part in &string.parts {
            match part {
                StringPart::Text(_) => {}
                StringPart::Splice(splice) => {
                    let mut extra = Vec::new();
                    if splice.meta.defaulted {
                        extra.push(Guard::Default {
                            path: splice.values_path.clone(),
                        });
                    }
                    self.push_read(&splice.values_path, &extra);
                }
                StringPart::Taint(taint) => {
                    for path in &taint.paths {
                        self.push_read(path, &[]);
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
            GuardDnf::from_contract_predicate_disjunction_preserving_evidence(
                meta.predicates.iter().map(|branch| branch.iter().cloned()),
            )
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
                path: values_path.to_string(),
            }]);
        }
        self.push_read_row_with_condition(
            values_path,
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
        GuardDnf::from_contract_predicate_conjunction(
            self.active_predicates
                .iter()
                .filter(|predicate| {
                    let path = match predicate {
                        Predicate::Guard(Guard::Truthy { path }) => path,
                        Predicate::Not(inner) => match inner.as_ref() {
                            Predicate::Guard(Guard::Truthy { path }) => path,
                            _ => return true,
                        },
                        _ => return true,
                    };
                    path == claim_path
                        || !sibling_claims.contains(path)
                        || crate::helper_meta::values_paths_are_related(path, claim_path)
                })
                .cloned(),
        )
    }

    /// Absorb helper-body reads at a call site: each read keeps its
    /// helper-internal guards and gains the site's ambient guards; the
    /// site's provenance leads the read's helper-body sites. Helper-internal
    /// reads carry no resource of their own, so site-less rows stay
    /// resource-free exactly like the summary lane always was.
    /// Absorb called-helper fail conjunctions: the body recorded its
    /// internal predicates; the call site prepends its ambient predicates,
    /// the same scoping helper reads get.
    pub(super) fn absorb_helper_fails(&mut self, fails: &[FailCapture]) {
        for body_capture in fails {
            let mut approximate: BTreeSet<String> =
                self.approximate_condition_paths.iter().cloned().collect();
            approximate.extend(body_capture.approximate_condition_paths.iter().cloned());
            let mut direct_ranged: BTreeSet<String> =
                self.active_direct_ranged_paths.iter().cloned().collect();
            direct_ranged.extend(body_capture.direct_ranged_paths.iter().cloned());
            let capture = FailCapture {
                conjunction: self.fail_capture_conjunction(body_capture.conjunction.clone()),
                approximate_condition_paths: approximate,
                direct_ranged_paths: direct_ranged,
                member_access: body_capture.member_access,
            };
            if capture
                .conjunction
                .iter()
                .any(|p| matches!(p, Predicate::False))
            {
                continue;
            }
            if !self.fail_conditions.contains(&capture) {
                self.fail_conditions.push(capture);
            }
        }
    }

    pub(super) fn absorb_helper_reads_with_suppression(
        &mut self,
        reads: &[ValueRead],
        suppressed: &BTreeSet<&String>,
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
                && !suppressed.contains(&read.values_path)
                && suppressed.iter().any(|narrowed| {
                    helm_schema_core::values_path_is_descendant(narrowed, &read.values_path)
                })
            {
                continue;
            }
            let mut provenance = site_provenance.clone();
            merge_provenance_sites(&mut provenance, &read.provenance);
            let condition = self
                .claim_scoped_ambient_condition(&read.values_path, sibling_claims)
                .conjoined(&read.condition);
            self.push_read_row_with_condition(
                &read.values_path,
                read.kind,
                condition,
                read.resource.clone(),
                provenance,
                read.dependency,
            );
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
                    let region_index = index;
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
                    // Descendants of *earlier* siblings that escaped forward
                    // into the region (a branch contributing to a container
                    // opened before it) belong to branch bodies too; their
                    // in-place evaluation was bounded at the region start.
                    let mut escaped = Vec::new();
                    for prior in &nodes[..region_index] {
                        if matches!(prior.node, Node::Control(_)) {
                            continue;
                        }
                        let mut chain = Vec::new();
                        super::control::collect_deferred(
                            prior.node,
                            region.span.start,
                            prior.child_limit,
                            &mut chain,
                            &mut escaped,
                        );
                    }
                    out.extend(self.eval_control(region, &adopted, escaped));
                }
                Node::Output(action) => {
                    let consumed = self.eval_output_with_lookahead(action, nodes, index, &mut out);
                    index += consumed;
                }
                _ => {
                    // Evaluation stops at the next control sibling's start:
                    // descendants escaping into that region evaluate inside
                    // its branches instead of unguarded in place.
                    let mut bounded = *view;
                    if let Some(region_start) =
                        nodes[index + 1..].iter().find_map(|next| match next.node {
                            Node::Control(region) => Some(region.span.start),
                            _ => None,
                        })
                    {
                        bounded.child_limit = Some(
                            bounded
                                .child_limit
                                .map_or(region_start, |limit| limit.min(region_start)),
                        );
                    }
                    let contributions = self.eval_node(bounded);
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
            let (value, width) = self.eval_output_action(action.span);
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
                    self.eval_entire_hole(value_action.span)
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

    /// The structural indent of one node: containers report their own
    /// indent, scalars their line indent, plain outputs their line indent,
    /// and control regions the minimum over their branch bodies (a region's
    /// rendered content sits at the body indent; the header line is
    /// conventionally unindented). Outputs with an explicit rendered indent
    /// (`… | nindent N`) report `None`: they float, and the float rules own
    /// their placement (line columns are layout noise for them).
    pub(super) fn structural_content_indent(&self, node: &Node) -> Option<usize> {
        match node {
            Node::Mapping(entry) => Some(entry.indent),
            Node::Sequence(item) => Some(item.indent),
            Node::Scalar(line) => Some(line.indent),
            Node::Control(region) => region
                .branches
                .iter()
                .flat_map(|branch| &branch.body)
                .filter_map(|child| self.structural_content_indent(child))
                .min(),
            Node::Output(action) => {
                let width = parse_expr_text(self.text(action.span))
                    .iter()
                    .rev()
                    .find_map(TemplateExpr::fragment_indent_width);
                match width {
                    Some(_) => None,
                    None => Some(self.line_indent(action.span.start)),
                }
            }
            Node::Comment(_) | Node::Opaque(_) => None,
        }
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

    fn eval_node(&mut self, view: NodeView<'_>) -> Contributions {
        let mut out = Contributions::default();
        match view.node {
            Node::Mapping(entry) => {
                let previous_site = self.enter_hole_site(entry.key.span);
                let key = self.entry_key(&entry.key);
                self.push_key_reads(&key);
                self.restore_site(previous_site);
                let mut value = Guarded::empty();
                if let Some(block) = &entry.block {
                    value.extend(self.eval_block_scalar(block));
                }
                if let Some(parts) = &entry.value {
                    value.extend(self.eval_scalar_parts(parts));
                }
                let (children, siblings) = self.split_structural_children(view, entry.indent);
                if !children.is_empty() {
                    let mut child = self.eval_node_list(&children);
                    let opened_empty = entry.value.is_none() && entry.block.is_none();
                    let marked_at = content_child_mark(&entry.children, entry.indent);
                    value.extend(child.take_floating_below(entry.indent, opened_empty, marked_at));
                    out.floating.append(&mut child.floating);
                    value.extend(child.assemble());
                }
                out.merge_entry(key, value);
                if !siblings.is_empty() {
                    out.extend(self.eval_node_list(&siblings));
                }
            }
            Node::Sequence(item) => {
                let mut value = Guarded::empty();
                if let Some(block) = &item.block {
                    value.extend(self.eval_block_scalar(block));
                }
                if let Some(parts) = &item.value {
                    value.extend(self.eval_scalar_parts(parts));
                }
                let (children, siblings) = self.split_structural_children(view, item.indent);
                if !children.is_empty() {
                    let mut child = self.eval_node_list(&children);
                    // Items never accept same-indent output (the open-slot
                    // query pushes item frames without that allowance).
                    value.extend(child.take_floating_below(item.indent, false, None));
                    out.floating.append(&mut child.floating);
                    value.extend(child.assemble());
                }
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
            Node::Control(_) | Node::Output(_) | Node::Comment(_) | Node::Opaque(_) => {}
        }
        out
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
        container_indent: usize,
    ) -> (Vec<NodeView<'n>>, Vec<NodeView<'n>>) {
        view.in_scope_children()
            .into_iter()
            .partition(|child| self.node_belongs_inside(child.node, container_indent))
    }

    fn node_belongs_inside(&self, node: &Node, container_indent: usize) -> bool {
        match node {
            Node::Mapping(entry) => entry.indent > container_indent,
            Node::Sequence(item) => item.indent >= container_indent,
            Node::Scalar(line) => line.indent > container_indent,
            Node::Control(region) => region
                .branches
                .iter()
                .flat_map(|branch| &branch.body)
                .all(|child| self.node_belongs_inside(child, container_indent)),
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
