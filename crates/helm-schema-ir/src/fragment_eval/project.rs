//! Projection: contract claims read off the fragment tree.
//!
//! A value use is one `(values_path, yaml_path, guards)` claim: splices and
//! taint attribute at their tree position with the root-to-leaf conditions
//! lowered to contract guards; pathless reads (conditions, assignment
//! right-hand sides, helper-internal guard reads) carry the guards recorded
//! at their read site. Row facts beyond the claim triple come from the
//! render-site stamps: the containing resource (kept on placed rows, and on
//! site-scoped reads exactly like the previous emission terminal), List-item
//! path rebasing, and source provenance.

use crate::contract::ContractIr;
use crate::{ContractProvenance, ContractUse, Guard, ValueKind, YamlPath};
use helm_schema_core::{Predicate, sequence_item_path};

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
    );
    for read in &document.reads {
        if guards_are_contradictory(&read.guards) {
            continue;
        }
        let kind = if read.kind == ValueKind::PartialScalar {
            ValueKind::Scalar
        } else {
            read.kind
        };
        let row = ContractUse::with_provenances(
            read.values_path.clone(),
            YamlPath(Vec::new()),
            kind,
            read.guards.clone(),
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
    contract
}

/// Whether a lowered guard stack contains an exact complementary pair
/// (`B ∧ ¬B`): such an arm cannot render (an outer branch condition stacked
/// onto a helper's opposite-arm meta), so its row is dead by construction.
fn guards_are_contradictory(guards: &[Guard]) -> bool {
    guards.iter().any(|guard| match guard {
        Guard::Truthy { path } => guards
            .iter()
            .any(|other| matches!(other, Guard::Not { path: not_path } if not_path == path)),
        Guard::Eq { path, value } => guards.iter().any(|other| {
            matches!(
                other,
                Guard::NotEq { path: other_path, value: other_value }
                    if other_path == path && other_value == value
            )
        }),
        _ => false,
    })
}

fn walk_guarded(
    guarded: &Guarded<AbstractFragment>,
    path: &YamlPath,
    conditions: &mut Vec<Predicate>,
    contract: &mut ContractIr,
) {
    for (condition, node) in &guarded.arms {
        let pushed = !condition.is_trivial();
        if pushed {
            conditions.push(condition.clone());
        }
        walk_node(node, path, conditions, contract);
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
) {
    match node {
        AbstractFragment::Mapping(mapping) => {
            for entry in &mapping.entries {
                match &entry.key {
                    EntryKey::Literal(key) if !key.is_empty() => {
                        let mut child = path.clone();
                        child.0.push(key.clone());
                        walk_guarded(&entry.value, &child, conditions, contract);
                    }
                    EntryKey::Literal(_) => {
                        walk_guarded(&entry.value, path, conditions, contract);
                    }
                    EntryKey::Dynamic(_) => {
                        // Templated keys: the key's reads were recorded at
                        // the eval site (where range/branch predicates were
                        // still ambient); the value attributes at the parent
                        // path without an invented segment.
                        walk_guarded(&entry.value, path, conditions, contract);
                    }
                }
            }
        }
        AbstractFragment::Sequence(sequence) => {
            let item_path = sequence_item_path(path);
            for item in &sequence.items {
                walk_guarded(item, &item_path, conditions, contract);
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
            let row = splice_row(splice, path, conditions);
            if !guards_are_contradictory(&row.guards) {
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
                    Predicate::contract_guard_stack(conditions),
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
                let row = splice_row(splice, path, conditions);
                if !guards_are_contradictory(&row.guards) {
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
                        Predicate::contract_guard_stack(conditions),
                        taint.site.as_deref(),
                        &taint.provenance,
                    ));
                }
            }
        }
    }
}

fn splice_row(splice: &Splice, path: &YamlPath, conditions: &[Predicate]) -> ContractUse {
    let mut guards = Predicate::contract_guard_stack(conditions);
    if splice.meta.defaulted {
        let default_guard = Guard::Default {
            path: splice.values_path.clone(),
        };
        if !guards.contains(&default_guard) {
            guards.push(default_guard);
        }
    }
    // Encoded renders don't expose the value's shape to the sink schema.
    let kind = if splice.meta.encoded {
        ValueKind::PartialScalar
    } else {
        splice.kind
    };
    placed_row(
        splice.values_path.clone(),
        path,
        kind,
        guards,
        splice.meta.site.as_deref(),
        &splice.meta.provenance,
    )
}

/// One placed row with the shared site policy applied: List-item path
/// rebasing, partial-scalar normalization at pathless positions, the site's
/// resource scope, and site-then-helper provenance.
fn placed_row(
    values_path: String,
    path: &YamlPath,
    kind: ValueKind,
    guards: Vec<Guard>,
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
    ContractUse::with_provenances(
        values_path,
        path,
        kind,
        guards,
        site.and_then(|site| site.resource.clone()),
        provenance,
    )
}
