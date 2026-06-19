use std::collections::{BTreeMap, BTreeSet};

use crate::GuardValue;
use crate::provider_schema_use::ProviderSchemaUse;

/// Values-decidable guard expression that can be lowered into JSON Schema
/// conditionals.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConditionalGuard {
    Truthy { path: String },
    Eq { path: String, value: GuardValue },
    NotEq { path: String, value: GuardValue },
    Absent { path: String },
    TypeIs { path: String, schema_type: String },
    Not(Box<ConditionalGuard>),
    AllOf(Vec<ConditionalGuard>),
    AnyOf(Vec<ConditionalGuard>),
}

/// Conditionally-scoped values path whose schema can be lowered under a
/// values-decidable guard set.
///
/// Multiple entries in `guards` mean conjunction: all guards in the set must
/// hold for the overlay to apply.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConditionalPathOverlay {
    pub target_value_path: String,
    pub guards: Vec<ConditionalGuard>,
    pub evidence: ContractPathSchemaEvidence,
    /// Keep the unconditional/base schema for this path alongside the guarded
    /// overlay because the contract also observed an unguarded use.
    pub preserve_base_schema: bool,
}

/// Type-level constraints declared by template guards.
///
/// These are contract facts, not JSON Schema fragments. Schema lowering stays
/// in the generator so the contract layer remains independent from output
/// format policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardConstraint {
    /// `if eq .Values.X "value"` admits the literal value when the branch
    /// renders.
    Eq { value: GuardValue },
    /// `if typeIs "<json type>" .Values.X` declares the type accepted by the
    /// branch.
    TypeIs { schema_type: String },
}

/// Kubernetes `metadata.*` field shape referenced by a values path.
///
/// The contract layer records the field category structurally from the
/// rendered document path. JSON Schema lowering remains a generator policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetadataFieldKind {
    /// `metadata.labels` and `metadata.annotations`.
    StringMap,
    /// `metadata.name`.
    Name,
    /// `metadata.namespace`.
    Namespace,
}

/// Path-level facts derived directly from normalized contract claims.
///
/// These are the values paths that downstream schema generation must consider,
/// plus typed guard facts that can be lowered by generator policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractPathSignals {
    pub referenced_value_paths: BTreeSet<String>,
    pub ranged_value_paths: BTreeSet<String>,
    pub value_paths_used_as_fragment: BTreeSet<String>,
    pub partial_scalar_value_paths: BTreeSet<String>,
    pub guard_constraints_by_value_path: BTreeMap<String, Vec<GuardConstraint>>,
    pub metadata_fields_by_value_path: BTreeMap<String, BTreeSet<MetadataFieldKind>>,
}

/// All schema-lowering evidence for one values path.
///
/// The contract layer owns this view so downstream generation can consume one
/// path-local static-analysis fact instead of reassembling meaning from
/// several parallel maps.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractPathSchemaEvidence {
    pub value_path: String,
    pub is_referenced_value_path: bool,
    pub facts: ContractValuePathFacts,
    pub guard_constraints: Vec<GuardConstraint>,
    pub metadata_field_kinds: BTreeSet<MetadataFieldKind>,
    pub type_hints: BTreeSet<String>,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
    pub requiredness: ContractRequirednessEvidence,
}

/// Contract-derived facts consumed by core values-schema generation.
///
/// This is the typed boundary between static template interpretation and JSON
/// Schema lowering. Optional post-passes can ask for their own projections,
/// but core schema generation should consume this artifact rather than
/// re-reading raw contract claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractSchemaSignals {
    pub path_signals: ContractPathSignals,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
    pub type_hints_by_value_path: BTreeMap<String, BTreeSet<String>>,
    pub nullable_value_paths: BTreeSet<String>,
    pub paths_with_referenced_descendants: BTreeSet<String>,
    pub value_path_facts: BTreeMap<String, ContractValuePathFacts>,
    pub schema_evidence_by_value_path: BTreeMap<String, ContractPathSchemaEvidence>,
    pub conditional_path_overlays: Vec<ConditionalPathOverlay>,
}

/// Schema-generation facts for one input values path.
///
/// This bundles the contract-owned path state that schema lowering needs, so
/// generator code does not have to reconstruct semantic facts from multiple
/// lower-level projections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractValuePathFacts {
    pub has_referenced_descendants: bool,
    pub used_as_fragment: bool,
    pub is_ranged_source: bool,
    pub is_partial_scalar_value_path: bool,
    pub has_render_use: bool,
    pub has_self_guarded_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_self_range_guard_render_use: bool,
    pub is_nullable: bool,
}

/// Path-local evidence consumed by the optional `--infer-required` post-pass.
///
/// These are still static-analysis facts, not a decision that the path must be
/// required. The generator combines them with render-use facts and chart
/// defaults before mutating the JSON Schema.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContractRequirednessEvidence {
    pub is_positive_header: bool,
    pub is_conditionally_optional: bool,
    pub has_default_fallback: bool,
}
