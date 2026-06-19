use std::path::PathBuf;

use serde_json::Value;
use vfs::VfsPath;

use crate::provider_builder::ProviderOptions;

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub chart_dir: VfsPath,
    pub include_tests: bool,
    pub include_subchart_values: bool,
    pub values_files: Vec<PathBuf>,
    pub infer_required: bool,
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
    pub schema: Value,
    pub subchart_value_prefixes: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct GeneratedSchema {
    pub schema: Value,
    pub subchart_value_prefixes: Vec<Vec<String>>,
}
