//! In-domain helper summaries: a bound helper call evaluates its body's CST
//! through the same fragment interpreter as documents, producing one
//! [`FragmentSummary`] — the body's abstract fragment plus its pathless
//! reads — memoized per bound call in the analysis db.
//!
//! The summary is call-site independent: reads carry only helper-internal
//! guards and helper-body provenance, and fragment sites keep the body's
//! own facts. Call sites add their ambient guards, site provenance, and
//! resource scope when absorbing reads, and [`splice_summary`] rebases the
//! fragment's sites onto the call site.
//!
//! One value projection ([`FragmentSummary::value`]) derives the
//! `AbstractValue` for helper calls in value position (inside expressions):
//!
//! - literal text arms project as string sets,
//! - splices project as `OutputPath` rows whose meta carries the arm's
//!   root-to-leaf conditions, defaultedness, and body provenance,
//! - opaque taint projects as per-path `OutputPath` rows (the influence is
//!   attributable even though the text is not),
//! - mappings/sequences project as dicts/lists; dynamic-key entry values
//!   merge at the parent level (no invented segment),
//! - render-suppressed scalars project no value (their splices surface as
//!   dependency reads instead),
//! - arms merge with the value lattice's merge rules.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::Rc;

use helm_schema_syntax::TemplatedDocument;

use crate::abstract_value::AbstractValue;
use crate::analysis_db::{BoundHelperCallResolution, IrAnalysisDb};
use crate::helper_meta::{HelperOutputMeta, RenderedRow, merge_provenance_sites};
use crate::symbolic_local_state::SymbolicLocalState;
use crate::{ContractProvenance, ValueKind};
use helm_schema_core::{GuardDnf, Predicate};

use super::domain::{AbstractFragment, Guarded, PathCondition, SiteFacts, Splice, StringPart};
use super::eval::{Interpreter, NodeView, ValueRead};

/// One bound helper call's evaluation in the fragment domain.
#[derive(Debug, Default)]
pub(crate) struct FragmentSummary {
    /// Predicate paths severed by index-call narrowing inside the body;
    /// callers absorb their own reads against them.
    pub(crate) suppress_predicate_paths: BTreeSet<String>,
    /// The body's abstract fragment (sites carry helper-body facts).
    pub(crate) root: Guarded<AbstractFragment>,
    /// Pathless reads observed in the body (helper-internal guards only).
    pub(crate) reads: Vec<ValueRead>,
    /// Declared input-type hints observed unconditionally in the body.
    pub(crate) type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Input-type hints observed only under body branch predicates.
    pub(crate) guarded_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Input-type hints from literal `default`/`coalesce` fallbacks in the
    /// body: they type only the truthy arm of the path.
    pub(crate) fallback_type_hints: BTreeMap<String, BTreeSet<String>>,
    /// Paths consumed as serialized YAML by `fromYaml` in the body.
    pub(crate) parsed_yaml_input_paths: BTreeSet<String>,
    /// Paths serialized with `toYaml` in the helper's projected output.
    pub(crate) yaml_serialized_paths: BTreeSet<String>,
    /// Paths consumed through total stringifications anywhere in the body:
    /// the chart tolerates any input type at them.
    pub(crate) shape_erased_paths: BTreeSet<String>,
    /// Paths carrying a real runtime string contract in the body.
    pub(crate) string_contract_paths: BTreeSet<String>,
    /// The body's per-path range facts after bound-context resolution
    /// (direct iteration identity, JSON-decoded values, destructuring).
    /// Callers need the direct identity to project the runtime domain.
    pub(crate) range_modes: crate::range_modes::RangeModes,
    /// `fail` captures of the body, helper-internal state only.
    pub(crate) fail_conditions: Vec<crate::eval_effect::FailCapture>,
    /// Object-producing value mutations observed in source order.
    pub(crate) member_host_conversions: BTreeSet<crate::eval_effect::MemberHostConversion>,
    /// Chart-level `set … default` normalizations the body applies.
    pub(crate) chart_defaults: BTreeSet<String>,
    /// Root-context fields replaced while the helper executes.
    pub(crate) root_set_mutations: BTreeMap<String, AbstractValue>,
    /// Truth predicates for root-context fields replaced by the helper.
    pub(crate) root_set_predicates: BTreeMap<String, Predicate>,
    /// Chart value subtrees supplying defaults to a replaced effective values tree.
    pub(crate) values_default_sources: BTreeSet<crate::ValuesDefaultSource>,
    /// The value projection (see module docs), computed once.
    pub(crate) value: Option<AbstractValue>,
    /// Rendered splice/taint rows flattened from the tree: per-path branch
    /// conditions, defaultedness, encoding, and provenance. Value-position
    /// call sites use these for no-render demotion and for restoring
    /// per-path meta after transfer functions collapse the value shape.
    pub(crate) rendered: Vec<RenderedRow>,
}

