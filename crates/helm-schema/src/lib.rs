//! Library facade for chart analysis and Helm values schema generation.

mod analysis;
mod chart;
mod error;
mod fetch_policy;
/// JSON Schema reference bundling and inlining.
pub mod flatten;
/// Schema-generation inputs and staged output artifacts.
pub mod generation;
mod load_budget;
mod output_pipeline;
mod provider_builder;
/// Deterministic merge policy for caller-supplied override schemas.
pub mod schema_override;
mod session;
mod values_roots;

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

/// Runtime diagnostics produced by Kubernetes and CRD schema lookup.
pub mod diagnostics {
    pub use helm_schema_k8s::{
        Diagnostic, DiagnosticKey, DiagnosticSink, format_diagnostic_json, format_diagnostic_text,
    };
}

/// Stable inspection types for the recovered Helm contract.
pub mod contract {
    pub use helm_schema_ir::{
        ContractDocument, ContractProvenance, ContractUse, Guard, SourceSpan, ValueKind,
    };
}

/// Final schema output policy and serialization APIs.
pub mod output {
    pub use crate::fetch_policy::FetchPolicy;
    pub use crate::load_budget::LoadBudget;
    pub use crate::output_pipeline::{
        JsonOutputFormat, OutputPipelineOptions, PolicyInputOptions, PolicyInputs, ReferenceMode,
        apply_schema_output_pipeline, load_policy_inputs, write_schema_json,
    };
}

/// Kubernetes and CRD provider configuration types.
pub mod provider {
    pub use crate::provider_builder::ProviderOptions;
    pub use helm_schema_k8s::{K8sVersionChain, LocalSchemaUniverse};
}

pub use session::{Analysis, AnalysisSession, ValuePathExplanation};

pub use error::{CliError, EngineResult};
pub use generation::{GenerateOptions, GeneratedSchema, ResolvedContract};
