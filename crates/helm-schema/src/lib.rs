mod analysis;
mod chart;
mod error;
mod fetch_policy;
pub mod flatten;
pub mod generation;
mod load_budget;
mod output_pipeline;
mod provider_builder;
mod required_inference;
pub mod schema_override;
mod session;
mod values_roots;

#[cfg(test)]
mod tests;

pub mod diagnostics {
    pub use helm_schema_k8s::{
        Diagnostic, DiagnosticKey, DiagnosticSink, format_diagnostic_json, format_diagnostic_text,
    };
}

pub mod contract {
    pub use helm_schema_ir::{
        ContractDocument, ContractDocumentGuard, ContractDocumentProvenance, ContractDocumentSpan,
        ContractDocumentUse, ValueKind,
    };
}

pub mod output {
    pub use crate::fetch_policy::FetchPolicy;
    pub use crate::load_budget::LoadBudget;
    pub use crate::output_pipeline::{
        JsonOutputFormat, OutputPipelineOptions, PolicyInputOptions, PolicyInputs, ReferenceMode,
        apply_schema_output_pipeline, load_policy_inputs, write_schema_json,
    };
}

pub mod provider {
    pub use crate::provider_builder::ProviderOptions;
    pub use helm_schema_k8s::K8sVersionChain;
}

pub use session::{Analysis, AnalysisSession, ValuePathExplanation};

pub use error::{CliError, CliResult};
pub use generation::{GenerateOptions, GeneratedSchema, ResolvedContract};