/// Evaluate one bound helper body as a fragment. `seen` is the active call
/// chain (this helper included), threaded so nested calls cut cycles.
pub(crate) fn eval_bound_helper_fragment(
    name: &str,
    resolution: &BoundHelperCallResolution,
    db: &IrAnalysisDb,
    seen: &HashSet<String>,
) -> FragmentSummary {
    let Some(body) = db.parsed_helper_body(name) else {
        return FragmentSummary::default();
    };
    let document = TemplatedDocument::parse_with_root(body.source, body.tree.root_node());
    let body_facts = db.helper_body_eval_facts(name, || {
        super::eval::BodyEvalFacts::collect(body.source, db, &body.tree, &document)
    });
    let mut interpreter = Interpreter::with_body_facts(
        body.source,
        Some(body.source_path),
        db,
        &document,
        body_facts,
    );
    interpreter.source_offset = body.body_offset;
    interpreter.inline_files = vec![format!("define:{name}")];
    interpreter.helper_scope = true;
    interpreter.helper_seen = seen.clone();
    interpreter.root_bindings = resolution.bindings.clone();
    interpreter.root_value_dot = resolution.dot.helper.clone();
    interpreter.dot_stack.push(resolution.dot.fragment.clone());
    interpreter.locals = SymbolicLocalState::default();
    let roots: Vec<NodeView<'_>> = document.roots().iter().map(NodeView::plain).collect();
    let contributions = interpreter.eval_node_list(&roots);
    let root = contributions.assemble();
    // Guard reads that are strict ancestors of an index-narrowed path are
    // traversal steps, not conditions on the ancestor (the narrowing severed
    // them); dependency rows and the narrowed paths themselves stay.
    let suppress = interpreter.suppress_predicate_paths;
    let mut reads: Vec<ValueRead> = interpreter
        .reads
        .into_iter()
        .filter(|read| {
            read.dependency
                || suppress.contains(&read.values_path)
                || !suppress.iter().any(|narrowed| {
                    helm_schema_core::values_path_is_descendant(narrowed, &read.values_path)
                })
        })
        .collect();
    let rendered = rendered_rows(&root);
    prune_sibling_conditions(&mut reads, &rendered);
    let mut summary = FragmentSummary {
        value: projected_value(&root),
        rendered,
        root,
        reads,
        suppress_predicate_paths: suppress,
        type_hints: interpreter.type_hints,
        guarded_type_hints: interpreter.guarded_type_hints,
        fallback_type_hints: interpreter.fallback_type_hints,
        parsed_yaml_input_paths: interpreter.parsed_yaml_input_paths,
        yaml_serialized_paths: interpreter.yaml_serialized_paths,
        shape_erased_paths: interpreter.shape_erased_paths,
        string_contract_paths: interpreter.string_contract_paths,
        range_modes: interpreter.range_modes,
        fail_conditions: interpreter.fail_conditions,
        member_host_conversions: interpreter.member_host_conversions,
        chart_defaults: interpreter.chart_defaults_observed,
        root_set_mutations: interpreter.root_set_mutations_observed,
        root_set_predicates: interpreter.root_set_predicates_observed,
        values_default_sources: interpreter.values_default_sources_observed,
    };
    // Render-suppressed splices (block-scalar bodies) influence the text
    // without rendering a sink-typed value; value-position consumers see
    // them as dependency reads, matching the demotion the summary walk
    // applied at suppressing slots.
    append_suppressed_reads(&summary.root, &mut Vec::new(), &mut summary.reads);
    summary
}

