//! Pure projections over fragment trees and their rendered provenance.
//!
//! These consumers flatten existing tree facts or rebase their sites.
//! They do not execute template syntax or retain helper-call state.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::abstract_value::AbstractValue;
use crate::helper_meta::{HelperOutputMeta, RenderedRow, merge_provenance_sites};
use crate::{ContractProvenance, ValueKind};
use helm_schema_core::{GuardDnf, Predicate};

use super::ValueRead;
use super::domain::{AbstractFragment, Guarded, PathCondition, SiteFacts, Splice, StringPart};

/// Rebases a fragment's sites while preserving resources declared inside its body.
pub(crate) fn rebase_fragment(
    fragment: &Guarded<AbstractFragment>,
    call_site: Option<&Rc<SiteFacts>>,
    stringify_scalar_output: bool,
) -> Guarded<AbstractFragment> {
    let mut root = fragment.clone();
    for (_, node) in &mut root.arms {
        rebase_node_sites(node, call_site);
        if stringify_scalar_output {
            mark_root_scalar_output_stringified(node);
        }
    }
    root
}

/// A helper spliced into one YAML value returns text even when its body
/// selected a raw values path. Structured helper roots retain their shape.
fn mark_root_scalar_output_stringified(node: &mut AbstractFragment) {
    match node {
        AbstractFragment::Scalar(scalar) => {
            for part in &mut scalar.parts {
                if let StringPart::Splice(splice) = part
                    && matches!(splice.kind, ValueKind::Scalar | ValueKind::PartialScalar)
                {
                    splice.meta.stringified = true;
                }
            }
        }
        AbstractFragment::Splice(splice)
            if matches!(splice.kind, ValueKind::Scalar | ValueKind::PartialScalar) =>
        {
            splice.meta.stringified = true;
        }
        AbstractFragment::Mapping(_)
        | AbstractFragment::Sequence(_)
        | AbstractFragment::Splice(_)
        | AbstractFragment::Opaque(_) => {}
    }
}

fn rebase_site(
    site: &mut Option<Rc<SiteFacts>>,
    provenance: &mut Vec<ContractProvenance>,
    call_site: Option<&Rc<SiteFacts>>,
) {
    match site.as_deref() {
        Some(body_site) if body_site.resource.is_none() => {
            let mut merged: Vec<ContractProvenance> =
                body_site.provenance.iter().cloned().collect();
            merge_provenance_sites(&mut merged, provenance);
            *provenance = merged;
            *site = call_site.cloned();
        }
        Some(_) => {}
        None => *site = call_site.cloned(),
    }
}

fn rebase_node_sites(node: &mut AbstractFragment, call_site: Option<&Rc<SiteFacts>>) {
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
        match condition.kind() {
            helm_schema_core::PredicateKind::True => {}
            helm_schema_core::PredicateKind::And(parts) => branch.extend(parts.iter().cloned()),
            _ => {
                branch.insert(condition.clone());
            }
        }
    }
    branch
}

