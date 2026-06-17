mod analysis;
mod chart;
mod chart_evidence;
mod error;
mod fetch_policy;
pub mod flatten;
pub mod generation;
mod output_pipeline;
mod provider_builder;
mod required_inference;
pub mod schema_override;
mod session;
mod values_roots;

pub mod diagnostics {
    pub use helm_schema_k8s::{
        Diagnostic, DiagnosticKey, DiagnosticSink, format_diagnostic_json, format_diagnostic_text,
    };
}

pub mod output {
    pub use crate::fetch_policy::FetchPolicy;
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
pub use generation::{
    GenerateOptions, GeneratedSchema, ResolvedContract, generate_values_schema_for_chart,
    generate_values_schema_for_chart_output, generate_values_schema_for_chart_with_diagnostics,
};
