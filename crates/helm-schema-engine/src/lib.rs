pub mod compatibility {
    pub use helm_schema_ir::{ContractDocumentV1, ContractProjection, ValueUse};
}

pub mod helpers {
    pub use helm_schema_ast::DefineIndex;
    pub use helm_schema_ir::{DefineBlock, extract_define_blocks, extract_helper_calls};
}

pub mod parse {
    pub use helm_schema_ast::{
        HelmAst, HelmParser, ParseError, TreeSitterParser, contains_template_action,
        extract_values_yaml_descriptions,
    };
}

pub use helm_schema_core::{
    ApiPresenceQuery, CapabilityGuard, CapabilityOracle, CapabilityPresencePredicate, HelperBranch,
    HelperBranchBody, ProviderSchemaUse, ResourceRef, ResourceSchemaOracle, StaticOracle,
    ValueKind, YamlPath, evaluate_guard, live_literals, ordered_api_versions_for_resource,
    select_live_branch,
};
pub use helm_schema_gen::{ValuesSchemaInput, generate_values_schema};
pub use helm_schema_ir::{
    ConditionalGuard, ConditionalPathOverlay, ContractIr, ContractPathSignals, ContractProvenance,
    ContractSchemaSignals, ContractUse, ContractValuePathFacts, Guard, GuardConstraint,
    MetadataFieldKind, RequiredInferenceSignals, SourceSpan, SymbolicIrContext,
    extract_default_type_hints,
};

pub mod required_inference {
    pub use helm_schema_gen::required_inference::apply_required_inference;
    pub use helm_schema_ir::required_inference::extract_default_fallback_paths;
}
