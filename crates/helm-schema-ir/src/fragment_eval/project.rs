//! Projection: contract claims read off the fragment tree.
//!
//! A value use is one `(values_path, yaml_path, condition)` claim: splices and
//! taint attribute at their tree position with the root-to-leaf conditions
//! projected to the contract predicate vocabulary; pathless reads
//! (conditions, assignment right-hand sides, helper-internal guard reads)
//! carry the condition recorded at their read site. Row facts beyond the
//! claim triple come from the
//! render-site stamps: the containing resource (kept on placed rows, and on
//! site-scoped reads exactly like the previous emission terminal), List-item
//! path rebasing, and source provenance.

use crate::contract::ContractIr;
use crate::{ContractProvenance, ContractUse, Guard, ValueKind, YamlPath};
use helm_schema_core::{GuardDnf, Predicate, dynamic_mapping_value_path, sequence_item_path};

use super::domain::{
    AbstractFragment, AbstractString, EntryKey, Guarded, SiteFacts, Splice, StringPart,
};
use super::eval::EvaluatedDocument;

/// Project an evaluated document into the contract graph.
#[must_use]
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
    contract.extend_type_hints(
        document
            .type_hints
            .iter()
            .map(|(path, hints)| (path.clone(), hints.clone())),
    );
    contract.extend_guarded_type_hints(
        document
            .guarded_type_hints
            .iter()
            .map(|(path, hints)| (path.clone(), hints.clone())),
    );
    contract.extend_fallback_type_hints(
        document
            .fallback_type_hints
            .iter()
            .map(|(path, hints)| (path.clone(), hints.clone())),
    );
    contract.extend_guarded_fallback_type_hints(
        document
            .guarded_fallback_type_hints
            .iter()
            .map(|(path, hints)| (path.clone(), hints.clone())),
    );
    contract.extend_shape_erased_value_paths(document.shape_erased_paths.iter().cloned());
    contract.extend_string_contract_value_paths(document.string_contract_paths.iter().cloned());
    contract.merge_range_modes(&document.range_modes);
    contract.extend_values_default_sources(document.values_default_sources.iter().cloned());
    contract.extend_fail_conditions(document.fail_conditions.iter().cloned());
    contract
}

fn walk_guarded(
    guarded: &Guarded<AbstractFragment>,
    path: &YamlPath,
    conditions: &mut Vec<Predicate>,
    contract: &mut ContractIr,
    member_sibling_keys: &std::collections::BTreeSet<String>,
) {
    let open_mapping_entry = find_open_mapping_entry(guarded);
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
        let pushed = !condition.is_trivial();
        if pushed {
            conditions.push(condition.clone());
        }
        let mut effective_path = path.clone();
        if let Some((owner, key)) = &open_mapping_entry
            && arm_continues_open_mapping_entry(owner, key, condition, node)
        {
            effective_path.0.push(key.to_string());
        }
        walk_node(node, &effective_path, conditions, contract, &sibling_keys);
        if pushed {
            conditions.pop();
        }
    }
}

/// The trailing literal mapping entry an arm leaves OPEN: a valueless
/// `config:`-style header whose only successors are dynamic entries. A
/// `with`-scoped splice (velero's `{{- with .config }} config: {{- range … }}`
/// pattern) emits the header in one arm and the member writes as sibling
/// arms; those siblings belong under the header's key.
fn find_open_mapping_entry(guarded: &Guarded<AbstractFragment>) -> Option<(Predicate, String)> {
    guarded.arms.iter().rev().find_map(|(condition, node)| {
        let AbstractFragment::Mapping(mapping) = node else {
            return None;
        };
        mapping
            .entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, entry)| match &entry.key {
                EntryKey::Literal(key)
                    if !key.is_empty()
                        && entry.value.arms.is_empty()
                        && mapping.entries[index + 1..]
                            .iter()
                            .all(|entry| matches!(entry.key, EntryKey::Dynamic(_))) =>
                {
                    Some((condition.clone(), key.clone()))
                }
                _ => None,
            })
    })
}

/// Whether an arm's content continues the open mapping entry of `owner`:
/// the arm must run under (at least) the owner's conjuncts, and be either a
/// ranged splice or a mapping made only of dynamic entries and the open
/// header itself.
fn arm_continues_open_mapping_entry(
    owner: &Predicate,
    key: &str,
    condition: &Predicate,
    node: &AbstractFragment,
) -> bool {
    if !predicate_is_conjunctive_subset(owner, condition) {
        return false;
    }
    match node {
        AbstractFragment::Splice(_) => predicate_has_range(condition),
        AbstractFragment::Mapping(mapping) => mapping.entries.iter().all(|entry| {
            matches!(entry.key, EntryKey::Dynamic(_))
                || matches!(&entry.key, EntryKey::Literal(entry_key)
                    if entry_key == key && entry.value.arms.is_empty())
        }),
        _ => false,
    }
}