impl FragmentSummary {
    /// The rendered `.Values` paths of the summary that are encoded at their
    /// render site (the sink does not constrain the value's shape).
    pub(crate) fn encoded_paths(&self) -> BTreeSet<String> {
        self.rendered
            .iter()
            .filter(|row| row.encoded)
            .map(|row| row.path.clone())
            .collect()
    }
}

/// Splice a memoized summary fragment at a call site: nodes stamped with a
/// helper-body site keep it when the body declared its own resource
/// (resource-defining helper bodies scope their own rows); otherwise the
/// body site becomes helper provenance on the node and the call site's
/// facts take the site slot.
pub(crate) fn splice_summary(
    summary: &FragmentSummary,
    call_site: &Option<Rc<SiteFacts>>,
) -> Guarded<AbstractFragment> {
    let mut root = summary.root.clone();
    for (_, node) in &mut root.arms {
        rebase_node_sites(node, call_site);
    }
    root
}

fn rebase_site(
    site: &mut Option<Rc<SiteFacts>>,
    provenance: &mut Vec<ContractProvenance>,
    call_site: &Option<Rc<SiteFacts>>,
) {
    match site.as_deref() {
        Some(body_site) if body_site.resource.is_none() => {
            let mut merged: Vec<ContractProvenance> =
                body_site.provenance.iter().cloned().collect();
            merge_provenance_sites(&mut merged, provenance);
            *provenance = merged;
            *site = call_site.clone();
        }
        Some(_) => {}
        None => *site = call_site.clone(),
    }
}

fn rebase_node_sites(node: &mut AbstractFragment, call_site: &Option<Rc<SiteFacts>>) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mut mapping.entries {
                for (_, value) in &mut entry.value.arms {
                    rebase_node_sites(value, call_site);
                }
            }
        }
        AbstractFragment::Sequence(sequence) => {
            for item in &mut sequence.items {
                for (_, value) in &mut item.arms {
                    rebase_node_sites(value, call_site);
                }
            }
        }
        AbstractFragment::Scalar(scalar) => {
            for part in &mut scalar.parts {
                match part {
                    StringPart::Text(_) => {}
                    StringPart::Splice(splice) => {
                        rebase_site(
                            &mut splice.meta.site,
                            &mut splice.meta.provenance,
                            call_site,
                        );
                    }
                    StringPart::Taint(taint) => {
                        rebase_site(&mut taint.site, &mut taint.provenance, call_site);
                    }
                }
            }
        }
        AbstractFragment::Splice(splice) => {
            rebase_site(
                &mut splice.meta.site,
                &mut splice.meta.provenance,
                call_site,
            );
        }
        AbstractFragment::Opaque(opaque) => {
            rebase_site(&mut opaque.site, &mut opaque.provenance, call_site);
        }
    }
}

/// The root-to-leaf condition chain as one predicate branch (flattening
/// `And` compounds into their parts, the shape branch meta always used).
fn condition_branch(conditions: &[PathCondition]) -> BTreeSet<Predicate> {
    let mut branch = BTreeSet::new();
    for condition in conditions {
        match condition {
            Predicate::True => {}
            Predicate::And(parts) => branch.extend(parts.iter().cloned()),
            other => {
                branch.insert(other.clone());
            }
        }
    }
    branch
}

