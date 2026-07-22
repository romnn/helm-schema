use std::path::PathBuf;

use serde_json::Value;
use vfs::VfsPath;

use crate::provider_builder::ProviderOptions;

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
    /// Values prefixes owned by discovered subcharts.
    pub subchart_value_prefixes: Vec<Vec<String>>,
}

/// Final schema and subchart metadata after optional generation transforms.
#[derive(Debug, Clone)]
pub struct GeneratedSchema {
    /// Final generated JSON Schema.
    pub schema: Value,
    /// Values prefixes owned by discovered subcharts.
    pub subchart_value_prefixes: Vec<Vec<String>>,
}
