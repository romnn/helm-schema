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
mod helper_literal_dispatch;
mod helper_meta;
mod node_eval;
mod resource_identity;
mod static_file_template;
mod symbolic;
mod symbolic_local_state;
mod value_path_context;

pub use contract::{ContractDocument, ContractIr, ContractUse, FinalizedContract};
pub use helm_schema_core::{
    CapabilityGuard, ConditionalGuard, ConditionalPathOverlay, ContractPathSchemaEvidence,
    ContractProvenance, ContractSchemaSignals, ContractValuePathFacts, Guard, GuardValue,
    HelperBranch, HelperBranchBody, MetadataFieldKind, ProviderSchemaUse, ResourceRef, SourceSpan,
    ValueKind, YamlPath,
};
pub use symbolic::SymbolicIrContext;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