fn splice_row_meta(splice: &Splice, conditions: &[PathCondition]) -> HelperOutputMeta {
    let mut provenance: Vec<ContractProvenance> = splice
        .meta
        .site
        .as_deref()
        .and_then(|site| site.provenance.clone())
        .into_iter()
        .collect();
    merge_provenance_sites(&mut provenance, &splice.meta.provenance);
    let mut meta = HelperOutputMeta {
        defaulted: splice.meta.defaulted,
        shape_erased: splice.meta.shape_erased,
        string_contract: splice.meta.string_contract,
        json_serialized: splice.meta.json_serialized,
        json_decoded: splice.meta.json_decoded,
        lexical_escapes: splice.meta.lexical_escapes.clone(),
        provenance,
        ..HelperOutputMeta::default()
    };
    let branch = condition_branch(conditions);
    if !branch.is_empty() {
        meta.predicates.insert(branch);
    }
    meta
}

fn taint_row_meta(
    site: Option<&SiteFacts>,
    provenance: &[ContractProvenance],
    conditions: &[PathCondition],
) -> HelperOutputMeta {
    let mut merged: Vec<ContractProvenance> = site
        .and_then(|site| site.provenance.clone())
        .into_iter()
        .collect();
    merge_provenance_sites(&mut merged, provenance);
    let mut meta = HelperOutputMeta {
        provenance: merged,
        ..HelperOutputMeta::default()
    };
    let branch = condition_branch(conditions);
    if !branch.is_empty() {
        meta.predicates.insert(branch);
    }
    meta
}

fn scalar_taint_row_meta(
    taint: &super::domain::TaintPart,
    conditions: &[PathCondition],
) -> HelperOutputMeta {
    let mut meta = taint_row_meta(taint.site.as_deref(), &taint.provenance, conditions);
    meta.json_serialized = taint.json_serialized;
    meta
}

fn project_structured_taint_value(
    value: &AbstractValue,
    outer_meta: &HelperOutputMeta,
) -> AbstractValue {
    match value {
        AbstractValue::ValuesPath(path) => {
            AbstractValue::OutputPath(path.clone(), outer_meta.clone())
        }
        AbstractValue::JsonDecodedPath(path) => {
            let mut meta = outer_meta.clone();
            meta.json_decoded = true;
            AbstractValue::OutputPath(path.clone(), meta)
        }
        AbstractValue::OutputPath(path, inner_meta) => {
            AbstractValue::OutputPath(path.clone(), conjoin_output_meta(inner_meta, outer_meta))
        }
        AbstractValue::Dict(entries) => AbstractValue::Dict(
            entries
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        project_structured_taint_value(value, outer_meta),
                    )
                })
                .collect(),
        ),
        AbstractValue::List(items) => AbstractValue::List(
            items
                .iter()
                .map(|item| project_structured_taint_value(item, outer_meta))
                .collect(),
        ),
        AbstractValue::Overlay { entries, fallback } => AbstractValue::Overlay {
            entries: entries
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        project_structured_taint_value(value, outer_meta),
                    )
                })
                .collect(),
            fallback: Box::new(project_structured_taint_value(fallback, outer_meta)),
        },
        AbstractValue::Choice(choices) => AbstractValue::Choice(
            choices
                .iter()
                .map(|choice| project_structured_taint_value(choice, outer_meta))
                .collect(),
        ),
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RangeKey(_)
        | AbstractValue::RootContext
        | AbstractValue::StringSet(_)
        | AbstractValue::DerivedBoolean(_)
        | AbstractValue::SplitList { .. }
        | AbstractValue::SplitSegment { .. }
        | AbstractValue::Widened(_) => value.clone(),
    }
}

fn conjoin_output_meta(inner: &HelperOutputMeta, outer: &HelperOutputMeta) -> HelperOutputMeta {
    let inner_branches = if inner.predicates.is_empty() {
        vec![BTreeSet::new()]
    } else {
        inner.predicates.iter().cloned().collect()
    };
    let outer_branches = if outer.predicates.is_empty() {
        vec![BTreeSet::new()]
    } else {
        outer.predicates.iter().cloned().collect()
    };
    let predicates = inner_branches
        .into_iter()
        .flat_map(|inner| {
            outer_branches.iter().map(move |outer| {
                let mut branch = inner.clone();
                branch.extend(outer.iter().cloned());
                branch
            })
        })
        .filter(|branch| !branch.is_empty())
        .collect();
    let mut combined = inner.clone();
    combined.predicates.clear();
    let mut outer = outer.clone();
    outer.predicates.clear();
    combined.merge(&outer);
    combined.predicates = predicates;
    combined
}

