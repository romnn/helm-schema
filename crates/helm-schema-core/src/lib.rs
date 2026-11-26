//! Shared semantic contracts used by Helm analysis and schema providers.

mod capability;
mod capability_liveness;
mod contract_signals;
mod contract_use;
mod guard;
/// Normalization helpers for conjunction and disjunction guard sets.
mod guard_algebra;
mod guard_conjunction;
mod guard_dnf;
mod output_path;
mod pattern_dialect;
mod predicate;
mod predicate_bdd;
mod provenance;
mod provider_origin;
mod provider_schema_fragment;
mod provider_schema_use;
mod release_namespace;
mod requirement_dag;
mod requirement_guard;
mod schema_provider;
mod text_preimage;
mod types;
mod value_path;
mod value_pattern;

pub use capability::{ApiPresenceQuery, CapabilityGuard, HelperBranch, HelperBranchBody};
pub use capability_liveness::{CapabilityOracle, live_literals};
pub use contract_signals::{
    AllUses, ConditionalGuard, ConditionalOverlayEvidence, ConditionalOverlayFlavor,
    ConditionalPathOverlay, ContractPathSchemaEvidence, ContractRequirednessEvidence,
    ContractRequirementImplication, ContractRequirementTarget, ContractSchemaSignals,
    ContractValuePathFacts, FailValueRequirement, GuardedValuesDefaultSource, MetadataFieldKind,
    RangeDomain, RequirementPattern, ResourceObservation, ValuesDefaultSource,
};
pub use contract_use::{
    ContractUse, MergeLayer, MergeLayerTransform, MergeLayersUse, SplitSegmentUse,
};
pub use guard::{CollectionDomain, CollectionQuantifier, Guard, GuardValue};
pub use guard_conjunction::{CanonicalGuardComponents, GuardConjunction, GuardIter, GuardPrefix};
pub use guard_dnf::GuardDnf;
pub use output_path::{
    DYNAMIC_MAPPING_VALUE_SEGMENT, append_relative_path, dynamic_mapping_value_path,
    sequence_item_path, values_path_has_descendant, values_path_is_descendant,
};
pub use pattern_dialect::{
    escape_regex_literal, normalize_schema_pattern_dialects, re2_to_ecma_pattern,
    unicode_mode_escaped_pattern,
};
pub use predicate::{ApproximationRole, Conjunction, Predicate, PredicateKind};
pub use predicate_bdd::{
    PredicateMemo, PredicateResidualAtom, PredicateResidualNode, PredicateResiduals,
};
pub use provenance::{ContractProvenance, SourceSpan};
pub use provider_origin::ProviderOrigin;
pub use provider_schema_fragment::{
    ProviderSchemaFragment, ProviderSchemaSource, ProviderSourceFragment,
};
pub use provider_schema_use::ProviderSchemaUse;
pub use release_namespace::{
    RELEASE_NAMESPACE_MAX_BYTES, RELEASE_NAMESPACE_MIN_BYTES, RELEASE_NAMESPACE_PATTERN,
    is_release_namespace,
};
pub use requirement_dag::{
    RequirementCondition, RequirementConditionBuilder, RequirementConditionNode,
    RequirementConditions,
};
pub use requirement_guard::{
    RequirementAtom, RequirementChoice, RequirementDecision, RequirementGuard, RequirementNode,
};
pub use schema_provider::ResourceSchemaOracle;
pub use text_preimage::{
    RegularLanguage, RegularTransition, TextPreimage, pattern_end_states, regex_character_class,
};
pub use types::{KindBranch, PlainScalarPlacement, ResourceRef, ValueKind, YamlPath};
pub use value_path::{
    Segment, ValuesPath, append_each_member_value_path, append_value_path, join_encoded_value_path,
    join_value_path, split_value_path,
};
pub use value_pattern::ValuePattern;

pub use predicate_bdd::RequirementBdd;
