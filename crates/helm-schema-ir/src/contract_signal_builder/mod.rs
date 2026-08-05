use std::collections::{BTreeMap, BTreeSet};

use crate::scalar_value::TruthCondition;
use crate::{Guard, ProviderSchemaUse, ValueKind, contract::ContractUse};
use helm_schema_core::{
    ApproximationRole, ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractFailImplication, ContractPathSchemaEvidence, ContractRequirednessEvidence,
    ContractRequirementTarget, ContractSchemaSignals, ContractValuePathFacts, FailValueRequirement,
    GuardDnf, GuardValue, MetadataFieldKind, Predicate,
};

mod conditional_overlays;
mod contract_rows;
mod final_signals;
mod input_channels;
mod requirements;

use conditional_overlays::{
    collapse_layered_truthy_gates, collect_paths_with_descendants, conditional_guard_predicates,
    extend_lowerable_predicate, hard_negation_paths, lowerable_conditional_guard_set,
    lowerable_conditional_guard_subset, member_local_truthy_selector, path_contains_wildcard,
    predicate_is_positive_header, predicate_is_self_guarding, predicate_is_self_presence,
    predicate_is_structural_ancestor_guard, predicate_is_unlowerable_output_selection,
    predicate_skips_falsy_source, predicate_tests_source_type, predicate_to_guard,
    provider_schema_use, range_guard_is_iteration_ancestor, ranged_member_parent,
    record_member_range_requirement, terminal_clause_guard,
};
use contract_rows::{
    ContractPathAccumulator, MemberAccessConditions, PathSchemaFactsAccumulator,
    has_selection_chain_marker_stamp, lowerable_range_outer_guards, partition_compatible_hints,
    predicate_is_truthy_disjunction_over, record_contract_use, record_range_input_capture,
    remove_redundant_approximate_conditions,
};
use final_signals::{
    SourceUseFactSplit, finish_schema_signals, metadata_field_kind_from_yaml_path, path_accumulator,
};
pub(crate) use input_channels::derive_schema_signals_from_contract_parts;
use requirements::{record_fail_conjunction, record_member_access_implications};