/// The documented value projection (module docs). Arms of one guarded value
/// project independently and merge with the lattice's merge rules; splice
/// conditions survive in `OutputPath` meta.
pub(super) fn projected_value(root: &Guarded<AbstractFragment>) -> Option<AbstractValue> {
    let mut conditions = Vec::new();
    let values = project_guarded(root, &mut conditions);
    AbstractValue::merge_all(values)
}

fn project_guarded(
    guarded: &Guarded<AbstractFragment>,
    conditions: &mut Vec<PathCondition>,
) -> Vec<AbstractValue> {
    let mut values = Vec::new();
    for (condition, node) in &guarded.arms {
        let pushed = !condition.is_trivial();
        if pushed {
            conditions.push(condition.clone());
        }
        values.extend(project_node(node, conditions));
        if pushed {
            conditions.pop();
        }
    }
    values
}

fn project_node(
    node: &AbstractFragment,
    conditions: &mut Vec<PathCondition>,
) -> Vec<AbstractValue> {
    match node {
        AbstractFragment::Mapping(mapping) => {
            let mut entries = BTreeMap::new();
            let mut extra = Vec::new();
            for entry in &mapping.entries {
                let child_values = project_guarded(&entry.value, conditions);
                match &entry.key {
                    super::domain::EntryKey::Literal(key) if !key.is_empty() => {
                        if let Some(value) = AbstractValue::merge_all(child_values) {
                            entries.insert(key.clone(), value);
                        }
                    }
                    // Dynamic (and empty) keys attribute at the parent level;
                    // the projection invents no segment for them.
                    _ => extra.extend(child_values),
                }
            }
            let mut values = Vec::new();
            if !entries.is_empty() {
                values.push(AbstractValue::Dict(entries));
            }
            values.extend(extra);
            values
        }
        AbstractFragment::Sequence(sequence) => {
            let items: Vec<AbstractValue> = sequence
                .items
                .iter()
                .filter_map(|item| {
                    let values = project_guarded(item, conditions);
                    AbstractValue::choice(values)
                })
                .collect();
            if items.is_empty() {
                Vec::new()
            } else {
                vec![AbstractValue::List(items)]
            }
        }
        AbstractFragment::Scalar(scalar) => {
            if scalar.suppressed {
                return Vec::new();
            }
            let mut values = Vec::new();
            let mut strings = BTreeSet::new();
            let mut has_non_text = false;
            for part in &scalar.parts {
                match part {
                    StringPart::Text(alternatives) => strings.extend(alternatives.iter().cloned()),
                    StringPart::Splice(splice) => {
                        has_non_text = true;
                        values.push(AbstractValue::OutputPath(
                            splice.values_path.clone(),
                            splice_row_meta(splice, conditions),
                        ));
                    }
                    StringPart::Taint(taint) => {
                        has_non_text = true;
                        let meta = scalar_taint_row_meta(taint, conditions);
                        if let Some(value) = &taint.structured_value {
                            values.push(project_structured_taint_value(value, &meta));
                        } else {
                            for path in &taint.paths {
                                values.push(AbstractValue::OutputPath(path.clone(), meta.clone()));
                            }
                        }
                    }
                }
            }
            // Literal text forms the value only when the scalar is pure
            // text; text around splices renders but is not the value the
            // call site consumes (the paths are).
            if !has_non_text && !strings.is_empty() {
                values.push(AbstractValue::StringSet(strings));
            }
            values
        }
        AbstractFragment::Splice(splice) => {
            vec![AbstractValue::OutputPath(
                splice.values_path.clone(),
                splice_row_meta(splice, conditions),
            )]
        }
        AbstractFragment::Opaque(opaque) => {
            let meta = taint_row_meta(opaque.site.as_deref(), &opaque.provenance, conditions);
            opaque
                .taint
                .iter()
                .map(|path| AbstractValue::OutputPath(path.clone(), meta.clone()))
                .collect()
        }
    }
}