fn splice_row_meta(splice: &Splice, conditions: &[PathCondition]) -> HelperOutputMeta {
    let format_only = matches!(
        splice.kind,
        ValueKind::GoFormattedMap | ValueKind::GoFormattedSequence
    );
    let mut provenance: Vec<ContractProvenance> = splice
        .meta
        .site
        .as_deref()
        .and_then(|site| site.provenance.clone())
        .into_iter()
        .collect();
    merge_provenance_sites(&mut provenance, &splice.meta.provenance);
    let mut meta = HelperOutputMeta {
        input_identity: splice.meta.input_identity && !format_only,
        defaulted: splice.meta.defaulted,
        shape_erased: splice.meta.shape_erased || format_only,
        derived_text: format_only,
        // A helper returns rendered text even when its body selected a raw
        // values path. Explicit metadata still carries conversions that
        // happened before the helper boundary.
        stringified: !format_only
            && (splice.meta.stringified
                || matches!(splice.kind, ValueKind::Scalar | ValueKind::PartialScalar)),
        nil_omitted: splice.meta.nil_omitted,
        templated_yaml: splice.meta.templated_yaml,
        plain_slot_string_format: splice.meta.plain_slot_string_format,
        json_serialized: splice.meta.json_serialized,
        json_decoded: splice.meta.json_decoded,
        lexical_escapes: splice.meta.lexical_escapes.clone(),
        omitted_keys: splice.meta.omitted_members.clone(),
        // Crossing the helper-summary boundary makes the layer facts
        // binding-carried: the caller renders the helper's OUTPUT, whose
        // sibling dispatch arms may rely on the base typing the direct
        // render-site lane moves onto synthesized arms.
        merge_layers: splice
            .meta
            .merge_layers
            .clone()
            .map(helm_schema_core::MergeLayersUse::into_via_binding),
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
        AbstractValue::FirstTruthy(candidates) => AbstractValue::FirstTruthy(
            candidates
                .iter()
                .map(|candidate| project_structured_taint_value(candidate, outer_meta))
                .collect(),
        ),
        AbstractValue::MergedLayers(layers) => AbstractValue::merged_layers(
            layers
                .iter()
                .map(|layer| project_structured_taint_value(layer, outer_meta))
                .collect(),
        )
        .unwrap_or(AbstractValue::Unknown),
        AbstractValue::Top
        | AbstractValue::Unknown
        | AbstractValue::RangeKey(_)
        | AbstractValue::KeysList(_)
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

/// Projects existing tree arms into values while retaining splice conditions in metadata.
pub(crate) fn projected_value(root: &Guarded<AbstractFragment>) -> Option<AbstractValue> {
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
        if *condition == Predicate::False {
            continue;
        }
        let pushed = *condition != Predicate::True;
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
            // A scalar composing literal text AROUND splices renders each
            // spliced path as DERIVED TEXT rather than as the bare value:
            // a structural re-lowering of the projection (the pod-template
            // `fromYaml | toYaml` roundtrip) must keep the partial-text
            // discipline instead of minting full-value preimages
            // (traefik's `--…={{ $value }}` flag items). Splice-only part
            // sets stay bare — contribution-set degradation merges
            // ALTERNATIVE renders into one part list (airflow's
            // nil-aware `revisionHistoryLimit` picker), where each arm
            // still renders its raw value exactly.
            let has_literal_text = scalar.parts.iter().any(|part| {
                matches!(
                    part,
                    StringPart::Text(alternatives)
                        if alternatives.iter().any(|text| !text.is_empty())
                )
            });
            let partial = has_literal_text && scalar.parts.len() > 1;
            for part in &scalar.parts {
                match part {
                    StringPart::Text(alternatives) => strings.extend(alternatives.iter().cloned()),
                    StringPart::Splice(splice) => {
                        has_non_text = true;
                        let mut meta = splice_row_meta(splice, conditions);
                        meta.partial_text |= partial;
                        values.push(AbstractValue::OutputPath(splice.values_path.clone(), meta));
                    }
                    StringPart::Taint(taint) => {
                        has_non_text = true;
                        if !taint.claims_value_kind {
                            values.push(AbstractValue::Widened(taint.paths.clone()));
                            continue;
                        }
                        let mut meta = scalar_taint_row_meta(taint, conditions);
                        meta.partial_text |= partial;
                        if let Some(value) = &taint.structured_value {
                            values.push(project_structured_taint_value(value, &meta));
                        } else {
                            for path in &taint.paths {
                                values.push(output_path(path, meta.clone()));
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
                .map(|path| output_path(path, meta.clone()))
                .collect()
        }
    }
}

fn output_path(path: &helm_schema_core::ValuesPath, meta: HelperOutputMeta) -> AbstractValue {
    AbstractValue::OutputPath(path.clone(), meta)
}

#[derive(Default)]
struct SummaryRows {
    rendered: Vec<RenderedRow>,
    suppressed_reads: Vec<ValueRead>,
}

/// Flatten rendered claims and render-suppressed dependency reads in one
/// traversal while keeping the two result lanes independently ordered.
fn summary_rows(root: &Guarded<AbstractFragment>) -> SummaryRows {
    let mut rows = SummaryRows::default();
    let mut conditions = Vec::new();
    collect_summary_rows(root, &mut conditions, false, &mut rows);
    rows
}

/// Flattens rendered claims and appends suppressed dependency reads.
pub(crate) fn finalize_summary_rows(
    root: &Guarded<AbstractFragment>,
    reads: &mut Vec<ValueRead>,
) -> Vec<RenderedRow> {
    let SummaryRows {
        rendered,
        suppressed_reads,
    } = summary_rows(root);
    prune_sibling_conditions(reads, &rendered);
    // Render-suppressed splices (block-scalar bodies) influence the text
    // without rendering a sink-typed value; value-position consumers see
    // them as dependency reads, matching the demotion the summary fold
    // applied at suppressing slots.
    for read in suppressed_reads {
        if !reads.contains(&read) {
            reads.push(read);
        }
    }
    rendered
}

fn collect_summary_rows(
    guarded: &Guarded<AbstractFragment>,
    conditions: &mut Vec<PathCondition>,
    suppressed: bool,
    rows: &mut SummaryRows,
) {
    for (condition, node) in &guarded.arms {
        if *condition == Predicate::False {
            continue;
        }
        let pushed = *condition != Predicate::True;
        if pushed {
            conditions.push(condition.clone());
        }
        collect_summary_node(node, conditions, suppressed, rows);
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
    let kind = if matches!(
        kind,
        ValueKind::GoFormattedMap | ValueKind::GoFormattedSequence
    ) {
        ValueKind::Serialized
    } else {
        kind
    };
    let path = helm_schema_core::ValuesPath::parse(path);
    if path.segments().next().is_none() {
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
        path,
        kind,
        encoded,
        meta,
    });
}

fn collect_summary_node(
    node: &AbstractFragment,
    conditions: &mut Vec<PathCondition>,
    suppressed: bool,
    rows: &mut SummaryRows,
) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mapping.entries {
                collect_summary_rows(&entry.value, conditions, suppressed, rows);
            }
        }
        AbstractFragment::Sequence(sequence) => {
            for item in &sequence.items {
                collect_summary_rows(item, conditions, suppressed, rows);
            }
        }
        AbstractFragment::Scalar(scalar) => {
            let suppressed = suppressed || scalar.suppressed;
            if scalar.suppressed {
                collect_suppressed_scalar_reads(
                    &scalar.parts,
                    conditions,
                    &mut rows.suppressed_reads,
                );
            }
            if suppressed {
                return;
            }
            for part in &scalar.parts {
                match part {
                    StringPart::Text(_) => {}
                    StringPart::Splice(splice) => push_rendered_row(
                        &mut rows.rendered,
                        &splice.values_path.encode(),
                        splice.kind,
                        splice.meta.encoded,
                        splice_row_meta(splice, conditions),
                    ),
                    StringPart::Taint(taint) => {
                        if !taint.claims_value_kind {
                            continue;
                        }
                        let meta = scalar_taint_row_meta(taint, conditions);
                        for path in &taint.paths {
                            push_rendered_row(
                                &mut rows.rendered,
                                &path.encode(),
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
                    &mut rows.rendered,
                    &splice.values_path.encode(),
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
                push_rendered_row(
                    &mut rows.rendered,
                    &path.encode(),
                    opaque.kind,
                    false,
                    meta.clone(),
                );
            }
        }
    }
}

/// Dependency reads for splices inside render-suppressed scalars.
fn collect_suppressed_scalar_reads(
    parts: &[StringPart],
    conditions: &[PathCondition],
    reads: &mut Vec<ValueRead>,
) {
    for part in parts {
        let (paths, site, provenance): (Vec<String>, Option<&SiteFacts>, &[ContractProvenance]) =
            match part {
                StringPart::Text(_) => continue,
                StringPart::Splice(splice) => (
                    vec![splice.values_path.encode()],
                    splice.meta.site.as_deref(),
                    &splice.meta.provenance,
                ),
                StringPart::Taint(taint) => {
                    if !taint.claims_value_kind {
                        continue;
                    }
                    (
                        taint
                            .paths
                            .iter()
                            .map(helm_schema_core::ValuesPath::encode)
                            .collect(),
                        taint.site.as_deref(),
                        &taint.provenance,
                    )
                }
            };
        let meta = taint_row_meta(site, provenance, conditions);
        for path in paths {
            if path.trim().is_empty() {
                continue;
            }
            let read = ValueRead {
                values_path: helm_schema_core::ValuesPath::parse(&path),
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

/// The summary lane's sibling-condition rule for pathless reads: a
/// truthiness condition about a *different* summary source describes that
/// sibling's branch, not this read's, and drops unless the paths are
/// related. A read defaulted on its own path additionally drops its own
/// negation when a positive condition remains (the default supplies the
/// value on the negative side).
fn prune_sibling_conditions(reads: &mut Vec<ValueRead>, rendered: &[RenderedRow]) {
    let mut sources: BTreeSet<String> =
        reads.iter().map(|read| read.values_path.encode()).collect();
    sources.extend(rendered.iter().map(|row| row.path.encode()));
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
        let read_path = read.values_path.encode();
        let conjunctions = read.condition.disjuncts().iter().map(|conjunction| {
            let has_truthy_sibling = conjunction.iter().any(|predicate| {
                matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(crate::Guard::Truthy { path }) if unrelated_sibling(&path.encode(), &read_path))
            });
            let defaulted = conjunction.iter().any(|predicate| {
                matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(crate::Guard::Default { path }) if path == &read.values_path)
            });
            let has_self_truthy = conjunction.iter().any(|predicate| {
                matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(crate::Guard::Truthy { path }) if path == &read.values_path)
            });
            let mut predicates = conjunction
                .iter()
                .filter(|predicate| {
                    !matches!(predicate.kind(), helm_schema_core::PredicateKind::Guard(crate::Guard::Truthy { path }) if unrelated_sibling(&path.encode(), &read_path))
                })
                .cloned()
                .collect::<Vec<_>>();
            if defaulted && (has_self_truthy || has_truthy_sibling) {
                predicates.retain(|predicate| {
                    !matches!(predicate.kind(), helm_schema_core::PredicateKind::Not(inner) if matches!(inner.kind(), helm_schema_core::PredicateKind::Guard(crate::Guard::Truthy { path }) if path == &read.values_path))
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
