mod chart_args;
mod crd_args;
mod diag_args;
mod emission_args;
mod inference_args;
mod k8s_args;
mod output_args;
mod perf_args;
mod profile_args;

use std::path::PathBuf;

use clap::Parser;

pub use chart_args::ChartArgs;
pub use crd_args::{CrdArgs, CrdVersionLookup};
pub use diag_args::{DiagArgs, DiagFormat};
pub use emission_args::{EmissionArgs, PolicyToggle};
pub use inference_args::InferenceArgs;
pub use k8s_args::{DEFAULT_AUTO_WINDOW, K8sArgs, K8sVersionFallback};
pub use output_args::OutputArgs;
pub use perf_args::PerfArgs;
pub use profile_args::SchemaProfile;

/// Complete command-line interface for one schema-generation invocation.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "helm-schema",
    about = "Generate JSON schema for Helm values.yaml",
    after_long_help = "EMISSION RETENTION:\n  full  keeps mandatory facts, root/local ordinary conditionals, terminal clauses,\n        and kind partitions.\n  lean  keeps every mandatory fact and every locally anchored ordinary conditional;\n        it drops root-anchored ordinary conditionals, terminal clauses, and kind\n        partitions.\n\nMandatory facts cannot be disabled. The four emission override flags only change\nW-class refinements and therefore only widen acceptance when switched off."
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

    /// Read policy from this file instead of discovering `helm-schema.yaml`.
    /// Relative paths are resolved from the invocation working directory.
    #[arg(long, value_name = "PATH", conflicts_with = "no_config")]
    pub config: Option<PathBuf>,

    /// Ignore both discovered and explicit chart policy configuration.
    #[arg(long, conflicts_with = "config")]
    pub no_config: bool,

    /// Print resolved policy values and their sources without analyzing the chart.
    #[arg(long)]
    pub print_effective_config: bool,

    /// Select how much analyzed contract evidence is emitted.
    ///
    /// `lean` retains mandatory and locally anchored ordinary facts. It drops
    /// root ordinary conditionals, terminals, and kind partitions, so it only
    /// widens acceptance while reducing Helm's schema compilation cost.
    #[arg(long, value_enum)]
    pub profile: Option<SchemaProfile>,

    /// W-class emission-policy overrides.
    #[command(flatten)]
    pub emission: EmissionArgs,

    /// Schema files to merge on top of the inferred output, applied in
    /// the order given. Repeatable: pass multiple `--override-schema`
    /// flags to layer (e.g. a shared cross-chart top-level schema followed by
    /// a chart-specific override). Overrides must carry their own definitions
    /// rather than reference helm-schema's private `$defs` names.
    #[arg(long)]
    pub override_schema: Vec<PathBuf>,
}