/// Flatten the tree's rendered splice/taint rows into per-path claims.
fn rendered_rows(root: &Guarded<AbstractFragment>) -> Vec<RenderedRow> {
    let mut rows = Vec::new();
    let mut conditions = Vec::new();
    collect_rendered(root, &mut conditions, false, &mut rows);
    rows
}

fn collect_rendered(
    guarded: &Guarded<AbstractFragment>,
    conditions: &mut Vec<PathCondition>,
    suppressed: bool,
    rows: &mut Vec<RenderedRow>,
) {
    for (condition, node) in &guarded.arms {
        let pushed = !condition.is_trivial();
        if pushed {
            conditions.push(condition.clone());
        }
        collect_rendered_node(node, conditions, suppressed, rows);
        if pushed {
            conditions.pop();
        }
    }
}

fn push_rendered_row(
    rows: &mut Vec<RenderedRow>,
    path: &str,
    kind: ValueKind,
    encoded: bool,
    meta: HelperOutputMeta,
) {
    if path.trim().is_empty() {
        return;
    }
    if let Some(existing) = rows
        .iter_mut()
        .find(|row| row.path == path && row.kind == kind && row.encoded == encoded)
    {
        existing.meta.merge(&meta);
        return;
    }
    rows.push(RenderedRow {
        path: path.to_string(),
        kind,
        encoded,
        meta,
    });
}

fn collect_rendered_node(
    node: &AbstractFragment,
    conditions: &mut Vec<PathCondition>,
    suppressed: bool,
    rows: &mut Vec<RenderedRow>,
) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mapping.entries {
                collect_rendered(&entry.value, conditions, suppressed, rows);
            }
        }
        AbstractFragment::Sequence(sequence) => {
            for item in &sequence.items {
                collect_rendered(item, conditions, suppressed, rows);
            }
        }
        AbstractFragment::Scalar(scalar) => {
            let suppressed = suppressed || scalar.suppressed;
            if suppressed {
                return;
            }
            for part in &scalar.parts {
                match part {
                    StringPart::Text(_) => {}
                    StringPart::Splice(splice) => push_rendered_row(
                        rows,
                        &splice.values_path,
                        splice.kind,
                        splice.meta.encoded,
                        splice_row_meta(splice, conditions),
                    ),
                    StringPart::Taint(taint) => {
                        let meta = scalar_taint_row_meta(taint, conditions);
                        for path in &taint.paths {
                            push_rendered_row(
                                rows,
                                path,
                                ValueKind::PartialScalar,
                                false,
                                meta.clone(),
                            );
                        }
                    }
                }
            }
        }
        AbstractFragment::Splice(splice) => {
            if !suppressed {
                push_rendered_row(
                    rows,
                    &splice.values_path,
                    splice.kind,
                    splice.meta.encoded,
                    splice_row_meta(splice, conditions),
                );
            }
        }
        AbstractFragment::Opaque(opaque) => {
            if suppressed {
                return;
            }
            let meta = taint_row_meta(opaque.site.as_deref(), &opaque.provenance, conditions);
            for path in &opaque.taint {
                push_rendered_row(rows, path, opaque.kind, false, meta.clone());
            }
        }
    }
}

