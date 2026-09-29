use std::collections::BTreeMap;
use std::path::PathBuf;

use helm_schema_json_schema_minify::DefinitionOrigin;
use serde_json::Value;
use vfs::VfsPath;

use crate::provider_builder::ProviderOptions;

pub use helm_schema_gen::{
    ConditionalAnchors, EmissionClassKind, EmissionPolicy, EmissionPolicyDelta, EmissionReport,
    EmissionSelection, InvalidEmissionPolicy, LintDocument, LintOutcome, LintWithdrawal,
    ResolvedEmissionPolicy, SchemaProfile,
};

/// Inputs and analysis policy for generating one chart schema.
#[derive(Debug, Clone)]
pub struct GenerateOptions {
    /// Virtual-filesystem directory containing `Chart.yaml`.
    pub chart_dir: VfsPath,
    /// Whether templates under the chart's test directories are analyzed.
    pub include_tests: bool,
    /// Whether dependency values are exposed beneath their subchart keys.
    pub include_subchart_values: bool,
    /// Additional values files applied after the chart defaults.
    pub values_files: Vec<PathBuf>,
    /// Whether the optional required-property heuristic runs.
    pub infer_required: bool,
    /// Amount of analyzed contract evidence emitted into the schema.
    pub emission: EmissionSelection,
    /// Kubernetes and CRD schema-provider policy.
    pub provider: ProviderOptions,
}

/// Provider-resolved values contract prior to heuristic required-inference
/// and output-pipeline emission transforms.
///
/// The resolved contract contains facts inferred from templates, helpers,
/// composed values defaults/descriptions, and provider schemas. The later
/// `GeneratedSchema` stage is reserved for additional synthesized mutations
/// like the optional `--infer-required` heuristic.
#[derive(Debug, Clone)]
pub struct ResolvedContract {
    /// JSON Schema lowered from structural contract evidence.
    pub schema: Value,
    /// Fact and carrier accounting from schema emission.
    pub emission_report: EmissionReport,
    /// Content origins of the private definition handles in `schema`, named
    /// by the output pipeline after minimization.
    pub definition_origins: BTreeMap<String, Vec<DefinitionOrigin>>,
    /// The per-path decisions the emitter run took while producing `schema`.
    pub generation_decisions: helm_schema_gen::GenerationDecisions,
}

/// Final schema after optional generation transforms.
#[derive(Debug, Clone)]
pub struct GeneratedSchema {
    /// Final generated JSON Schema.
    pub schema: Value,
    /// Fact and carrier accounting from schema emission.
    pub emission_report: EmissionReport,
    /// Content origins of the private definition handles in `schema`, named
    /// by the output pipeline after minimization.
    pub definition_origins: BTreeMap<String, Vec<DefinitionOrigin>>,
}
