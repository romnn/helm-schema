//! Symbolic Helm interpretation and normalized contract IR.

mod abstract_value;
mod analysis_db;
mod contract;
mod contract_normalization;
mod contract_signal_builder;
pub(crate) mod fragment_eval;
mod function_semantics;
mod helper_meta;
mod node_eval;
mod observed_facts;
mod range_modes;
mod resource_identity;
use value::scalar_value;
mod symbolic;
mod value;

pub use analysis_db::ParsedDefines;
pub use contract::{
    ContractDocument, ContractIr, ContractUse, DependencyValuesRoot, FinalizedContract,
};
#[doc(hidden)]
pub use helm_schema_core::escape_regex_literal;
pub use helm_schema_core::{
    CapabilityGuard, ConditionalGuard, ConditionalPathOverlay, ContractPathSchemaEvidence,
    ContractProvenance, ContractSchemaSignals, ContractValuePathFacts, Guard, GuardValue,
    GuardedValuesDefaultSource, HelperBranch, HelperBranchBody, MetadataFieldKind,
    ProviderSchemaUse, RangeDomain, ResourceRef, SourceSpan, ValueKind, ValuesDefaultSource,
    YamlPath,
};
pub use symbolic::{SymbolicIrContext, SymbolicPolicy};

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
