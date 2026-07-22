mod chart_args;
mod crd_args;
mod diag_args;
mod inference_args;
mod k8s_args;
mod output_args;
mod perf_args;

use std::path::PathBuf;

use clap::Parser;

pub use chart_args::ChartArgs;
pub use crd_args::{CrdArgs, CrdVersionLookup};
pub use diag_args::{DiagArgs, DiagFormat};
pub use inference_args::InferenceArgs;
pub use k8s_args::{DEFAULT_AUTO_WINDOW, K8sArgs, K8sVersionFallback};
pub use output_args::OutputArgs;
pub use perf_args::PerfArgs;

/// Complete command-line interface for one schema-generation invocation.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "helm-schema",
    about = "Generate JSON schema for Helm values.yaml"
)]
pub struct Cli {
    /// Chart directory or packaged chart archive to analyze.
    #[arg(value_name = "CHART_DIR")]
    pub chart_dir: PathBuf,

    /// Final output and reference-processing options.
    #[command(flatten)]
    pub output: OutputArgs,

    /// Kubernetes schema source and version options.
    #[command(flatten)]
    pub k8s: K8sArgs,

    /// CRD schema source and lookup options.
    #[command(flatten)]
    pub crd: CrdArgs,

    /// API-version inference options.
    #[command(flatten)]
    pub inference: InferenceArgs,

    /// Runtime diagnostic formatting options.
    #[command(flatten)]
    pub diag: DiagArgs,

    /// Chart discovery and values-composition options.
    #[command(flatten)]
    pub chart: ChartArgs,

    /// Performance tracing options.
    #[command(flatten)]
    pub perf: PerfArgs,

    /// Schema files to merge on top of the inferred output, applied in
    /// the order given. Repeatable: pass multiple `--override-schema`
    /// flags to layer (e.g. a shared cross-chart top-level schema
    /// followed by a chart-specific override).
    #[arg(long)]
    pub override_schema: Vec<PathBuf>,
}
