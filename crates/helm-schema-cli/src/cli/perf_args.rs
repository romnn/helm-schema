use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug, Clone, Default)]
pub struct PerfArgs {
    /// Write a Perfetto-readable trace file for the run.
    #[arg(long = "trace-output")]
    pub trace_output: Option<PathBuf>,
}
