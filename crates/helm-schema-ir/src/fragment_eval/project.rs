//! Projection: contract claims read off the fragment tree.
//!
//! A value use is one `(values_path, yaml_path, condition)` claim: splices
//! attribute at their tree position with the root-to-leaf conditions
//! projected to the contract predicate vocabulary; pathless reads
//! (conditions, assignment right-hand sides, helper-internal guard reads)
//! carry the condition recorded at their read site. Row facts beyond the
//! claim triple come from the
//! render-site stamps: the containing resource (kept on placed rows, and on
//! site-scoped reads exactly like the previous emission terminal), List-item
//! path rebasing, and source provenance.

use crate::contract::ContractIr;
use crate::{ContractProvenance, ContractUse, ValueKind, YamlPath};
use helm_schema_core::{GuardDnf, Predicate, dynamic_mapping_value_path, sequence_item_path};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, SiteFacts, Splice, StringPart,
};
use super::eval::EvaluatedDocument;

/// Project an evaluated document into the contract graph.
#[must_use]
#[tracing::instrument(skip_all, level = "debug", fields(reads = document.reads.len()))]
pub(crate) fn contract_ir_from_document(document: &EvaluatedDocument) -> ContractIr {
    let mut contract = ContractIr::default();
    let mut conditions = Vec::new();
    walk_guarded(
        &document.root,
        &YamlPath(Vec::new()),
        &mut conditions,
        &mut contract,
        &std::collections::BTreeSet::new(),
    );
    for read in &document.reads {
        if read.condition.is_never() {
            continue;
        }
        let row = ContractUse::with_condition_and_provenances(
            read.values_path.clone(),
            YamlPath(Vec::new()),
            read.kind,
            read.condition.clone(),
            read.resource.clone(),
            read.provenance.iter().cloned(),
        );
        if read.dependency {
            contract.push_dependency_use(row);
        } else {
            contract.push(row);
        }
    }
    contract.absorb_observed_facts(&document.observed_facts);
    contract
}

fn walk_guarded(
    guarded: &Guarded<AbstractFragment>,
    path: &YamlPath,
    conditions: &mut Vec<Predicate>,
    contract: &mut ContractIr,
    member_sibling_keys: &std::collections::BTreeSet<String>,
) {
    // Sibling MAPPING arms of the same guarded position contribute literal
    // keys to the object a member-level splice completes (`- name: tmp`
    // above a `toYaml .Values.tmpVolume | nindent` action): the splice's
    // provider slot must know them so its object requiredness does not
    // re-demand template-supplied members. Conditional siblings widen the
    // set, which can only relax requiredness, never reject.
    let mut sibling_keys = member_sibling_keys.clone();
    for (_, node) in &guarded.arms {
        if let AbstractFragment::Mapping(mapping) = node {
            sibling_keys.extend(mapping.entries.iter().filter_map(|entry| match &entry.key {
                EntryKey::Literal(key) if !key.is_empty() => Some(key.clone()),
                _ => None,
            }));
        }
    }
    for (condition, node) in &guarded.arms {
        if *condition == Predicate::False {
            continue;
        }
        let pushed = *condition != Predicate::True;
        if pushed {
            conditions.push(condition.clone());
        }
        walk_node(node, path, conditions, contract, &sibling_keys);
        if pushed {
            conditions.pop();
        }
    }
}

fn walk_node(
    node: &AbstractFragment,
    path: &YamlPath,
    conditions: &mut Vec<Predicate>,
    contract: &mut ContractIr,
    member_sibling_keys: &std::collections::BTreeSet<String>,
) {
    let no_siblings = std::collections::BTreeSet::new();
    match node {
        AbstractFragment::Mapping(mapping) => {
            // Literal keys the template itself writes into this mapping: a
            // member-contributing fragment splice's provider slot already
            // holds them, so its object requiredness must not re-demand
            // them from the user value (metrics-server's `- name: tmp`
            // beside `toYaml .Values.tmpVolume`).
            let literal_keys: std::collections::BTreeSet<String> = mapping
                .entries
                .iter()
                .filter_map(|entry| match &entry.key {
                    EntryKey::Literal(key) if !key.is_empty() => Some(key.clone()),
                    _ => None,
                })
                .collect();
            for entry in &mapping.entries {
                match &entry.key {
                    EntryKey::Literal(key) if !key.is_empty() => {
                        let mut child = path.clone();
                        child.0.push(key.clone());
                        walk_guarded(&entry.value, &child, conditions, contract, &no_siblings);
                    }
                    EntryKey::Literal(_) => {
                        walk_guarded(&entry.value, path, conditions, contract, &literal_keys);
                    }
                    EntryKey::Dynamic(_) => {
                        // Templated keys: the key's reads were recorded at
                        // the eval site, where range/branch predicates were
                        // still ambient. The structural member segment lets
                        // provider lookup descend through the container's
                        // additionalProperties schema without guessing the
                        // rendered key.
                        let child = dynamic_mapping_value_path(path);
                        walk_guarded(&entry.value, &child, conditions, contract, &no_siblings);
                    }
                }
            }
        }
        AbstractFragment::Sequence(sequence) => {
            let item_path = sequence_item_path(path);
            for item in &sequence.items {
                walk_guarded(item, &item_path, conditions, contract, &no_siblings);
            }
        }
        AbstractFragment::Scalar(scalar) => {
            // Render-suppressed blobs (block scalar bodies) influence their
            // text without sink-typing the document position.
            let effective_path = if scalar.suppressed {
                YamlPath(Vec::new())
            } else {
                path.clone()
            };
            project_parts(scalar, &effective_path, conditions, contract);
        }
        AbstractFragment::Splice(splice) => {
            let row = splice_row(splice, path, conditions, member_sibling_keys);
            if !row.condition.is_never() {
                contract.push(row);
            }
        }
    }
}

