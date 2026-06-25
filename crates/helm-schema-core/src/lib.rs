mod capability;
mod capability_liveness;
mod contract_signals;
mod contract_use;
mod guard;
mod output_path;
mod predicate;
mod provenance;
mod provider_origin;
mod provider_schema_fragment;
mod provider_schema_use;
mod schema_provider;
mod types;

pub use capability::{
    ApiPresenceQuery, CapabilityGuard, CapabilityPresencePredicate, HelperBranch, HelperBranchBody,
};
pub use capability_liveness::{
    CapabilityOracle, StaticOracle, evaluate_guard, live_literals, select_live_branch,
};
pub use contract_signals::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MetadataFieldKind,
};
pub use contract_use::ContractUse;
pub use guard::{Guard, GuardValue};
pub use output_path::{
    append_relative_path, sequence_item_path, values_path_has_descendant, values_path_is_descendant,
};
pub use predicate::Predicate;
pub use provenance::{ContractProvenance, SourceSpan};
pub use provider_origin::ProviderOrigin;
pub use provider_schema_fragment::{
    ProviderSchemaFragment, ProviderSchemaSource, ProviderSourceFragment,
};
pub use provider_schema_use::ProviderSchemaUse;
pub use schema_provider::ResourceSchemaOracle;
pub use types::{ResourceRef, ValueKind, YamlPath, ordered_api_versions_for_resource};
