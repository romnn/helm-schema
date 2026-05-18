mod chart_args;
mod crd_args;
mod diag_args;
mod inference_args;
mod k8s_args;
mod output_args;

use std::path::PathBuf;

use clap::Parser;

pub use chart_args::ChartArgs;
pub use crd_args::{CrdArgs, CrdVersionLookup};
pub use diag_args::{DiagArgs, DiagFormat};
pub use inference_args::InferenceArgs;
pub use k8s_args::{DEFAULT_AUTO_WINDOW, K8sArgs, K8sVersionFallback};
pub use output_args::OutputArgs;

#[derive(Parser, Debug, Clone)]
#[command(
    name = "helm-schema",
    about = "Generate JSON schema for Helm values.yaml"
)]
pub struct Cli {
    #[arg(value_name = "CHART_DIR")]
    pub chart_dir: PathBuf,

    #[command(flatten)]
    pub output: OutputArgs,

    #[command(flatten)]
    pub k8s: K8sArgs,

    #[command(flatten)]
    pub crd: CrdArgs,

    #[command(flatten)]
    pub inference: InferenceArgs,

    #[command(flatten)]
    pub diag: DiagArgs,

    #[command(flatten)]
    pub chart: ChartArgs,

    #[arg(long)]
    pub override_schema: Option<PathBuf>,
}
