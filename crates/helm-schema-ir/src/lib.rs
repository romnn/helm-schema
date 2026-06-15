mod abstract_document;
mod abstract_document_hole;
mod abstract_value;
mod assignment_action_plan;
mod binding;
mod bound_helper_call_analysis;
mod bound_value_analysis;
mod capability_branch;
mod chart_facts;
mod compatibility;
mod condition_action_plan;
mod condition_guards;
mod contract;
mod contract_normalization;
mod contract_signal_builder;
mod contract_signals;
mod contract_sink;
mod default_type_hints;
mod define_body_cache;
mod document_helper_contract;
mod document_hole_context;
mod document_value_analysis;
mod eval_effect;
mod eval_env;
mod expr_call_eval;
mod expr_eval;
#[cfg(test)]
mod expr_eval_tests;
mod expr_function_catalog;
mod expr_pipeline_eval;
mod expression_analysis;
mod fragment_assignment;
mod fragment_classification;
mod fragment_expr_eval;
mod fragment_range_scope;
#[cfg(test)]
mod fragment_scope_eval_tests;
mod helper_analysis;
mod helper_analysis_mutation;
mod helper_analysis_projection;
mod helper_arg_projection;
mod helper_aware_expr_eval;
mod helper_binding_projection;
mod helper_body_analysis;
mod helper_discovery;
mod helper_fragment_output_uses;
mod helper_inline;
mod helper_output_projection;
mod helper_summary;
mod helper_value_analysis;
mod helper_value_expression;
mod helper_walk_state;
mod local_projection;
mod node_action_effect;
mod node_action_kind;
mod node_eval;
mod output_path;
mod predicate;
mod printf_eval;
mod range_action_plan;
mod rendered_yaml_context;
pub mod required_inference;
mod resource_identity;
mod static_file_template;
mod symbolic;
mod symbolic_local_state;
mod symbolic_scope_state;
mod template_comment_filter;
mod template_expr_analysis;
mod template_expr_cache;
mod tree_sitter_utils;
mod value_path_context;
mod value_path_extraction;
mod yaml_shape;

pub use capability_branch::{CapabilityGuard, HelperBranch, HelperBranchBody};
pub use chart_facts::{ChartFacts, PathFact};
pub use compatibility::{Guard, ResourceRef, ValueKind, ValueUse, YamlPath};
pub use contract::{ContractIr, ContractProjection, ContractUse};
pub use contract_signals::{
    ContractPathSignals, ContractSchemaSignals, ContractValuePathFacts, GuardConstraint,
    MetadataFieldKind, ProviderSchemaUse, RequiredInferenceSignals,
};
pub use default_type_hints::extract_default_type_hints;
pub use helper_discovery::{DefineBlock, extract_define_blocks, extract_helper_calls};
pub use symbolic::SymbolicIrContext;

#[cfg(test)]
mod tests;
