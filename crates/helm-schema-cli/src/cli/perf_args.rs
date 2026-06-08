use std::path::PathBuf;

use clap::Args;

#[derive(Args, Debug, Clone, Default)]
pub struct PerfArgs {
    /// Emit coarse phase timings to stderr after a successful run.
    #[arg(long = "profile-phases")]
    pub profile_phases: bool,

    /// Write a Perfetto-readable trace file for the profiled run.
    #[arg(long = "trace-output")]
    pub trace_output: Option<PathBuf>,
}
