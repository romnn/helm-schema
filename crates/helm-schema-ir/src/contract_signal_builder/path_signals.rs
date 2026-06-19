use std::collections::{BTreeMap, BTreeSet};

use crate::contract_signals::{GuardConstraint, MetadataFieldKind};

/// Path-level accumulator derived from normalized contract claims before the
/// builder finalizes path-local schema evidence.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ContractPathSignals {
    pub referenced_value_paths: BTreeSet<String>,
    pub ranged_value_paths: BTreeSet<String>,
    pub value_paths_used_as_fragment: BTreeSet<String>,
    pub partial_scalar_value_paths: BTreeSet<String>,
    pub guard_constraints_by_value_path: BTreeMap<String, Vec<GuardConstraint>>,
    pub metadata_fields_by_value_path: BTreeMap<String, BTreeSet<MetadataFieldKind>>,
}