fn predicate_has_range(predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Guard(Guard::Range { .. }) => true,
        Predicate::Not(inner) => predicate_has_range(inner),
        Predicate::And(predicates) | Predicate::Or(predicates) => {
            predicates.iter().any(predicate_has_range)
        }
        Predicate::True
        | Predicate::False
        | Predicate::Approximate { .. }
        | Predicate::Guard(_) => false,
    }
}

fn predicate_is_conjunctive_subset(subset: &Predicate, superset: &Predicate) -> bool {
    fn collect(predicate: &Predicate, out: &mut std::collections::BTreeSet<Predicate>) {
        match predicate {
            Predicate::True => {}
            Predicate::And(predicates) => {
                for predicate in predicates {
                    collect(predicate, out);
                }
            }
            other => {
                out.insert(other.clone());
            }
        }
    }

    let mut subset_conjuncts = std::collections::BTreeSet::new();
    let mut superset_conjuncts = std::collections::BTreeSet::new();
    collect(subset, &mut subset_conjuncts);
    collect(superset, &mut superset_conjuncts);
    subset_conjuncts.is_subset(&superset_conjuncts)
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
        AbstractFragment::Opaque(opaque) => {
            for taint_path in &opaque.taint {
                if taint_path.is_empty() {
                    continue;
                }
                contract.push(placed_row(
                    taint_path.clone(),
                    path,
                    opaque.kind,
                    GuardDnf::from_conjunction(conditions.iter().cloned()),
                    opaque.site.as_deref(),
                    &opaque.provenance,
                ));
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
            StringPart::Text(_) => {}
            StringPart::Splice(splice) => {
                let row = splice_row(splice, path, conditions, &std::collections::BTreeSet::new());
                if !row.condition.is_never() {
                    contract.push(row);
                }
            }
            StringPart::Taint(taint) => {
                for taint_path in &taint.paths {
                    if taint_path.is_empty() {
                        continue;
                    }
                    contract.push(placed_row(
                        taint_path.clone(),
                        path,
                        ValueKind::PartialScalar,
                        GuardDnf::from_conjunction(conditions.iter().cloned()),
                        taint.site.as_deref(),
                        &taint.provenance,
                    ));
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
    if splice.meta.defaulted {
        let default_guard = Guard::Default {
            path: splice.values_path.clone(),
        };
        condition = condition.conjoined_with_guards([default_guard.clone()]);
    }
    // Serialization and encoding transforms don't expose the input shape to
    // the sink schema. Fragment serialization stays distinguishable from a
    // scalar text transform so provider resolution cannot recover its shape.
    // A total stringification (`quote`, `toString`, `join`) erases shape at
    // every position: unlike `b64enc`, its input is not required to be text.
    let kind = if splice.meta.shape_erased {
        ValueKind::Serialized
    } else if splice.meta.encoded {
        if splice.kind == ValueKind::Fragment {
            ValueKind::Serialized
        } else {
            ValueKind::PartialScalar
        }
    } else if splice.meta.yaml_serialized {
        ValueKind::YamlSerialized
    } else {
        splice.kind
    };
    let mut row = placed_row(
        splice.values_path.clone(),
        path,
        kind,
        condition,
        splice.meta.site.as_deref(),
        &splice.meta.provenance,
    );
    row.has_string_contract = splice.meta.string_contract;
    row.template_supplied_member_keys = member_sibling_keys.clone();
    row.split_segment = splice.meta.split_segment.clone();
    row
}

/// One placed row with the shared site policy applied: List-item path
/// rebasing, partial-scalar normalization at pathless positions, the site's
/// resource scope, and site-then-helper provenance.
fn placed_row(
    values_path: String,
    path: &YamlPath,
    kind: ValueKind,
    condition: GuardDnf,
    site: Option<&SiteFacts>,
    helper_provenance: &[ContractProvenance],
) -> ContractUse {
    let mut path = path.clone();
    if let Some(site) = site
        && !site.path_prefix.is_empty()
        && path.0.starts_with(&site.path_prefix)
    {
        path = YamlPath(path.0[site.path_prefix.len()..].to_vec());
    }
    let mut kind = kind;
    if kind == ValueKind::PartialScalar && path.0.is_empty() {
        kind = ValueKind::Scalar;
    }
    let mut provenance: Vec<ContractProvenance> = site
        .and_then(|site| site.provenance.clone())
        .into_iter()
        .collect();
    crate::helper_meta::merge_provenance_sites(&mut provenance, helper_provenance);
    ContractUse::with_condition_and_provenances(
        values_path,
        path,
        kind,
        condition,
        site.and_then(|site| site.resource.clone()),
        provenance,
    )
}
