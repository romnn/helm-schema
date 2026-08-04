//! Symbolic Helm interpretation and normalized contract IR.

mod abstract_value;
mod analysis_db;
mod bound_value_analysis;
mod contract;
mod contract_normalization;
mod contract_signal_builder;
mod eval_effect;
mod eval_env;
mod expr_call_eval;
mod expr_eval;
mod fragment_assignment;
pub mod fragment_eval;
mod fragment_expr_eval;
mod function_semantics;
mod helper_literal_dispatch;
mod helper_meta;
mod node_eval;
mod range_modes;
mod resource_identity;
mod scalar_value;
mod static_file_template;
mod symbolic;
mod symbolic_local_state;
mod value_path_context;

pub use analysis_db::define_bodies_in_source;
pub use contract::{ContractDocument, ContractIr, ContractUse, FinalizedContract};
#[doc(hidden)]
pub use helm_schema_core::escape_regex_literal;
pub use helm_schema_core::{
    CapabilityGuard, ConditionalGuard, ConditionalPathOverlay, ContractPathSchemaEvidence,
    ContractProvenance, ContractSchemaSignals, ContractValuePathFacts, Guard, GuardValue,
    HelperBranch, HelperBranchBody, MetadataFieldKind, ProviderSchemaUse, ResourceRef, SourceSpan,
    ValueKind, ValuesDefaultSource, YamlPath,
};
pub use symbolic::{SymbolicIrContext, SymbolicPolicy};

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
