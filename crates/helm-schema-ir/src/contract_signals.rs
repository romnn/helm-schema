use std::collections::{BTreeMap, BTreeSet};

use crate::ChartFacts;
use crate::provider_schema_use::ProviderSchemaUse;

/// Type-level constraints declared by template guards.
///
/// These are contract facts, not JSON Schema fragments. Schema lowering stays
/// in the generator so the contract layer remains independent from output
/// format policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuardConstraint {
    /// `if eq .Values.X "value"` admits the literal value when the branch
    /// renders.
    Eq { value: String },
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

/// Compatibility signal for the optional `required` schema post-pass.
///
/// The contract layer identifies which paths look like positive guard headers
/// and which paths are ruled out by optional control flow. JSON Schema mutation
/// remains a generator policy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequiredInferenceSignals {
    pub positive_header_paths: BTreeSet<String>,
    pub conditionally_optional_paths: BTreeSet<String>,
    pub default_fallback_paths: BTreeSet<String>,
}

/// Contract-derived facts consumed by core values-schema generation.
///
/// This is the typed boundary between static template interpretation and JSON
/// Schema lowering. Optional post-passes can ask for their own projections,
/// but core schema generation should consume this artifact rather than
/// re-reading raw contract claims.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractSchemaSignals {
    pub chart_facts: ChartFacts,
    pub path_signals: ContractPathSignals,
    pub provider_schema_uses: Vec<ProviderSchemaUse>,
    pub nullable_value_paths: BTreeSet<String>,
    pub paths_with_referenced_descendants: BTreeSet<String>,
    pub value_path_facts: BTreeMap<String, ContractValuePathFacts>,
    pub required_inference_signals: RequiredInferenceSignals,
}

/// Schema-generation facts for one input values path.
///
/// This bundles the contract-owned path state that schema lowering needs, so
/// generator code does not have to reconstruct semantic facts from multiple
/// lower-level projections.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContractValuePathFacts {
    pub has_referenced_descendants: bool,
    pub used_as_fragment: bool,
    pub is_ranged_source: bool,
    pub is_partial_scalar_value_path: bool,
    pub has_render_use: bool,
    pub all_render_uses_self_guarded: bool,
    pub has_self_range_guard_render_use: bool,
    pub is_nullable: bool,
}
