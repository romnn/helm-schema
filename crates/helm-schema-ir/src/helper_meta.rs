//! Per-path helper render facts shared between the value lattice and the
//! fragment domain: `OutputPath` values, local bindings, and fragment
//! summaries all carry a [`HelperOutputMeta`] per rendered `.Values` path.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ContractProvenance, ValueKind};
use helm_schema_core::Predicate;

/// The facts one rendered path carries out of a helper body: the branch
/// conditions under which it renders (one set per branch), whether the
/// render site substitutes a fallback, and the body sites it was derived
/// through.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct HelperOutputMeta {
    pub(crate) predicates: BTreeSet<BTreeSet<Predicate>>,
    pub(crate) defaulted: bool,
    pub(crate) provenance: Vec<ContractProvenance>,
    /// Predicate paths this row's derivation explicitly severed (index-call
    /// narrowing): guard reads of their strict ancestors are dropped.
    pub(crate) suppress_predicate_paths: BTreeSet<String>,
}

impl HelperOutputMeta {
    pub(crate) fn merge(&mut self, other: &Self) {
        self.predicates.extend(other.predicates.iter().cloned());
        self.defaulted |= other.defaulted;
        merge_provenance_sites(&mut self.provenance, &other.provenance);
        self.suppress_predicate_paths
            .extend(other.suppress_predicate_paths.iter().cloned());
    }

    pub(crate) fn suppress_predicate_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !path.is_empty() {
            self.suppress_predicate_paths.insert(path);
        }
    }

    /// Conjoin `predicates` onto every recorded branch (one fresh branch when
    /// none are recorded yet).
    pub(crate) fn conjoin_branches(&mut self, predicates: &BTreeSet<Predicate>) {
        if predicates.is_empty() {
            return;
        }
        if self.predicates.is_empty() {
            self.predicates.insert(predicates.clone());
            return;
        }
        self.predicates = std::mem::take(&mut self.predicates)
            .into_iter()
            .map(|mut branch| {
                branch.extend(predicates.iter().cloned());
                branch
            })
            .collect();
    }
}

/// One rendered claim of a helper call flattened from its summary fragment:
/// the path, its render kind, encoding, and per-path meta. Call sites use
/// these for no-render demotion (assignments and conditions read the paths
/// without rendering them) and for restoring per-path branch meta when
/// transfer functions collapse the value shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RenderedRow {
    pub(crate) path: String,
    pub(crate) kind: ValueKind,
    pub(crate) encoded: bool,
    pub(crate) meta: HelperOutputMeta,
}

/// Merges the meta of every rendered row into a per-source meta map (the
/// shape local bindings carry).
pub(crate) fn merge_rendered_row_meta(
    output_meta: &mut BTreeMap<String, HelperOutputMeta>,
    rows: &[RenderedRow],
) {
    for row in rows {
        output_meta
            .entry(row.path.clone())
            .or_default()
            .merge(&row.meta);
    }
}

/// Appends `extra` provenance sites onto `target`, preserving first-seen
/// order and skipping sites already present. Every provenance merge in the
/// contract pipeline uses this discipline so emitted site lists stay
/// deterministic.
pub(crate) fn merge_provenance_sites(
    target: &mut Vec<ContractProvenance>,
    extra: &[ContractProvenance],
) {
    for site in extra {
        if !target.contains(site) {
            target.push(site.clone());
        }
    }
}

/// Whether two values paths describe related data: same top-level root, or
/// one is an ancestor of the other.
pub(crate) fn values_paths_are_related(left: &str, right: &str) -> bool {
    fn root(path: &str) -> &str {
        path.split('.').next().unwrap_or(path)
    }
    root(left) == root(right)
        || helm_schema_core::values_path_is_descendant(left, right)
        || helm_schema_core::values_path_is_descendant(right, left)
}

pub(crate) fn insert_type_hint(
    hints: &mut BTreeMap<String, BTreeSet<String>>,
    path: String,
    schema_type: &str,
) {
    if path.trim().is_empty() {
        return;
    }
    hints
        .entry(path)
        .or_default()
        .insert(schema_type.to_string());
}
