mod abstract_value;
mod assignment_action_plan;
mod bound_value_analysis;
mod capability_branch;
mod condition_action_plan;
mod condition_guards;
mod contract;
mod contract_normalization;
mod contract_signal_builder;
mod contract_signals;
mod contract_sink;
mod contract_types;
mod define_body_cache;
mod document_projection;
mod eval_effect;
mod eval_env;
mod expr_call_eval;
mod expr_eval;
mod expr_function_catalog;
mod expr_pipeline_eval;
mod fragment_assignment;
mod fragment_expr_eval;
mod fragment_range_scope;
mod helper_arg_projection;
mod helper_body_analysis;
mod helper_discovery;
mod helper_fragment_output_uses;
mod helper_inline;
mod helper_range_frame;
mod helper_range_plan;
mod helper_runtime_guards;
mod helper_runtime_plan;
mod helper_summary;
mod helper_value_expression;
mod helper_walk_state;
mod literal_schema_type;
mod node_eval;
mod output_path;
mod predicate;
mod printf_eval;
mod provenance;
mod range_action_plan;
mod resource_identity;
mod static_file_template;
mod symbolic;
mod symbolic_local_state;
mod symbolic_scope_state;
mod template_expr_cache;
mod tree_sitter_utils;
mod value_path_context;
mod value_path_extraction;
mod yaml_syntax;

pub use contract::{ContractDocument, ContractIr, ContractUse, FinalizedContract};
pub use contract_signals::{
    ConditionalGuard, ConditionalOverlayEvidence, ConditionalPathOverlay,
    ContractPathSchemaEvidence, ContractRequirednessEvidence, ContractSchemaSignals,
    ContractValuePathFacts, MetadataFieldKind,
};
pub use contract_types::{Guard, GuardValue};
pub use helm_schema_core::{
    ApiPresenceQuery, CapabilityGuard, CapabilityOracle, CapabilityPresencePredicate, HelperBranch,
    HelperBranchBody, ProviderSchemaUse, ResourceRef, StaticOracle, ValueKind, YamlPath,
    evaluate_guard, live_literals, select_live_branch,
};
pub use helper_discovery::{
    DefineBlock, extract_define_blocks, extract_helper_calls, extract_helper_calls_from_ast,
    extract_helper_calls_from_ast_body, extract_helper_calls_from_ast_excluding_defines,
};
pub use provenance::{ContractProvenance, SourceSpan};
pub use symbolic::SymbolicIrContext;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