/// The summary lane's sibling-condition rule for pathless reads: a
/// truthiness condition about a *different* summary source describes that
/// sibling's branch, not this read's, and drops unless the paths are
/// related. A read defaulted on its own path additionally drops its own
/// negation when a positive condition remains (the default supplies the
/// value on the negative side).
fn prune_sibling_conditions(reads: &mut Vec<ValueRead>, rendered: &[RenderedRow]) {
    let mut sources: BTreeSet<String> = reads.iter().map(|read| read.values_path.clone()).collect();
    sources.extend(rendered.iter().map(|row| row.path.clone()));
    if sources.len() < 2 {
        return;
    }
    let unrelated_sibling = |predicate_path: &str, read_path: &str| {
        predicate_path != read_path
            && sources.contains(predicate_path)
            && !crate::helper_meta::values_paths_are_related(predicate_path, read_path)
    };
    let mut pruned: Vec<ValueRead> = Vec::new();
    for mut read in reads.drain(..) {
        let conjunctions = read.condition.disjuncts().iter().map(|conjunction| {
            let has_truthy_sibling = conjunction.iter().any(|predicate| {
                matches!(predicate, Predicate::Guard(crate::Guard::Truthy { path }) if unrelated_sibling(path, &read.values_path))
            });
            let defaulted = conjunction.iter().any(|predicate| {
                matches!(predicate, Predicate::Guard(crate::Guard::Default { path }) if path == &read.values_path)
            });
            let has_self_truthy = conjunction.iter().any(|predicate| {
                matches!(predicate, Predicate::Guard(crate::Guard::Truthy { path }) if path == &read.values_path)
            });
            let mut predicates = conjunction
                .iter()
                .filter(|predicate| {
                    !matches!(predicate, Predicate::Guard(crate::Guard::Truthy { path }) if unrelated_sibling(path, &read.values_path))
                })
                .cloned()
                .collect::<Vec<_>>();
            if defaulted && (has_self_truthy || has_truthy_sibling) {
                predicates.retain(|predicate| {
                    !matches!(predicate, Predicate::Not(inner) if matches!(inner.as_ref(), Predicate::Guard(crate::Guard::Truthy { path }) if path == &read.values_path))
                });
            }
            predicates
        });
        read.condition = GuardDnf::from_disjunction(conjunctions);
        if !pruned.contains(&read) {
            pruned.push(read);
        }
    }
    *reads = pruned;
}

/// Dependency reads for splices inside render-suppressed scalars.
fn append_suppressed_reads(
    guarded: &Guarded<AbstractFragment>,
    conditions: &mut Vec<PathCondition>,
    reads: &mut Vec<ValueRead>,
) {
    for (condition, node) in &guarded.arms {
        let pushed = !condition.is_trivial();
        if pushed {
            conditions.push(condition.clone());
        }
        append_suppressed_node_reads(node, conditions, reads);
        if pushed {
            conditions.pop();
        }
    }
}

fn append_suppressed_node_reads(
    node: &AbstractFragment,
    conditions: &mut Vec<PathCondition>,
    reads: &mut Vec<ValueRead>,
) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mapping.entries {
                append_suppressed_reads(&entry.value, conditions, reads);
            }
        }
        AbstractFragment::Sequence(sequence) => {
            for item in &sequence.items {
                append_suppressed_reads(item, conditions, reads);
            }
        }
        AbstractFragment::Scalar(scalar) if scalar.suppressed => {
            for part in &scalar.parts {
                let (paths, site, provenance): (
                    Vec<&String>,
                    Option<&SiteFacts>,
                    &[ContractProvenance],
                ) = match part {
                    StringPart::Text(_) => continue,
                    StringPart::Splice(splice) => (
                        vec![&splice.values_path],
                        splice.meta.site.as_deref(),
                        &splice.meta.provenance,
                    ),
                    StringPart::Taint(taint) => (
                        taint.paths.iter().collect(),
                        taint.site.as_deref(),
                        &taint.provenance,
                    ),
                };
                let meta = taint_row_meta(site, provenance, conditions);
                for path in paths {
                    if path.trim().is_empty() {
                        continue;
                    }
                    let read = ValueRead {
                        values_path: path.clone(),
                        kind: ValueKind::Scalar,
                        condition: GuardDnf::from_conjunction(conditions.iter().cloned()),
                        resource: None,
                        provenance: meta.provenance.clone(),
                        dependency: true,
                    };
                    if !reads.contains(&read) {
                        reads.push(read);
                    }
                }
            }
        }
        AbstractFragment::Scalar(_) | AbstractFragment::Splice(_) | AbstractFragment::Opaque(_) => {
        }
    }
}