fn project_parts(
    scalar: &AbstractString,
    path: &YamlPath,
    conditions: &[Predicate],
    contract: &mut ContractIr,
) {
    for part in &scalar.parts {
        match part {
            StringPart::Text(_) | StringPart::Taint(_) => {}
            StringPart::Splice(splice) => {
                let mut row =
                    splice_row(splice, path, conditions, &std::collections::BTreeSet::new());
                if scalar.suppressed {
                    row.kind = ValueKind::Serialized;
                }
                if !row.condition.is_never() {
                    contract.push(row);
                }
            }
        }
    }
}

fn splice_row(
    splice: &Splice,
    path: &YamlPath,
    conditions: &[Predicate],
    member_sibling_keys: &std::collections::BTreeSet<String>,
) -> ContractUse {
    let mut condition = GuardDnf::from_conjunction(conditions.iter().cloned());
    if splice.meta.is_input_identity()
        && !path.0.is_empty()
        && splice
            .meta
            .site
            .as_ref()
            .and_then(|site| site.resource.as_ref())
            .is_some()
    {
        // An approximate execution predicate can still expose a sound live
        // subset. It is safe to retain that subset for an identity splice:
        // the provider sees this input value there. Derived scalar influence
        // stays opaque and therefore keeps the approximation.
        condition = GuardDnf::from_disjunction(condition.disjuncts().iter().map(|conjunction| {
            conjunction.iter().map(|predicate| match predicate.kind() {
                helm_schema_core::PredicateKind::Approximate {
                    marker: _,
                    paths: _,
                    role,
                    sound_subset,
                } => {
                    if role == helm_schema_core::ApproximationRole::OutputSelection
                        && predicate.value_paths().contains(&splice.values_path)
                        && let Some(sound_subset) = sound_subset
                    {
                        sound_subset.clone()
                    } else {
                        predicate.clone()
                    }
                }
                helm_schema_core::PredicateKind::True
                | helm_schema_core::PredicateKind::False
                | helm_schema_core::PredicateKind::Guard(_)
                | helm_schema_core::PredicateKind::Not(_)
                | helm_schema_core::PredicateKind::And(_)
                | helm_schema_core::PredicateKind::Or(_) => predicate.clone(),
            })
        }));
    }
    let mut row = placed_row(
        splice.values_path.clone(),
        path,
        splice.kind,
        condition,
        splice.meta.site.as_deref(),
    );
    row.stringified = splice.meta.stringified;
    row.strip_marker = splice.meta.strip_marker;
    row.omitted_members = splice.meta.omitted_members.clone();
    row.plain_scalar_placement = splice.meta.plain_scalar_placement;
    row.template_supplied_member_keys = member_sibling_keys.clone();
    row
}

/// One placed row with the shared site policy applied: List-item path
/// rebasing, partial-scalar normalization at pathless positions, the site's
/// resource scope, and source provenance.
fn placed_row(
    values_path: helm_schema_core::ValuesPath,
    path: &YamlPath,
    kind: ValueKind,
    condition: GuardDnf,
    site: Option<&SiteFacts>,
) -> ContractUse {
    let mut path = path.clone();
    if let Some(site) = site
        && !site.path_prefix.is_empty()
        && path.0.starts_with(&site.path_prefix)
    {
        path = YamlPath(
            path.0
                .get(site.path_prefix.len()..)
                .unwrap_or_default()
                .to_vec(),
        );
    }
    let mut kind = kind;
    if kind == ValueKind::PartialScalar && path.0.is_empty() {
        kind = ValueKind::Scalar;
    }
    let provenance: Vec<ContractProvenance> = site
        .and_then(|site| site.provenance.clone())
        .into_iter()
        .collect();
    ContractUse::with_condition_and_provenances(
        values_path,
        path,
        kind,
        condition,
        site.and_then(|site| site.resource.clone()),
        provenance,
    )
}
